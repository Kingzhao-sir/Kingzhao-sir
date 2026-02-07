from __future__ import annotations

import logging
from dataclasses import dataclass, field
from datetime import datetime

from .broker import LiveBroker, SimBroker
from .config import EngineConfig, RunMode
from .data import CCXTOHLCVFeed, HistoricalCSVFeed
from .models import Bar
from .risk import RiskLimits
from .strategy import Strategy

logger = logging.getLogger(__name__)


@dataclass
class EngineState:
    last_bar: Bar | None = None
    last_price: float | None = None
    equity_curve: list[tuple[datetime, float]] = field(default_factory=list)


class TradingEngine:
    def __init__(self, config: EngineConfig, strategy: Strategy):
        self.config = config
        self.strategy = strategy
        self.state = EngineState()
        self.risk_limits = RiskLimits(
            max_position_pct=config.max_position_pct,
            risk_per_trade_pct=config.risk_per_trade_pct,
        )
        self.sim_broker = SimBroker(
            cash=config.cash,
            fee_rate=config.fee_rate,
            slippage_bps=config.slippage_bps,
        )
        self.live_broker = LiveBroker(config.market_type)

    def _data_feed(self):
        if self.config.data_csv:
            return HistoricalCSVFeed(self.config.data_csv)
        return CCXTOHLCVFeed(
            symbol=self.config.symbol,
            timeframe=self.config.timeframe,
            market_type=self.config.market_type,
            limit=self.config.lookback_limit,
        )

    def run(self) -> EngineState:
        if self.config.run_mode == RunMode.LIVE:
            return self._run_live()
        return self._run_sim()

    def _run_sim(self) -> EngineState:
        feed = self._data_feed()
        for bar in feed.bars():
            self._handle_bar(bar, live=False)
        return self.state

    def _run_live(self) -> EngineState:
        feed = self._data_feed()
        for bar in feed.bars():
            self._handle_bar(bar, live=True)
        return self.state

    def _handle_bar(self, bar: Bar, live: bool) -> None:
        self.state.last_bar = bar
        self.state.last_price = bar.close
        orders = self.strategy.on_bar(bar)
        for order in orders:
            if live:
                logger.info("Live order %s", order)
                self.live_broker.submit_order(order)
            else:
                self.sim_broker.submit_order(order, last_price=bar.close)
        equity = self.sim_broker.equity({self.config.symbol: bar.close})
        self.state.equity_curve.append((bar.timestamp, equity))
