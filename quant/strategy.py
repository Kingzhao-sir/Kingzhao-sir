from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import List

from .models import Bar, Order, Side, OrderType


@dataclass
class StrategyState:
    bars: List[Bar] = field(default_factory=list)


class Strategy(ABC):
    def __init__(self, symbol: str):
        self.symbol = symbol
        self.state = StrategyState()

    @abstractmethod
    def on_bar(self, bar: Bar) -> list[Order]:
        raise NotImplementedError


class EmaCrossStrategy(Strategy):
    def __init__(self, symbol: str, fast: int = 9, slow: int = 21):
        super().__init__(symbol)
        self.fast = fast
        self.slow = slow

    def on_bar(self, bar: Bar) -> list[Order]:
        self.state.bars.append(bar)
        if len(self.state.bars) < self.slow:
            return []
        closes = [b.close for b in self.state.bars]
        fast_ema = _ema(closes, self.fast)
        slow_ema = _ema(closes, self.slow)
        if len(fast_ema) < 2 or len(slow_ema) < 2:
            return []
        if fast_ema[-2] <= slow_ema[-2] and fast_ema[-1] > slow_ema[-1]:
            return [Order(self.symbol, Side.BUY, OrderType.MARKET, amount=1.0)]
        if fast_ema[-2] >= slow_ema[-2] and fast_ema[-1] < slow_ema[-1]:
            return [Order(self.symbol, Side.SELL, OrderType.MARKET, amount=1.0)]
        return []


def _ema(values: list[float], period: int) -> list[float]:
    if not values:
        return []
    alpha = 2 / (period + 1)
    ema_values = [values[0]]
    for value in values[1:]:
        ema_values.append(alpha * value + (1 - alpha) * ema_values[-1])
    return ema_values
