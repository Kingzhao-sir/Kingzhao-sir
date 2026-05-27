"""
数据摄取网关 - 多源市场数据融合

核心功能:
1. 连接 Binance WebSocket 获取 BTC/USDT 实时价格和订单簿
2. 连接 Polymarket WebSocket 获取支持率 (隐含概率)
3. 计算 1 分钟/5 分钟 K 线
4. 统一数据格式，通过回调分发给策略引擎
"""

import asyncio
import json
import time
from datetime import datetime
from typing import Optional, Dict, Any, Callable, List
from dataclasses import dataclass, field
from enum import Enum


class DataSource(Enum):
    """数据源枚举"""
    BINANCE = "binance"
    POLYMARKET = "polymarket"


@dataclass
class TickData:
    """Tick 级数据"""
    timestamp: float
    source: DataSource
    symbol: str
    price: float
    quantity: float = 0.0
    side: str = ""  # "buy" or "sell"
    
    
@dataclass
class OrderBookLevel:
    """订单簿层级"""
    price: float
    quantity: float
    

@dataclass
class OrderBook:
    """完整订单簿"""
    timestamp: float
    symbol: str
    bids: List[OrderBookLevel] = field(default_factory=list)
    asks: List[OrderBookLevel] = field(default_factory=list)
    
    def best_bid(self) -> Optional[float]:
        """获取最优买价"""
        return self.bids[0].price if self.bids else None
    
    def best_ask(self) -> Optional[float]:
        """获取最优卖价"""
        return self.asks[0].price if self.asks else None
    
    def mid_price(self) -> Optional[float]:
        """获取中间价"""
        bid = self.best_bid()
        ask = self.best_ask()
        if bid and ask:
            return (bid + ask) / 2
        return None
    
    def spread(self) -> Optional[float]:
        """获取价差"""
        bid = self.best_bid()
        ask = self.best_ask()
        if bid and ask:
            return ask - bid
        return None


@dataclass
class KlineData:
    """K 线数据"""
    timestamp: float
    symbol: str
    interval: str  # "1m" or "5m"
    open: float
    high: float
    low: float
    close: float
    volume: float = 0.0
    trade_count: int = 0


@dataclass
class PolymarketSupportRate:
    """Polymarket 支持率数据"""
    timestamp: float
    token_id: str
    yes_price: float  # Yes 份额价格 (0-1)，即隐含概率
    no_price: float   # No 份额价格
    volume_24h: float = 0.0
    open_interest: float = 0.0


class DataGateway:
    """
    数据摄取网关
    
    负责从多个数据源获取实时市场数据，并统一格式后分发给订阅者
    """
    
    def __init__(self):
        # 数据回调
        self.tick_callbacks: List[Callable[[TickData], None]] = []
        self.orderbook_callbacks: List[Callable[[OrderBook], None]] = []
        self.kline_callbacks: List[Callable[[KlineData], None]] = []
        self.support_rate_callbacks: List[Callable[[PolymarketSupportRate], None]] = []
        
        # 运行状态
        self._running = False
        self._tasks: List[asyncio.Task] = []
        
        # K 线构建器
        self.kline_builders: Dict[str, 'KlineBuilder'] = {}
        
        # 最新数据缓存
        self.latest_tick: Optional[TickData] = None
        self.latest_orderbook: Optional[OrderBook] = None
        self.latest_support_rate: Optional[PolymarketSupportRate] = None
        
    def register_tick_callback(self, callback: Callable[[TickData], None]):
        """注册 Tick 数据回调"""
        self.tick_callbacks.append(callback)
    
    def register_orderbook_callback(self, callback: Callable[[OrderBook], None]):
        """注册订单簿回调"""
        self.orderbook_callbacks.append(callback)
    
    def register_kline_callback(self, callback: Callable[[KlineData], None]):
        """注册 K 线回调"""
        self.kline_callbacks.append(callback)
    
    def register_support_rate_callback(self, callback: Callable[[PolymarketSupportRate], None]):
        """注册支持率回调"""
        self.support_rate_callbacks.append(callback)
    
    async def _notify_tick(self, tick: TickData):
        """通知 Tick 数据"""
        self.latest_tick = tick
        for callback in self.tick_callbacks:
            try:
                if asyncio.iscoroutinefunction(callback):
                    await callback(tick)
                else:
                    callback(tick)
            except Exception as e:
                print(f"[DataGateway] Error in tick callback: {e}")
    
    async def _notify_orderbook(self, ob: OrderBook):
        """通知订单簿数据"""
        self.latest_orderbook = ob
        for callback in self.orderbook_callbacks:
            try:
                if asyncio.iscoroutinefunction(callback):
                    await callback(ob)
                else:
                    callback(ob)
            except Exception as e:
                print(f"[DataGateway] Error in orderbook callback: {e}")
    
    async def _notify_kline(self, kline: KlineData):
        """通知 K 线数据"""
        for callback in self.kline_callbacks:
            try:
                if asyncio.iscoroutinefunction(callback):
                    await callback(kline)
                else:
                    callback(kline)
            except Exception as e:
                print(f"[DataGateway] Error in kline callback: {e}")
    
    async def _notify_support_rate(self, sr: PolymarketSupportRate):
        """通知支持率数据"""
        self.latest_support_rate = sr
        for callback in self.support_rate_callbacks:
            try:
                if asyncio.iscoroutinefunction(callback):
                    await callback(sr)
                else:
                    callback(sr)
            except Exception as e:
                print(f"[DataGateway] Error in support rate callback: {e}")
    
    async def _connect_binance_ws(self):
        """连接 Binance WebSocket - 模拟模式"""
        print(f"[DataGateway] Simulating Binance WebSocket connection")
        
        # 模拟模式：生成模拟数据
        import random
        base_price = 67000.0
        
        while self._running:
            # 模拟 Tick 数据
            price_change = random.uniform(-50, 50)
            base_price += price_change
            tick = TickData(
                timestamp=time.time(),
                source=DataSource.BINANCE,
                symbol="BTCUSDT",
                price=base_price,
                quantity=random.uniform(0.01, 2.0),
                side="buy" if random.random() > 0.5 else "sell"
            )
            await self._notify_tick(tick)
            
            # 模拟订单簿
            ob = OrderBook(
                timestamp=time.time(),
                symbol="BTCUSDT",
                bids=[OrderBookLevel(base_price - i * 0.5, random.uniform(1, 10)) for i in range(10)],
                asks=[OrderBookLevel(base_price + i * 0.5 + 0.5, random.uniform(1, 10)) for i in range(10)]
            )
            await self._notify_orderbook(ob)
            
            await asyncio.sleep(0.5)  # 每 0.5 秒更新一次
    
    async def _connect_polymarket_ws(self, token_id: str):
        """连接 Polymarket WebSocket - 模拟模式"""
        print(f"[DataGateway] Simulating Polymarket WS for token: {token_id}")
        
        while self._running:
            # 模拟支持率数据
            import random
            yes_price = 0.5 + random.uniform(-0.1, 0.1)
            sr = PolymarketSupportRate(
                timestamp=time.time(),
                token_id=token_id,
                yes_price=max(0.01, min(0.99, yes_price)),
                no_price=1.0 - yes_price,
                volume_24h=100000.0,
                open_interest=50000.0
            )
            await self._notify_support_rate(sr)
            await asyncio.sleep(1)  # 每秒更新一次
    
    async def _update_kline(self, tick: TickData):
        """更新 K 线数据"""
        # 1 分钟 K 线
        kline_1m = self._get_or_create_kline_builder(tick.symbol, "1m")
        kline_1m.add_tick(tick)
        if kline_1m.is_complete():
            kline_data = kline_1m.finalize()
            await self._notify_kline(kline_data)
        
        # 5 分钟 K 线
        kline_5m = self._get_or_create_kline_builder(tick.symbol, "5m")
        kline_5m.add_tick(tick)
        if kline_5m.is_complete():
            kline_data = kline_5m.finalize()
            await self._notify_kline(kline_data)
    
    def _get_or_create_kline_builder(self, symbol: str, interval: str) -> 'KlineBuilder':
        """获取或创建 K 线构建器"""
        key = f"{symbol}_{interval}"
        if key not in self.kline_builders:
            self.kline_builders[key] = KlineBuilder(symbol, interval)
        return self.kline_builders[key]
    
    async def start(self, token_id: str = ""):
        """启动数据网关"""
        self._running = True
        
        # 启动 Binance 连接 (模拟)
        self._tasks.append(asyncio.create_task(self._connect_binance_ws()))
        
        # 启动 Polymarket 连接
        if token_id:
            self._tasks.append(asyncio.create_task(self._connect_polymarket_ws(token_id)))
        
        print("[DataGateway] Started")
    
    async def stop(self):
        """停止数据网关"""
        self._running = False
        for task in self._tasks:
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass
        self._tasks.clear()
        print("[DataGateway] Stopped")


class KlineBuilder:
    """
    K 线构建器
    
    基于 Tick 数据构建 K 线
    """
    
    def __init__(self, symbol: str, interval: str):
        self.symbol = symbol
        self.interval = interval
        self.interval_seconds = 60 if interval == "1m" else 300
        
        # 当前 K 线数据
        self.current_open: Optional[float] = None
        self.current_high: Optional[float] = None
        self.current_low: Optional[float] = None
        self.current_close: Optional[float] = None
        self.current_volume: float = 0.0
        self.current_trade_count: int = 0
        
        # 当前 K 线开始时间
        self.current_start_time: Optional[float] = None
    
    def add_tick(self, tick: TickData):
        """添加 Tick 数据"""
        # 检查是否需要开启新的 K 线
        if self.current_start_time is None:
            # 对齐到时间边界
            self.current_start_time = (int(tick.timestamp) // self.interval_seconds) * self.interval_seconds
        
        # 检查是否应该关闭当前 K 线
        if tick.timestamp >= self.current_start_time + self.interval_seconds:
            # 当前 K 线已完成，将在下次调用 finalize() 时返回
            pass
        
        # 更新 K 线数据
        if self.current_open is None:
            self.current_open = tick.price
            self.current_high = tick.price
            self.current_low = tick.price
        else:
            if tick.price > self.current_high:
                self.current_high = tick.price
            if tick.price < self.current_low:
                self.current_low = tick.price
        
        self.current_close = tick.price
        self.current_volume += tick.quantity
        self.current_trade_count += 1
    
    def is_complete(self) -> bool:
        """检查当前 K 线是否完成"""
        if self.current_start_time is None:
            return False
        return time.time() >= self.current_start_time + self.interval_seconds
    
    def finalize(self) -> KlineData:
        """完成当前 K 线并重置"""
        kline = KlineData(
            timestamp=self.current_start_time,
            symbol=self.symbol,
            interval=self.interval,
            open=self.current_open or 0,
            high=self.current_high or 0,
            low=self.current_low or 0,
            close=self.current_close or 0,
            volume=self.current_volume,
            trade_count=self.current_trade_count
        )
        
        # 重置为下一根 K 线
        self.current_start_time += self.interval_seconds
        self.current_open = None
        self.current_high = None
        self.current_low = None
        self.current_close = None
        self.current_volume = 0.0
        self.current_trade_count = 0
        
        return kline


# 测试代码
if __name__ == "__main__":
    async def test_data_gateway():
        gateway = DataGateway()
        
        received_ticks = 0
        received_ob = 0
        received_klines = 0
        received_sr = 0
        
        # 注册回调
        def on_tick(tick: TickData):
            nonlocal received_ticks
            received_ticks += 1
            if received_ticks <= 3:
                print(f"[Tick] {tick.symbol} @ {tick.price:.2f} ({tick.side})")
        
        def on_orderbook(ob: OrderBook):
            nonlocal received_ob
            received_ob += 1
            if received_ob <= 3:
                mid = ob.mid_price()
                if mid:
                    print(f"[OrderBook] Mid: {mid:.2f}, Spread: {ob.spread():.2f}")
        
        def on_kline(kline: KlineData):
            nonlocal received_klines
            received_klines += 1
            print(f"[Kline] {kline.symbol} {kline.interval}: O={kline.open:.2f} H={kline.high:.2f} L={kline.low:.2f} C={kline.close:.2f}")
        
        def on_support_rate(sr: PolymarketSupportRate):
            nonlocal received_sr
            received_sr += 1
            if received_sr <= 3:
                print(f"[SupportRate] {sr.token_id}: Yes={sr.yes_price:.3f}, No={sr.no_price:.3f}")
        
        gateway.register_tick_callback(on_tick)
        gateway.register_orderbook_callback(on_orderbook)
        gateway.register_kline_callback(on_kline)
        gateway.register_support_rate_callback(on_support_rate)
        
        # 启动网关
        await gateway.start(token_id="btc-up-test")
        
        # 运行 5 秒观察
        await asyncio.sleep(5)
        
        await gateway.stop()
        
        print(f"\n=== Test Summary ===")
        print(f"Ticks received: {received_ticks}")
        print(f"OrderBooks received: {received_ob}")
        print(f"Klines received: {received_klines}")
        print(f"SupportRates received: {received_sr}")
        print("DataGateway test completed!")
    
    asyncio.run(test_data_gateway())
