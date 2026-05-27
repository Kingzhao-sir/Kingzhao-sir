"""
回测引擎核心模块
支持 Tick 级事件驱动回测，模拟真实交易环境
"""
import asyncio
from typing import Callable, List, Optional, Dict
from collections import deque
from datetime import datetime
import pandas as pd

from .config import (
    BacktestConfig, 
    TickData, 
    DataSource, 
    TradeLogEntry, 
    BacktestReport
)


class OrderBookSimulator:
    """订单簿模拟器 - 模拟 Polymarket 订单簿行为"""
    
    def __init__(self, tick: TickData):
        self.tick = tick
        self.bid_depth: Dict[float, float] = {tick.bid_price: tick.bid_size}
        self.ask_depth: Dict[float, float] = {tick.ask_price: tick.ask_size}
    
    def update(self, tick: TickData):
        """更新订单簿状态"""
        self.tick = tick
        self.bid_depth = {tick.bid_price: tick.bid_size}
        self.ask_depth = {tick.ask_price: tick.ask_size}
    
    def can_fill_buy(self, quantity: float, price: float) -> bool:
        """检查买单是否能成交"""
        available = sum(size for p, size in self.ask_depth.items() if p <= price)
        return available >= quantity
    
    def can_fill_sell(self, quantity: float, price: float) -> bool:
        """检查卖单是否能成交"""
        available = sum(size for p, size in self.bid_depth.items() if p >= price)
        return available >= quantity
    
    def get_fill_price(self, side: str, quantity: float) -> float:
        """获取模拟成交价格 (考虑滑点)"""
        if side == "Buy":
            # 买单吃 ask，价格上移
            base_price = self.tick.ask_price
        else:
            # 卖单吃 bid，价格下移
            base_price = self.tick.bid_price
        
        # 简单滑点模型：交易量越大，滑点越大
        slippage = base_price * (quantity / max(self.tick.bid_size, self.tick.ask_size, 1)) * 0.001
        return base_price + slippage if side == "Buy" else base_price - slippage


class Position:
    """持仓管理"""
    
    def __init__(self):
        self.quantity: float = 0.0
        self.avg_entry_price: float = 0.0
        self.market_id: str = ""
        self.entry_time: int = 0
    
    def open(self, quantity: float, price: float, market_id: str, timestamp: int):
        """开仓"""
        self.quantity = quantity
        self.avg_entry_price = price
        self.market_id = market_id
        self.entry_time = timestamp
    
    def close(self, exit_price: float) -> float:
        """平仓并返回 PnL"""
        if self.quantity == 0:
            return 0.0
        
        # Polymarket: Yes 份额价格范围 0-1
        # Buy: 低价买入高价卖出盈利
        # Sell: 高价卖出低价买入盈利
        pnl = (exit_price - self.avg_entry_price) * self.quantity
        
        # 重置持仓
        self.quantity = 0.0
        self.avg_entry_price = 0.0
        return pnl
    
    @property
    def is_open(self) -> bool:
        return self.quantity > 0
    
    def unrealized_pnl(self, current_price: float) -> float:
        """计算未实现盈亏"""
        if not self.is_open:
            return 0.0
        return (current_price - self.avg_entry_price) * self.quantity


class BacktestEngine:
    """回测引擎核心"""
    
    def __init__(self, config: BacktestConfig):
        self.config = config
        self.capital = config.initial_capital
        self.position = Position()
        self.orderbook: Optional[OrderBookSimulator] = None
        self.trade_log: List[TradeLogEntry] = []
        
        # 策略回调
        self.on_tick_callback: Optional[Callable[[TickData], None]] = None
        self.on_signal_callback: Optional[Callable[[TickData], str]] = None  # 返回 "Buy"/"Sell"/"Hold"
        
        # 统计
        self.peak_capital = config.initial_capital
        self.max_drawdown = 0.0
        self.daily_returns: List[float] = []
    
    def set_strategy(self, on_tick: Callable[[TickData], None], 
                     on_signal: Callable[[TickData], str]):
        """设置策略回调函数"""
        self.on_tick_callback = on_tick
        self.on_signal_callback = on_signal
    
    async def load_data(self) -> List[TickData]:
        """加载历史数据"""
        try:
            df = pd.read_csv(self.config.data_path)
            ticks = []
            for _, row in df.iterrows():
                tick = TickData(
                    timestamp=int(row['timestamp']),
                    source=DataSource(row.get('source', 'binance_spot')),
                    last_price=float(row['last_price']),
                    bid_price=float(row['bid_price']),
                    ask_price=float(row['ask_price']),
                    bid_size=float(row.get('bid_size', 1.0)),
                    ask_size=float(row.get('ask_size', 1.0)),
                    implied_probability=float(row['implied_probability']) if 'implied_probability' in row else None,
                    volume=float(row.get('volume', 0.0)),
                    market_id=row.get('market_id', '')
                )
                ticks.append(tick)
            return sorted(ticks, key=lambda x: x.timestamp)
        except FileNotFoundError:
            print(f"Warning: Data file not found: {self.config.data_path}")
            return self._generate_sample_data()
    
    def _generate_sample_data(self) -> List[TickData]:
        """生成示例数据用于测试"""
        import random
        ticks = []
        base_price = 0.52  # Polymarket Yes 价格
        start_ts = self.config.start_time or 1700000000000
        
        for i in range(1000):
            ts = start_ts + i * 1000  # 每秒一个 tick
            noise = random.uniform(-0.01, 0.01)
            price = base_price + noise
            
            tick = TickData(
                timestamp=ts,
                source=DataSource.POLYMARKET_ORDERBOOK,
                last_price=price,
                bid_price=price - 0.001,
                ask_price=price + 0.001,
                bid_size=100.0,
                ask_size=100.0,
                implied_probability=price,
                volume=random.uniform(10, 50),
                market_id="btc-up-5m-test"
            )
            ticks.append(tick)
        
        return ticks
    
    async def run(self) -> BacktestReport:
        """运行回测"""
        print(f"Starting backtest: {self.config.strategy_name}")
        print(f"Period: {datetime.fromtimestamp(self.config.start_time/1000)} to {datetime.fromtimestamp(self.config.end_time/1000)}")
        print(f"Initial Capital: ${self.config.initial_capital}")
        
        ticks = await self.load_data()
        if not ticks:
            print("No data loaded, aborting backtest")
            return self._generate_empty_report()
        
        # 回放循环
        for tick in ticks:
            self.orderbook = OrderBookSimulator(tick)
            
            # 调用策略
            if self.on_tick_callback:
                self.on_tick_callback(tick)
            
            # 获取信号并执行
            if self.on_signal_callback:
                signal = self.on_signal_callback(tick)
                await self._execute_signal(signal, tick)
            
            # 更新资金曲线
            if self.position.is_open:
                unrealized = self.position.unrealized_pnl(tick.last_price)
                current_capital = self.capital + unrealized
            else:
                current_capital = self.capital
            
            # 计算回撤
            if current_capital > self.peak_capital:
                self.peak_capital = current_capital
            drawdown = (self.peak_capital - current_capital) / self.peak_capital
            if drawdown > self.max_drawdown:
                self.max_drawdown = drawdown
        
        # 强制平掉所有未平仓
        if self.position.is_open and ticks:
            final_tick = ticks[-1]
            await self._close_position(final_tick, "Backtest_End")
        
        return self._generate_report()
    
    async def _execute_signal(self, signal: str, tick: TickData):
        """执行交易信号"""
        # 收盘前 30 秒逻辑检查
        market_end_time = (tick.timestamp // 300000 + 1) * 300000  # 5 分钟边界
        time_to_close = (market_end_time - tick.timestamp) / 1000  # 秒
        
        if signal == "Hold":
            return
        
        # 如果快到期了，只允许平仓不允许开新仓
        if time_to_close < 5 and not self.position.is_open:
            print(f"[{tick.timestamp}] Too late to open new position ({time_to_close:.1f}s left)")
            return
        
        if signal == "Buy" and not self.position.is_open:
            # 开多
            quantity = self.capital * 0.95 / tick.ask_price  # 使用 95% 资金
            fill_price = tick.ask_price * (1 + self.config.slippage_bps / 10000)
            fees = quantity * fill_price * self.config.taker_fee
            
            if quantity * fill_price + fees <= self.capital:
                self.capital -= (quantity * fill_price + fees)
                self.position.open(quantity, fill_price, tick.market_id, tick.timestamp)
                print(f"[{tick.timestamp}] BUY {quantity:.2f} @ {fill_price:.4f}, Fees: ${fees:.2f}")
        
        elif signal == "Sell" and self.position.is_open:
            # 平仓
            await self._close_position(tick, "Signal_Close")
    
    async def _close_position(self, tick: TickData, reason: str):
        """平仓"""
        if not self.position.is_open:
            return
        
        fill_price = tick.bid_price * (1 - self.config.slippage_bps / 10000)
        pnl = self.position.close(fill_price)
        fees = abs(pnl) * self.config.taker_fee if pnl != 0 else 0
        
        # 更新资金
        self.capital += (abs(pnl) if pnl > 0 else 0) - fees
        
        # 记录交易
        entry = TradeLogEntry(
            timestamp=tick.timestamp,
            market_id=self.position.market_id,
            side="Buy",
            entry_price=self.position.avg_entry_price,
            exit_price=fill_price,
            quantity=self.position.quantity,
            pnl=pnl,
            fees=fees,
            reason=reason
        )
        self.trade_log.append(entry)
        print(f"[{tick.timestamp}] CLOSE {reason}: PnL=${pnl:.2f}, Fees=${fees:.2f}")
    
    def _generate_report(self) -> BacktestReport:
        """生成回测报告"""
        winning = [t for t in self.trade_log if t.pnl > 0]
        losing = [t for t in self.trade_log if t.pnl <= 0]
        
        total_pnl = sum(t.pnl for t in self.trade_log)
        total_fees = sum(t.fees for t in self.trade_log)
        
        # 计算夏普比率 (简化版)
        if len(self.daily_returns) > 1:
            import numpy as np
            sharpe = np.mean(self.daily_returns) / (np.std(self.daily_returns) + 1e-9) * np.sqrt(252)
        else:
            sharpe = 0.0
        
        report = BacktestReport(
            total_trades=len(self.trade_log),
            winning_trades=len(winning),
            losing_trades=len(losing),
            win_rate=len(winning) / max(len(self.trade_log), 1),
            total_pnl=total_pnl,
            max_drawdown=self.max_drawdown,
            sharpe_ratio=sharpe,
            final_capital=self.capital,
            total_fees=total_fees,
            trade_log=self.trade_log
        )
        
        print("\n" + "="*50)
        print("BACKTEST REPORT")
        print("="*50)
        for key, value in report.to_dict().items():
            print(f"{key}: {value}")
        print("="*50 + "\n")
        
        return report
    
    def _generate_empty_report(self) -> BacktestReport:
        """生成空报告"""
        return BacktestReport(
            final_capital=self.config.initial_capital
        )
