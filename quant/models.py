from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from enum import Enum


class Side(str, Enum):
    BUY = "buy"
    SELL = "sell"


class OrderType(str, Enum):
    MARKET = "market"
    LIMIT = "limit"


@dataclass
class Bar:
    timestamp: datetime
    open: float
    high: float
    low: float
    close: float
    volume: float


@dataclass
class Order:
    symbol: str
    side: Side
    order_type: OrderType
    amount: float
    price: float | None = None


@dataclass
class Trade:
    symbol: str
    side: Side
    amount: float
    price: float
    fee: float
    timestamp: datetime


@dataclass
class Position:
    symbol: str
    amount: float = 0.0
    avg_price: float = 0.0
