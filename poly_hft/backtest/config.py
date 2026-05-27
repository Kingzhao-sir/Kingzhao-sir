"""
回测系统配置模块
支持 Tick 级高精度回测，包含手续费、滑点模拟
"""
from dataclasses import dataclass, field
from typing import Optional, List
from enum import Enum


class DataSource(Enum):
    """数据源类型"""
    BINANCE_SPOT = "binance_spot"
    POLYMARKET_ORDERBOOK = "polymarket_orderbook"
    POLYMARKET_TRADE = "polymarket_trade"


@dataclass
class BacktestConfig:
    """回测配置"""
    strategy_name: str = "BTC_5m_Prediction"
    start_time: int = 0  # Unix Timestamp
    end_time: int = 0
    initial_capital: float = 1000.0  # USDC
    maker_fee: float = 0.0005  # 0.05%
    taker_fee: float = 0.0010  # 0.10%
    slippage_bps: float = 2.0  # 滑点 (基点)
    data_path: str = "./data/backtest_input.csv"
    playback_speed: float = 100.0  # 回放速度倍数
    
    def __post_init__(self):
        if self.start_time >= self.end_time and self.end_time != 0:
            raise ValueError("start_time must be less than end_time")
        if self.initial_capital <= 0:
            raise ValueError("initial_capital must be positive")


@dataclass
class TickData:
    """历史行情数据包 (Tick 级)"""
    timestamp: int  # Unix Timestamp (ms)
    source: DataSource
    last_price: float  # 最新价格
    bid_price: float  # 买一价
    ask_price: float  # 卖一价
    bid_size: float  # 买一量
    ask_size: float  # 卖一量
    implied_probability: Optional[float] = None  # Polymarket 隐含概率 (支持率)
    volume: float = 0.0
    market_id: str = ""
    
    def mid_price(self) -> float:
        """计算中间价"""
        return (self.bid_price + self.ask_price) / 2.0
    
    def spread_bps(self) -> float:
        """计算价差 (基点)"""
        if self.mid_price() == 0:
            return 0.0
        return ((self.ask_price - self.bid_price) / self.mid_price()) * 10000


@dataclass
class TradeLogEntry:
    """交易记录"""
    timestamp: int
    market_id: str
    side: str  # "Buy" or "Sell"
    entry_price: float
    exit_price: float
    quantity: float
    pnl: float
    fees: float
    reason: str  # e.g., "Signal_Close", "Timeout", "Stop_Loss"


@dataclass
class BacktestReport:
    """回测结果统计"""
    total_trades: int = 0
    winning_trades: int = 0
    losing_trades: int = 0
    win_rate: float = 0.0
    total_pnl: float = 0.0
    max_drawdown: float = 0.0
    sharpe_ratio: float = 0.0
    final_capital: float = 0.0
    total_fees: float = 0.0
    trade_log: List[TradeLogEntry] = field(default_factory=list)
    
    def to_dict(self) -> dict:
        """转换为字典格式用于 JSON 序列化"""
        return {
            "total_trades": self.total_trades,
            "winning_trades": self.winning_trades,
            "losing_trades": self.losing_trades,
            "win_rate": round(self.win_rate * 100, 2),  # 百分比
            "total_pnl": round(self.total_pnl, 2),
            "max_drawdown": round(self.max_drawdown, 2),
            "sharpe_ratio": round(self.sharpe_ratio, 2),
            "final_capital": round(self.final_capital, 2),
            "total_fees": round(self.total_fees, 2),
            "trade_count_by_reason": self._count_by_reason(),
        }
    
    def _count_by_reason(self) -> dict:
        """按平仓原因统计交易次数"""
        from collections import Counter
        reasons = [t.reason for t in self.trade_log]
        return dict(Counter(reasons))
