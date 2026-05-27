"""
动态市场路由器 - 解决 Polymarket 5 分钟市场轮动问题

核心功能:
1. 监控当前周期剩余时间
2. 提前 60 秒预热下一个市场的 Token ID
3. 无缝切换数据流和交易流
4. 收盘前 30 秒触发决策，最后 5 秒禁止开仓
"""

import asyncio
import time
from datetime import datetime, timedelta
from typing import Optional, Dict, Any, Callable
from dataclasses import dataclass, field
from enum import Enum


class MarketState(Enum):
    """市场状态枚举"""
    PREHEAT = "preheat"          # 预热阶段 (剩余>60 秒)
    ACTIVE = "active"            # 活跃交易阶段 (60 秒>=剩余>30 秒)
    DECISION = "decision"        # 决策阶段 (30 秒>=剩余>5 秒)
    COOLDOWN = "cooldown"        # 冷却阶段 (剩余<=5 秒，禁止开仓)
    SETTLEMENT = "settlement"    # 结算阶段 (周期结束)


@dataclass
class MarketInfo:
    """市场信息数据类"""
    token_id: str
    condition_id: str
    question: str
    start_time: float
    end_time: float
    state: MarketState = MarketState.PREHEAT
    
    @property
    def time_remaining(self) -> float:
        """计算剩余时间 (秒)"""
        return max(0, self.end_time - time.time())
    
    @property
    def elapsed_time(self) -> float:
        """计算已过去时间 (秒)"""
        return min(self.end_time - self.start_time, time.time() - self.start_time)
    
    @property
    def total_duration(self) -> float:
        """总持续时间 (秒)"""
        return self.end_time - self.start_time


@dataclass
class MarketCycle:
    """完整的市场周期，包含当前和下一个市场"""
    current: Optional[MarketInfo] = None
    next_market: Optional[MarketInfo] = None
    last_switch_time: float = field(default_factory=time.time)


class MarketRouter:
    """
    动态市场路由器
    
    负责管理 Polymarket 5 分钟 BTC 涨跌预测市场的生命周期:
    - 自动发现新市场
    - 预热连接
    - 状态监控
    - 无缝切换
    """
    
    def __init__(self, gamma_api_base: str = "https://gamma-api.polymarket.com"):
        self.gamma_api_base = gamma_api_base
        self.current_cycle = MarketCycle()
        self.state_callbacks: Dict[MarketState, list] = {state: [] for state in MarketState}
        self._running = False
        self._monitor_task: Optional[asyncio.Task] = None
        
        # 配置参数
        self.preheat_seconds = 60      # 提前预热时间
        self.decision_seconds = 30     # 决策触发时间
        self.cooldown_seconds = 5      # 冷却时间 (禁止开仓)
        
    def register_state_callback(self, state: MarketState, callback: Callable[[MarketInfo], None]):
        """注册状态变化回调函数"""
        self.state_callbacks[state].append(callback)
    
    async def _fetch_next_market_info(self) -> Optional[MarketInfo]:
        """
        从 Gamma API 获取下一个市场信息
        
        实际实现需要调用 Polymarket Gamma API
        这里使用模拟逻辑
        """
        try:
            # TODO: 实现真实的 Gamma API 调用
            # endpoint = f"{self.gamma_api_base}/events?condition_id=<BTC_condition_id>"
            
            # 模拟逻辑：计算下一个 5 分钟周期的时间
            now = time.time()
            # 对齐到 5 分钟边界
            next_start = ((int(now) // 300) + 1) * 300
            next_end = next_start + 300
            
            return MarketInfo(
                token_id=f"btc-up-{next_start}",  # 模拟 Token ID
                condition_id="btc-5min-condition",  # 模拟 Condition ID
                question=f"Will BTC go up in the next 5 minutes?",
                start_time=next_start,
                end_time=next_end,
                state=MarketState.PREHEAT
            )
        except Exception as e:
            print(f"Error fetching market info: {e}")
            return None
    
    def _update_market_state(self, market: MarketInfo) -> MarketState:
        """根据剩余时间更新市场状态"""
        remaining = market.time_remaining
        
        if remaining <= 0:
            return MarketState.SETTLEMENT
        elif remaining <= self.cooldown_seconds:
            return MarketState.COOLDOWN
        elif remaining <= self.decision_seconds:
            return MarketState.DECISION
        elif remaining <= self.preheat_seconds:
            return MarketState.ACTIVE
        else:
            return MarketState.PREHEAT
    
    async def _notify_state_change(self, market: MarketInfo, new_state: MarketState):
        """通知状态变化"""
        if market.state != new_state:
            old_state = market.state
            market.state = new_state
            print(f"[MarketRouter] State changed: {old_state.value} -> {new_state.value} "
                  f"(Token: {market.token_id}, Remaining: {market.time_remaining:.1f}s)")
            
            for callback in self.state_callbacks[new_state]:
                try:
                    if asyncio.iscoroutinefunction(callback):
                        await callback(market)
                    else:
                        callback(market)
                except Exception as e:
                    print(f"Error in state callback: {e}")
    
    async def _monitor_cycle(self):
        """监控市场周期变化"""
        while self._running:
            if self.current_cycle.current:
                # 更新当前市场状态
                new_state = self._update_market_state(self.current_cycle.current)
                await self._notify_state_change(self.current_cycle.current, new_state)
                
                # 检查是否需要切换到下一个市场
                if new_state == MarketState.SETTLEMENT:
                    await self._switch_to_next_market()
                
                # 检查是否需要预热下一个市场
                elif (new_state in [MarketState.ACTIVE, MarketState.DECISION] and 
                      self.current_cycle.next_market is None):
                    next_market = await self._fetch_next_market_info()
                    if next_market:
                        self.current_cycle.next_market = next_market
                        print(f"[MarketRouter] Preheated next market: {next_market.token_id}")
            
            else:
                # 初始化第一个市场
                market = await self._fetch_next_market_info()
                if market:
                    self.current_cycle.current = market
                    print(f"[MarketRouter] Initialized market: {market.token_id}")
            
            await asyncio.sleep(1)  # 每秒检查一次
    
    async def _switch_to_next_market(self):
        """切换到下一个市场"""
        if self.current_cycle.next_market:
            print(f"[MarketRouter] Switching to next market: {self.current_cycle.next_market.token_id}")
            self.current_cycle.current = self.current_cycle.next_market
            self.current_cycle.next_market = None
            self.current_cycle.last_switch_time = time.time()
            
            # 立即预再下一个市场
            next_market = await self._fetch_next_market_info()
            if next_market:
                self.current_cycle.next_market = next_market
    
    async def start(self):
        """启动市场路由器"""
        self._running = True
        self._monitor_task = asyncio.create_task(self._monitor_cycle())
        print("[MarketRouter] Started monitoring market cycles")
    
    async def stop(self):
        """停止市场路由器"""
        self._running = False
        if self._monitor_task:
            self._monitor_task.cancel()
            try:
                await self._monitor_task
            except asyncio.CancelledError:
                pass
        print("[MarketRouter] Stopped")
    
    def get_current_market(self) -> Optional[MarketInfo]:
        """获取当前市场信息"""
        return self.current_cycle.current
    
    def get_next_market(self) -> Optional[MarketInfo]:
        """获取下一个市场信息"""
        return self.current_cycle.next_market
    
    def can_open_position(self) -> bool:
        """判断是否可以开仓 (不在冷却或结算阶段)"""
        if not self.current_cycle.current:
            return False
        state = self.current_cycle.current.state
        return state not in [MarketState.COOLDOWN, MarketState.SETTLEMENT]
    
    def should_make_decision(self) -> bool:
        """判断是否应该做出决策 (决策阶段)"""
        if not self.current_cycle.current:
            return False
        return self.current_cycle.current.state == MarketState.DECISION
    
    def get_time_until_decision(self) -> float:
        """获取距离决策阶段的时间 (秒)"""
        if not self.current_cycle.current:
            return float('inf')
        remaining = self.current_cycle.current.time_remaining
        return max(0, remaining - self.decision_seconds)


# 测试代码
if __name__ == "__main__":
    async def test_market_router():
        router = MarketRouter()
        
        # 注册状态回调
        def on_decision(market: MarketInfo):
            print(f"*** DECISION TIME! Token: {market.token_id}, Time remaining: {market.time_remaining:.1f}s ***")
        
        router.register_state_callback(MarketState.DECISION, on_decision)
        
        # 启动路由器
        await router.start()
        
        # 运行 10 秒观察
        for i in range(10):
            market = router.get_current_market()
            if market:
                print(f"[{i}] Market: {market.token_id}, State: {market.state.value}, "
                      f"Remaining: {market.time_remaining:.1f}s, Can Open: {router.can_open_position()}")
            await asyncio.sleep(1)
        
        await router.stop()
    
    # asyncio.run(test_market_router())
    print("MarketRouter module loaded successfully")
