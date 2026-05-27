from __future__ import annotations

from dataclasses import dataclass


@dataclass
class RiskLimits:
    max_position_pct: float
    risk_per_trade_pct: float

    def position_size(self, equity: float, price: float, stop_distance: float) -> float:
        if price <= 0 or stop_distance <= 0:
            return 0.0
        risk_amount = equity * self.risk_per_trade_pct
        size = risk_amount / stop_distance
        max_size = equity * self.max_position_pct / price
        return min(size, max_size)
