"""
回测系统单元测试
"""
import pytest
import asyncio
from datetime import datetime

from poly_hft.backtest.config import (
    BacktestConfig, 
    TickData, 
    DataSource, 
    TradeLogEntry, 
    BacktestReport
)
from poly_hft.backtest.engine import (
    OrderBookSimulator, 
    Position, 
    BacktestEngine
)


class TestBacktestConfig:
    """测试回测配置"""
    
    def test_default_config(self):
        """测试默认配置"""
        config = BacktestConfig()
        assert config.strategy_name == "BTC_5m_Prediction"
        assert config.initial_capital == 1000.0
        assert config.maker_fee == 0.0005
        assert config.taker_fee == 0.0010
        assert config.slippage_bps == 2.0
    
    def test_custom_config(self):
        """测试自定义配置"""
        config = BacktestConfig(
            strategy_name="Test_Strategy",
            initial_capital=5000.0,
            start_time=1700000000000,
            end_time=1700003600000
        )
        assert config.strategy_name == "Test_Strategy"
        assert config.initial_capital == 5000.0
    
    def test_invalid_config_negative_capital(self):
        """测试无效配置 - 负资金"""
        with pytest.raises(ValueError):
            BacktestConfig(initial_capital=-100.0)
    
    def test_invalid_config_time_range(self):
        """测试无效配置 - 时间范围错误"""
        with pytest.raises(ValueError):
            BacktestConfig(start_time=1700003600000, end_time=1700000000000)


class TestTickData:
    """测试 Tick 数据"""
    
    def test_mid_price(self):
        """测试中间价计算"""
        tick = TickData(
            timestamp=1700000000000,
            source=DataSource.POLYMARKET_ORDERBOOK,
            last_price=0.52,
            bid_price=0.519,
            ask_price=0.521,
            bid_size=100.0,
            ask_size=100.0
        )
        assert abs(tick.mid_price() - 0.52) < 0.0001
    
    def test_spread_bps(self):
        """测试价差计算 (基点)"""
        tick = TickData(
            timestamp=1700000000000,
            source=DataSource.POLYMARKET_ORDERBOOK,
            last_price=0.52,
            bid_price=0.519,
            ask_price=0.521,
            bid_size=100.0,
            ask_size=100.0
        )
        # (0.521 - 0.519) / 0.52 * 10000 ≈ 38.46 bps
        assert abs(tick.spread_bps() - 38.46) < 0.1


class TestOrderBookSimulator:
    """测试订单簿模拟器"""
    
    def test_init(self):
        """测试初始化"""
        tick = TickData(
            timestamp=1700000000000,
            source=DataSource.POLYMARKET_ORDERBOOK,
            last_price=0.52,
            bid_price=0.519,
            ask_price=0.521,
            bid_size=100.0,
            ask_size=100.0
        )
        ob = OrderBookSimulator(tick)
        assert ob.tick == tick
        assert 0.519 in ob.bid_depth
        assert 0.521 in ob.ask_depth
    
    def test_can_fill_buy(self):
        """测试买单成交检查"""
        tick = TickData(
            timestamp=1700000000000,
            source=DataSource.POLYMARKET_ORDERBOOK,
            last_price=0.52,
            bid_price=0.519,
            ask_price=0.521,
            bid_size=100.0,
            ask_size=50.0
        )
        ob = OrderBookSimulator(tick)
        
        # 可以成交小单
        assert ob.can_fill_buy(10.0, 0.521) is True
        # 不能成交大单
        assert ob.can_fill_buy(100.0, 0.521) is False
    
    def test_get_fill_price_with_slippage(self):
        """测试考虑滑点的成交价格"""
        tick = TickData(
            timestamp=1700000000000,
            source=DataSource.POLYMARKET_ORDERBOOK,
            last_price=0.52,
            bid_price=0.519,
            ask_price=0.521,
            bid_size=100.0,
            ask_size=100.0
        )
        ob = OrderBookSimulator(tick)
        
        # 买单价格应该略高于 ask
        fill_price = ob.get_fill_price("Buy", 50.0)
        assert fill_price > tick.ask_price
        
        # 卖单价格应该略低于 bid
        fill_price = ob.get_fill_price("Sell", 50.0)
        assert fill_price < tick.bid_price


class TestPosition:
    """测试持仓管理"""
    
    def test_open_position(self):
        """测试开仓"""
        pos = Position()
        assert pos.is_open is False
        
        pos.open(100.0, 0.52, "market-123", 1700000000000)
        assert pos.is_open is True
        assert pos.quantity == 100.0
        assert abs(pos.avg_entry_price - 0.52) < 0.0001
    
    def test_close_position_profit(self):
        """测试平仓盈利"""
        pos = Position()
        pos.open(100.0, 0.50, "market-123", 1700000000000)
        
        # 价格上涨，盈利
        pnl = pos.close(0.60)
        assert pnl > 0
        assert abs(pnl - 10.0) < 0.01  # (0.60 - 0.50) * 100
        assert pos.is_open is False
    
    def test_close_position_loss(self):
        """测试平仓亏损"""
        pos = Position()
        pos.open(100.0, 0.60, "market-123", 1700000000000)
        
        # 价格下跌，亏损
        pnl = pos.close(0.50)
        assert pnl < 0
        assert abs(pnl - (-10.0)) < 0.01
    
    def test_unrealized_pnl(self):
        """测试未实现盈亏"""
        pos = Position()
        pos.open(100.0, 0.50, "market-123", 1700000000000)
        
        # 当前价格 0.55，未实现盈利 5.0
        unrealized = pos.unrealized_pnl(0.55)
        assert abs(unrealized - 5.0) < 0.01


class TestBacktestEngine:
    """测试回测引擎"""
    
    @pytest.mark.asyncio
    async def test_engine_initialization(self):
        """测试引擎初始化"""
        config = BacktestConfig(
            initial_capital=1000.0,
            start_time=1700000000000,
            end_time=1700003600000
        )
        engine = BacktestEngine(config)
        
        assert engine.capital == 1000.0
        assert engine.position.is_open is False
    
    @pytest.mark.asyncio
    async def test_sample_data_generation(self):
        """测试示例数据生成"""
        config = BacktestConfig(
            data_path="./nonexistent_file.csv",  # 强制使用示例数据
            start_time=1700000000000,
            end_time=1700003600000
        )
        engine = BacktestEngine(config)
        
        ticks = await engine.load_data()
        assert len(ticks) > 0
        assert all(tick.implied_probability is not None for tick in ticks)
    
    @pytest.mark.asyncio
    async def test_simple_strategy(self):
        """测试简单策略 - 买入持有"""
        config = BacktestConfig(
            initial_capital=1000.0,
            start_time=1700000000000,
            end_time=1700003600000,
            data_path="./nonexistent_file.csv"
        )
        engine = BacktestEngine(config)
        
        signal_count = 0
        
        def on_tick(tick):
            pass
        
        def on_signal(tick):
            nonlocal signal_count
            signal_count += 1
            # 简单策略：第一个 tick 买入
            if signal_count == 1 and not engine.position.is_open:
                return "Buy"
            # 最后平仓
            if signal_count > 100 and engine.position.is_open:
                return "Sell"
            return "Hold"
        
        engine.set_strategy(on_tick, on_signal)
        report = await engine.run()
        
        assert report.total_trades >= 1
        assert report.final_capital > 0
    
    @pytest.mark.asyncio
    async def test_backtest_report_generation(self):
        """测试回测报告生成"""
        config = BacktestConfig(
            initial_capital=1000.0,
            start_time=1700000000000,
            end_time=1700003600000,
            data_path="./nonexistent_file.csv"
        )
        engine = BacktestEngine(config)
        
        # 空策略
        def on_tick(tick):
            pass
        
        def on_signal(tick):
            return "Hold"
        
        engine.set_strategy(on_tick, on_signal)
        report = await engine.run()
        
        assert report.total_trades == 0
        assert report.final_capital == 1000.0
        assert report.to_dict() is not None


class TestBacktestReport:
    """测试回测报告"""
    
    def test_report_to_dict(self):
        """测试报告序列化"""
        report = BacktestReport(
            total_trades=10,
            winning_trades=6,
            losing_trades=4,
            win_rate=0.6,
            total_pnl=100.0,
            max_drawdown=0.15,
            sharpe_ratio=1.5,
            final_capital=1100.0,
            total_fees=5.0
        )
        
        result = report.to_dict()
        assert result["total_trades"] == 10
        assert result["win_rate"] == 60.0  # 转换为百分比
        assert result["final_capital"] == 1100.0
    
    def test_report_with_trade_log(self):
        """测试带交易记录的报告"""
        trade1 = TradeLogEntry(
            timestamp=1700000000000,
            market_id="market-123",
            side="Buy",
            entry_price=0.50,
            exit_price=0.55,
            quantity=100.0,
            pnl=5.0,
            fees=0.05,
            reason="Signal_Close"
        )
        trade2 = TradeLogEntry(
            timestamp=1700000001000,
            market_id="market-123",
            side="Buy",
            entry_price=0.52,
            exit_price=0.50,
            quantity=100.0,
            pnl=-2.0,
            fees=0.02,
            reason="Stop_Loss"
        )
        
        report = BacktestReport(
            total_trades=2,
            winning_trades=1,
            losing_trades=1,
            win_rate=0.5,
            total_pnl=3.0,
            max_drawdown=0.02,
            sharpe_ratio=0.8,
            final_capital=1003.0,
            total_fees=0.07,
            trade_log=[trade1, trade2]
        )
        
        result = report.to_dict()
        assert "trade_count_by_reason" in result
        assert result["trade_count_by_reason"]["Signal_Close"] == 1
        assert result["trade_count_by_reason"]["Stop_Loss"] == 1


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
