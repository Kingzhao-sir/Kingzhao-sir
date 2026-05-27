from __future__ import annotations

import logging
from dataclasses import dataclass, field
from datetime import datetime, timezone

from .exchange import build_exchange
from .models import Order, Position, Side, Trade
from .config import MarketType

logger = logging.getLogger(__name__)


@dataclass
class SimBroker:
    cash: float
    fee_rate: float
    slippage_bps: float
    positions: dict[str, Position] = field(default_factory=dict)
    trades: list[Trade] = field(default_factory=list)

    def equity(self, last_prices: dict[str, float]) -> float:
        total = self.cash
        for symbol, position in self.positions.items():
            price = last_prices.get(symbol, position.avg_price)
            total += position.amount * price
        return total

    def submit_order(self, order: Order, last_price: float) -> Trade:
        price = self._apply_slippage(last_price, order.side)
        fee = abs(order.amount * price) * self.fee_rate
        cost = order.amount * price + fee if order.side == Side.BUY else -order.amount * price - fee
        self.cash -= cost
        position = self.positions.setdefault(order.symbol, Position(order.symbol))
        if order.side == Side.BUY:
            position.avg_price = _weighted_price(position, order.amount, price)
            position.amount += order.amount
        else:
            position.amount -= order.amount
        trade = Trade(
            symbol=order.symbol,
            side=order.side,
            amount=order.amount,
            price=price,
            fee=fee,
            timestamp=datetime.now(timezone.utc),
        )
        self.trades.append(trade)
        logger.info("Sim trade: %s", trade)
        return trade

    def _apply_slippage(self, price: float, side: Side) -> float:
        slip = price * (self.slippage_bps / 10_000)
        return price + slip if side == Side.BUY else price - slip


@dataclass
class LiveBroker:
    market_type: MarketType

    def __post_init__(self) -> None:
        self.exchange = build_exchange(self.market_type)

    def submit_order(self, order: Order) -> dict:
        params = {}
        response = self.exchange.create_order(
            symbol=order.symbol,
            type=order.order_type.value,
            side=order.side.value,
            amount=order.amount,
            price=order.price,
            params=params,
        )
        return response


def _weighted_price(position: Position, amount: float, price: float) -> float:
    if position.amount <= 0:
        return price
    total_cost = position.amount * position.avg_price + amount * price
    total_amount = position.amount + amount
    return total_cost / total_amount if total_amount else price
