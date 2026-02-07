from __future__ import annotations

import os
from dataclasses import dataclass

import ccxt

from .config import MarketType


@dataclass
class BinanceCredentials:
    api_key: str
    api_secret: str


def load_credentials() -> BinanceCredentials:
    api_key = os.getenv("BINANCE_API_KEY", "")
    api_secret = os.getenv("BINANCE_API_SECRET", "")
    return BinanceCredentials(api_key=api_key, api_secret=api_secret)


def build_exchange(market_type: MarketType) -> ccxt.binance:
    creds = load_credentials()
    exchange = ccxt.binance(
        {
            "apiKey": creds.api_key,
            "secret": creds.api_secret,
            "enableRateLimit": True,
            "options": {
                "defaultType": "spot" if market_type == MarketType.SPOT else "future"
            },
        }
    )
    return exchange
