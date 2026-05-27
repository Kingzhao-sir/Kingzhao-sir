from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable, Iterator, Protocol

import pandas as pd

from .exchange import build_exchange
from .models import Bar
from .config import MarketType


class DataFeed(Protocol):
    def bars(self) -> Iterable[Bar]:
        ...


@dataclass
class HistoricalCSVFeed:
    csv_path: Path

    def bars(self) -> Iterator[Bar]:
        frame = pd.read_csv(self.csv_path)
        frame["timestamp"] = pd.to_datetime(frame["timestamp"], utc=True)
        for row in frame.itertuples(index=False):
            yield Bar(
                timestamp=row.timestamp.to_pydatetime(),
                open=float(row.open),
                high=float(row.high),
                low=float(row.low),
                close=float(row.close),
                volume=float(row.volume),
            )


@dataclass
class CCXTOHLCVFeed:
    symbol: str
    timeframe: str
    market_type: MarketType
    limit: int = 500

    def bars(self) -> Iterator[Bar]:
        exchange = build_exchange(self.market_type)
        ohlcvs = exchange.fetch_ohlcv(self.symbol, timeframe=self.timeframe, limit=self.limit)
        for timestamp, open_, high, low, close, volume in ohlcvs:
            yield Bar(
                timestamp=datetime.fromtimestamp(timestamp / 1000, tz=timezone.utc),
                open=float(open_),
                high=float(high),
                low=float(low),
                close=float(close),
                volume=float(volume),
            )
