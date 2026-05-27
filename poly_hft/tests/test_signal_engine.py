"""SignalEngine 单元测试"""

import asyncio
import time
import sys
sys.path.insert(0, '/workspace/poly_hft/core')

from market_router import MarketRouter, MarketState, MarketInfo
from signal_engine import SignalEngine, SignalType, TradingSignal, FeatureSet
from data_gateway import TickData, OrderBook, OrderBookLevel, PolymarketSupportRate, KlineData, DataSource


def test_feature_calculation():
    """测试特征计算"""
    router = MarketRouter()
    engine = SignalEngine(router)
    
    # 注入测试数据
    tick = TickData(
        timestamp=time.time(),
        source=DataSource.BINANCE,
        symbol="BTCUSDT",
        price=67000.0,
        quantity=1.0,
        side="buy"
    )
    engine.on_tick(tick)
    
    ob = OrderBook(
        timestamp=time.time(),
        symbol="BTCUSDT",
        bids=[OrderBookLevel(66999 - i*0.5, 5.0) for i in range(5)],
        asks=[OrderBookLevel(67001 + i*0.5, 3.0) for i in range(5)]
    )
    engine.on_orderbook(ob)
    
    sr = PolymarketSupportRate(
        timestamp=time.time(),
        token_id="btc-up-test",
        yes_price=0.58,
        no_price=0.42
    )
    engine.on_support_rate(sr)
    
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
    
    # 计算特征
    features = engine._calculate_features()
    
    assert features.support_rate == 0.58, f"Expected 0.58, got {features.support_rate}"
    assert features.binance_price == 67000.0, f"Expected 67000, got {features.binance_price}"
    assert features.orderbook_imbalance > 0, f"Expected positive OBI, got {features.orderbook_imbalance}"
    assert features.momentum_1m > 0, f"Expected positive 1m momentum, got {features.momentum_1m}"
    assert features.momentum_5m > 0, f"Expected positive 5m momentum, got {features.momentum_5m}"
    
    print("  ✓ test_feature_calculation")


def test_signal_generation_buy():
    """测试买入信号生成"""
    router = MarketRouter()
    engine = SignalEngine(router)
    
    # 创建市场信息
    now = time.time()
    market = MarketInfo(
        token_id="btc-up-test",
        condition_id="cond-1",
        question="Test?",
        start_time=now - 280,
        end_time=now + 20,  # 在决策窗口内
        state=MarketState.DECISION
    )
    
    # 创建强买入特征
    features = FeatureSet(
        timestamp=now,
        support_rate=0.60,  # 高支持率
        support_rate_change_10s=0.03,  # 上升
        binance_price=67000.0,
        orderbook_imbalance=0.3,  # 买盘强劲
        momentum_1m=0.002,  # 短期上涨
        momentum_5m=0.01   # 中期上涨
    )
    
    signal = engine._generate_signal(features, market)
    
    assert signal.signal_type == SignalType.BUY, f"Expected BUY, got {signal.signal_type}"
    assert signal.confidence > 0.3, f"Expected confidence > 0.3, got {signal.confidence}"
    
    print("  ✓ test_signal_generation_buy")


def test_signal_generation_sell():
    """测试卖出信号生成"""
    router = MarketRouter()
    engine = SignalEngine(router)
    
    now = time.time()
    market = MarketInfo(
        token_id="btc-up-test",
        condition_id="cond-1",
        question="Test?",
        start_time=now - 280,
        end_time=now + 20,
        state=MarketState.DECISION
    )
    
    # 创建强卖出特征
    features = FeatureSet(
        timestamp=now,
        support_rate=0.35,  # 低支持率
        support_rate_change_10s=-0.03,  # 下降
        binance_price=67000.0,
        orderbook_imbalance=-0.3,  # 卖压沉重
        momentum_1m=-0.002,  # 短期下跌
        momentum_5m=-0.01   # 中期下跌
    )
    
    signal = engine._generate_signal(features, market)
    
    assert signal.signal_type == SignalType.SELL, f"Expected SELL, got {signal.signal_type}"
    assert signal.confidence > 0.3, f"Expected confidence > 0.3, got {signal.confidence}"
    
    print("  ✓ test_signal_generation_sell")


def test_signal_generation_cooldown():
    """测试冷却期禁止开仓"""
    router = MarketRouter()
    engine = SignalEngine(router)
    
    now = time.time()
    market = MarketInfo(
        token_id="btc-up-test",
        condition_id="cond-1",
        question="Test?",
        start_time=now - 297,
        end_time=now + 3,  # 在冷却期
        state=MarketState.COOLDOWN
    )
    
    # 即使有强信号，冷却期也应该 HOLD
    features = FeatureSet(
        timestamp=now,
        support_rate=0.70,  # 非常高支持率
        support_rate_change_10s=0.05,
        binance_price=67000.0,
        orderbook_imbalance=0.5,
        momentum_1m=0.005,
        momentum_5m=0.02
    )
    
    signal = engine._generate_signal(features, market)
    
    assert signal.signal_type == SignalType.HOLD, f"Expected HOLD in cooldown, got {signal.signal_type}"
    assert "冷却期" in signal.reason, f"Expected cooldown reason, got {signal.reason}"
    
    print("  ✓ test_signal_generation_cooldown")


async def test_signal_callback():
    """测试信号回调"""
    router = MarketRouter()
    await router.start()
    
    engine = SignalEngine(router)
    
    received_signals = []
    
    def on_signal(signal: TradingSignal):
        received_signals.append(signal)
    
    engine.register_signal_callback(on_signal)
    
    # 手动触发一个信号
    now = time.time()
    market = MarketInfo(
        token_id="btc-up-test",
        condition_id="cond-1",
        question="Test?",
        start_time=now - 280,
        end_time=now + 20,
        state=MarketState.DECISION
    )
    router.current_cycle.current = market
    
    features = FeatureSet(
        timestamp=now,
        support_rate=0.58,
        binance_price=67000.0,
        orderbook_imbalance=0.1,
        momentum_1m=0.001,
        momentum_5m=0.005
    )
    signal = engine._generate_signal(features, market)
    await engine._notify_signal(signal)
    
    assert len(received_signals) == 1, f"Expected 1 signal, got {len(received_signals)}"
    assert received_signals[0].token_id == "btc-up-test"
    
    await router.stop()
    print("  ✓ test_signal_callback")


def run_all_tests():
    """运行所有测试"""
    print("\n=== Running SignalEngine Tests ===\n")
    
    print("Testing feature calculation...")
    test_feature_calculation()
    
    print("\nTesting signal generation...")
    test_signal_generation_buy()
    test_signal_generation_sell()
    test_signal_generation_cooldown()
    
    print("\nTesting async operations...")
    asyncio.run(test_signal_callback())
    
    print("\n=== All SignalEngine tests passed! ===\n")


if __name__ == "__main__":
    run_all_tests()
