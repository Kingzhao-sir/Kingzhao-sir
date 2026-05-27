"""MarketRouter 单元测试 (无 pytest 依赖)"""

import asyncio
import time
import sys
sys.path.insert(0, '/workspace/poly_hft')

from core.market_router import MarketRouter, MarketState, MarketInfo


def test_time_remaining_future():
    """测试未来市场的剩余时间计算"""
    now = time.time()
    market = MarketInfo(
        token_id="test-1",
        condition_id="cond-1",
        question="Test?",
        start_time=now,
        end_time=now + 300
    )
    remaining = market.time_remaining
    assert 299 <= remaining <= 300, f"Expected ~300, got {remaining}"
    print("  ✓ test_time_remaining_future")

def test_time_remaining_past():
    """测试过去市场的剩余时间 (应为 0)"""
    now = time.time()
    market = MarketInfo(
        token_id="test-2",
        condition_id="cond-2",
        question="Test?",
        start_time=now - 600,
        end_time=now - 300
    )
    remaining = market.time_remaining
    assert remaining == 0, f"Expected 0, got {remaining}"
    print("  ✓ test_time_remaining_past")

def test_elapsed_time():
    """测试已过去时间计算"""
    now = time.time()
    market = MarketInfo(
        token_id="test-3",
        condition_id="cond-3",
        question="Test?",
        start_time=now - 120,
        end_time=now + 180
    )
    elapsed = market.elapsed_time
    assert 119 <= elapsed <= 121, f"Expected ~120, got {elapsed}"
    print("  ✓ test_elapsed_time")

def test_initialization():
    """测试初始化"""
    router = MarketRouter()
    assert router.preheat_seconds == 60
    assert router.decision_seconds == 30
    assert router.cooldown_seconds == 5
    assert router.get_current_market() is None
    print("  ✓ test_initialization")

def test_can_open_position_no_market():
    """测试无市场时不能开仓"""
    router = MarketRouter()
    assert router.can_open_position() is False
    print("  ✓ test_can_open_position_no_market")

def test_state_callbacks_registration():
    """测试状态回调注册"""
    router = MarketRouter()
    def test_callback(market: MarketInfo):
        pass
    router.register_state_callback(MarketState.DECISION, test_callback)
    assert len(router.state_callbacks[MarketState.DECISION]) == 1
    print("  ✓ test_state_callbacks_registration")

async def test_router_start_stop():
    """测试路由器启动和停止"""
    router = MarketRouter()
    await router.start()
    assert router._running is True
    await asyncio.sleep(0.5)
    await router.stop()
    assert router._running is False
    print("  ✓ test_router_start_stop")

def test_state_update_preheat():
    """测试预热状态"""
    now = time.time()
    market = MarketInfo(
        token_id="test-preheat",
        condition_id="cond",
        question="Test?",
        start_time=now,
        end_time=now + 300
    )
    router = MarketRouter()
    state = router._update_market_state(market)
    assert state == MarketState.PREHEAT
    print("  ✓ test_state_update_preheat")

def test_state_update_decision():
    """测试决策状态"""
    now = time.time()
    market = MarketInfo(
        token_id="test-decision",
        condition_id="cond",
        question="Test?",
        start_time=now - 280,
        end_time=now + 20
    )
    router = MarketRouter()
    state = router._update_market_state(market)
    assert state == MarketState.DECISION, f"Expected DECISION, got {state}"
    print("  ✓ test_state_update_decision")

def test_state_update_cooldown():
    """测试冷却状态"""
    now = time.time()
    market = MarketInfo(
        token_id="test-cooldown",
        condition_id="cond",
        question="Test?",
        start_time=now - 297,
        end_time=now + 3
    )
    router = MarketRouter()
    state = router._update_market_state(market)
    assert state == MarketState.COOLDOWN
    print("  ✓ test_state_update_cooldown")

def test_state_update_settlement():
    """测试结算状态"""
    now = time.time()
    market = MarketInfo(
        token_id="test-settlement",
        condition_id="cond",
        question="Test?",
        start_time=now - 600,
        end_time=now - 300
    )
    router = MarketRouter()
    state = router._update_market_state(market)
    assert state == MarketState.SETTLEMENT
    print("  ✓ test_state_update_settlement")

def run_all_tests():
    """运行所有测试"""
    print("\n=== Running MarketRouter Tests ===\n")
    
    print("Testing MarketInfo...")
    test_time_remaining_future()
    test_time_remaining_past()
    test_elapsed_time()
    
    print("\nTesting MarketRouter initialization...")
    test_initialization()
    test_can_open_position_no_market()
    test_state_callbacks_registration()
    
    print("\nTesting market state transitions...")
    test_state_update_preheat()
    test_state_update_decision()
    test_state_update_cooldown()
    test_state_update_settlement()
    
    print("\nTesting async operations...")
    asyncio.run(test_router_start_stop())
    
    print("\n=== All tests passed! ===\n")

if __name__ == "__main__":
    run_all_tests()
