from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from pathlib import Path


class RunMode(str, Enum):
    BACKTEST = "backtest"
    PAPER = "paper"
    LIVE = "live"


class MarketType(str, Enum):
    SPOT = "spot"
    FUTURES = "futures"


@dataclass(frozen=True)
class EngineConfig:
    symbol: str
    timeframe: str
    market_type: MarketType
    run_mode: RunMode
    cash: float = 10_000.0
    fee_rate: float = 0.0004
    slippage_bps: float = 1.0
    max_position_pct: float = 0.3
    risk_per_trade_pct: float = 0.01
    data_csv: Path | None = None
    lookback_limit: int = 500


@dataclass(frozen=True)
class WebConfig:
    host: str = "0.0.0.0"
    port: int = 8000
