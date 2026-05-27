"""
策略与信号计算引擎

核心功能:
1. 整合多源数据 (支持率、实时价格、K 线、订单簿)
2. 在收盘前 30 秒触发决策逻辑
3. 计算特征指标 (OBI, VWAP, 动量等)
4. 生成交易信号 (Buy/Sell/Hold)
"""

import asyncio
import time
from typing import Optional, Dict, List, Callable, Tuple
from dataclasses import dataclass, field
from enum import Enum
from collections import deque

# 导入数据网关的数据类型
from data_gateway import (
    TickData, OrderBook, KlineData, PolymarketSupportRate, DataSource
)
from market_router import MarketRouter, MarketState, MarketInfo


class SignalType(Enum):
    """信号类型"""
    BUY = "buy"           # 做多 (Yes)
    SELL = "sell"         # 做空 (No)
    HOLD = "hold"         # 持有/观望
    CLOSE = "close"       # 平仓


@dataclass
class TradingSignal:
    """交易信号"""
    timestamp: float
    signal_type: SignalType
    confidence: float     # 置信度 (0-1)
    token_id: str
    reason: str           # 信号原因描述
    
    # 关键数据快照
    support_rate: float = 0.0
    binance_price: float = 0.0
    orderbook_imbalance: float = 0.0
    momentum_1m: float = 0.0
    momentum_5m: float = 0.0


@dataclass
class FeatureSet:
    """特征集合 - 用于策略计算"""
    timestamp: float
    
    # 支持率相关
    support_rate: float = 0.0
    support_rate_change_10s: float = 0.0  # 10 秒内支持率变化
    
    # 价格相关
    binance_price: float = 0.0
    price_change_10s: float = 0.0  # 10 秒内价格变化百分比
    
    # 订单簿相关
    orderbook_imbalance: float = 0.0  # OBI = (bid_qty - ask_qty) / (bid_qty + ask_qty)
    spread: float = 0.0
    
    # K 线相关
    kline_1m_open: float = 0.0
    kline_1m_close: float = 0.0
    kline_1m_high: float = 0.0
    kline_1m_low: float = 0.0
    kline_5m_open: float = 0.0
    kline_5m_close: float = 0.0
    
    # 动量指标
    momentum_1m: float = 0.0  # 1 分钟动量
    momentum_5m: float = 0.0  # 5 分钟动量
    
    # 交叉市场套利机会
    cross_market_spread: float = 0.0  # Polymarket 隐含概率 vs Binance 价格推导概率


class SignalEngine:
    """
    信号计算引擎
    
    接收来自 DataGateway 的实时数据，结合 MarketRouter 的市场状态，
    在适当时机生成交易信号
    """
    
    def __init__(self, market_router: MarketRouter):
        self.market_router = market_router
        
        # 最新数据缓存
        self.latest_tick: Optional[TickData] = None
        self.latest_orderbook: Optional[OrderBook] = None
        self.latest_support_rate: Optional[PolymarketSupportRate] = None
        self.kline_1m: Optional[KlineData] = None
        self.kline_5m: Optional[KlineData] = None
        
        # 历史数据 (用于计算变化率)
        self.tick_history: deque = deque(maxlen=100)
        self.support_rate_history: deque = deque(maxlen=100)
        
        # 信号回调
        self.signal_callbacks: List[Callable[[TradingSignal], None]] = []
        
        # 决策窗口配置
        self.decision_window_start = 30  # 收盘前 30 秒开始决策
        self.no_trade_window = 5         # 收盘前 5 秒禁止开仓
        
        # 运行状态
        self._running = False
    
    def register_signal_callback(self, callback: Callable[[TradingSignal], None]):
        """注册信号回调"""
        self.signal_callbacks.append(callback)
    
    async def _notify_signal(self, signal: TradingSignal):
        """通知交易信号"""
        print(f"[SignalEngine] Signal: {signal.signal_type.value.upper()} | "
              f"Confidence: {signal.confidence:.2f} | Reason: {signal.reason}")
        
        for callback in self.signal_callbacks:
            try:
                if asyncio.iscoroutinefunction(callback):
                    await callback(signal)
                else:
                    callback(signal)
            except Exception as e:
                print(f"[SignalEngine] Error in signal callback: {e}")
    
    def on_tick(self, tick: TickData):
        """处理 Tick 数据"""
        self.latest_tick = tick
        self.tick_history.append((tick.timestamp, tick.price))
    
    def on_orderbook(self, ob: OrderBook):
        """处理订单簿数据"""
        self.latest_orderbook = ob
    
    def on_support_rate(self, sr: PolymarketSupportRate):
        """处理支持率数据"""
        self.latest_support_rate = sr
        self.support_rate_history.append((sr.timestamp, sr.yes_price))
    
    def on_kline(self, kline: KlineData):
        """处理 K 线数据"""
        if kline.interval == "1m":
            self.kline_1m = kline
        elif kline.interval == "5m":
            self.kline_5m = kline
    
    def _calculate_features(self) -> FeatureSet:
        """计算特征集合"""
        features = FeatureSet(timestamp=time.time())
        
        # 支持率
        if self.latest_support_rate:
            features.support_rate = self.latest_support_rate.yes_price
            
            # 计算 10 秒内支持率变化
            if len(self.support_rate_history) >= 2:
                now = time.time()
                old_sr = None
                for ts, sr in reversed(self.support_rate_history):
                    if now - ts >= 10:
                        old_sr = sr
                        break
                if old_sr:
                    features.support_rate_change_10s = features.support_rate - old_sr
        
        # 价格和订单簿
        if self.latest_tick:
            features.binance_price = self.latest_tick.price
            
            # 计算 10 秒内价格变化
            if len(self.tick_history) >= 2:
                now = time.time()
                old_price = None
                for ts, price in reversed(self.tick_history):
                    if now - ts >= 10:
                        old_price = price
                        break
                if old_price and old_price > 0:
                    features.price_change_10s = (features.binance_price - old_price) / old_price
        
        if self.latest_orderbook:
            # 计算订单簿失衡 (OBI)
            bid_qty = sum(level.quantity for level in self.latest_orderbook.bids[:5])
            ask_qty = sum(level.quantity for level in self.latest_orderbook.asks[:5])
            if bid_qty + ask_qty > 0:
                features.orderbook_imbalance = (bid_qty - ask_qty) / (bid_qty + ask_qty)
            features.spread = self.latest_orderbook.spread() or 0.0
        
        # K 线数据
        if self.kline_1m:
            features.kline_1m_open = self.kline_1m.open
            features.kline_1m_close = self.kline_1m.close
            features.kline_1m_high = self.kline_1m.high
            features.kline_1m_low = self.kline_1m.low
            features.momentum_1m = (self.kline_1m.close - self.kline_1m.open) / self.kline_1m.open if self.kline_1m.open > 0 else 0
        
        if self.kline_5m:
            features.kline_5m_open = self.kline_5m.open
            features.kline_5m_close = self.kline_5m.close
            features.momentum_5m = (self.kline_5m.close - self.kline_5m.open) / self.kline_5m.open if self.kline_5m.open > 0 else 0
        
        # 跨市场价差 (简化版)
        # 假设 Binance 价格上涨意味着 Polymarket Yes 份额应该升值
        features.cross_market_spread = features.support_rate - (0.5 + features.price_change_10s * 10)
        
        return features
    
    def _generate_signal(self, features: FeatureSet, market: MarketInfo) -> TradingSignal:
        """基于特征生成交易信号"""
        
        # 简单策略逻辑示例 (实际应使用 ML 模型)
        score = 0.0
        reasons = []
        
        # 1. 支持率因素 (权重 0.3)
        if features.support_rate > 0.55:
            score += 0.3
            reasons.append("高支持率")
        elif features.support_rate < 0.45:
            score -= 0.3
            reasons.append("低支持率")
        
        # 2. 支持率变化趋势 (权重 0.2)
        if features.support_rate_change_10s > 0.02:
            score += 0.2
            reasons.append("支持率上升")
        elif features.support_rate_change_10s < -0.02:
            score -= 0.2
            reasons.append("支持率下降")
        
        # 3. 订单簿失衡 (权重 0.2)
        if features.orderbook_imbalance > 0.2:
            score += 0.2
            reasons.append("买盘强劲")
        elif features.orderbook_imbalance < -0.2:
            score -= 0.2
            reasons.append("卖压沉重")
        
        # 4. 1 分钟动量 (权重 0.15)
        if features.momentum_1m > 0.001:
            score += 0.15
            reasons.append("短期上涨")
        elif features.momentum_1m < -0.001:
            score -= 0.15
            reasons.append("短期下跌")
        
        # 5. 5 分钟动量 (权重 0.15)
        if features.momentum_5m > 0.005:
            score += 0.15
            reasons.append("中期上涨")
        elif features.momentum_5m < -0.005:
            score -= 0.15
            reasons.append("中期下跌")
        
        # 确定信号类型
        confidence = abs(score)
        if score > 0.3:
            signal_type = SignalType.BUY
        elif score < -0.3:
            signal_type = SignalType.SELL
        else:
            signal_type = SignalType.HOLD
        
        # 检查是否在冷却期
        current_state = self.market_router._update_market_state(market)
        if current_state == MarketState.COOLDOWN:
            signal_type = SignalType.HOLD
            reasons.append("冷却期禁止开仓")
        
        return TradingSignal(
            timestamp=time.time(),
            signal_type=signal_type,
            confidence=min(1.0, confidence),
            token_id=market.token_id,
            reason="; ".join(reasons) if reasons else "无明显信号",
            support_rate=features.support_rate,
            binance_price=features.binance_price,
            orderbook_imbalance=features.orderbook_imbalance,
            momentum_1m=features.momentum_1m,
            momentum_5m=features.momentum_5m
        )
    
    async def _decision_loop(self):
        """决策循环 - 每秒检查是否应该生成信号"""
        while self._running:
            market = self.market_router.get_current_market()
            
            if market and self.market_router.should_make_decision():
                # 在决策窗口内，生成信号
                features = self._calculate_features()
                signal = self._generate_signal(features, market)
                await self._notify_signal(signal)
            
            await asyncio.sleep(1)
    
    async def start(self):
        """启动信号引擎"""
        self._running = True
        
        # 启动决策循环
        asyncio.create_task(self._decision_loop())
        
        print("[SignalEngine] Started")
    
    async def stop(self):
        """停止信号引擎"""
        self._running = False
        print("[SignalEngine] Stopped")


# 测试代码
if __name__ == "__main__":
    async def test_signal_engine():
        # 创建市场路由器
        router = MarketRouter()
        await router.start()
        
        # 创建信号引擎
        engine = SignalEngine(router)
        
        # 模拟数据输入
        def on_signal(signal: TradingSignal):
            print(f"\n>>> SIGNAL: {signal.signal_type.value.upper()} @ {signal.confidence:.2f}")
            print(f"    Token: {signal.token_id}")
            print(f"    Support Rate: {signal.support_rate:.3f}")
            print(f"    Binance Price: ${signal.binance_price:.2f}")
            print(f"    OBI: {signal.orderbook_imbalance:.3f}")
            print(f"    Momentum 1m: {signal.momentum_1m:.4f}, 5m: {signal.momentum_5m:.4f}")
            print(f"    Reason: {signal.reason}")
        
        engine.register_signal_callback(on_signal)
        
        # 手动注入一些测试数据
        from data_gateway import TickData, OrderBook, OrderBookLevel, PolymarketSupportRate, KlineData
        
        # 模拟 Tick
        tick = TickData(
            timestamp=time.time(),
            source=DataSource.BINANCE,
            symbol="BTCUSDT",
            price=67000.0,
            quantity=1.0,
            side="buy"
        )
        engine.on_tick(tick)
        
        # 模拟订单簿
        ob = OrderBook(
            timestamp=time.time(),
            symbol="BTCUSDT",
            bids=[OrderBookLevel(66999 - i*0.5, 5.0) for i in range(5)],
            asks=[OrderBookLevel(67001 + i*0.5, 3.0) for i in range(5)]
        )
        engine.on_orderbook(ob)
        
        # 模拟支持率
        sr = PolymarketSupportRate(
            timestamp=time.time(),
            token_id="btc-up-test",
            yes_price=0.58,
            no_price=0.42
        )
        engine.on_support_rate(sr)
        
        # 模拟 K 线
        kline_1m = KlineData(
            timestamp=time.time(),
            symbol="BTCUSDT",
            interval="1m",
            open=66950,
            high=67050,
            low=66900,
            close=67000,
            volume=100.0
        )
        engine.on_kline(kline_1m)
        
        kline_5m = KlineData(
            timestamp=time.time(),
            symbol="BTCUSDT",
            interval="5m",
            open=66800,
            high=67100,
            low=66750,
            close=67000,
            volume=500.0
        )
        engine.on_kline(kline_5m)
        
        # 启动信号引擎
        await engine.start()
        
        # 等待 5 秒观察
        await asyncio.sleep(5)
        
        await engine.stop()
        await router.stop()
        
        print("\n=== SignalEngine Test Completed ===")
    
    asyncio.run(test_signal_engine())
