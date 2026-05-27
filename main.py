from __future__ import annotations

import argparse
from pathlib import Path

from quant.config import EngineConfig, MarketType, RunMode
from quant.engine import TradingEngine
from quant.logging_utils import configure_logging
from quant.strategy import EmaCrossStrategy


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Quant Trading Framework")
    subparsers = parser.add_subparsers(dest="command", required=True)

    for mode in ["backtest", "paper", "live"]:
        cmd = subparsers.add_parser(mode)
        cmd.add_argument("--symbol", required=True)
        cmd.add_argument("--timeframe", required=True)
        cmd.add_argument("--market-type", default="spot", choices=["spot", "futures"])
        cmd.add_argument("--cash", type=float, default=10_000)
        cmd.add_argument("--csv", type=Path)

    return parser


def run_engine(args: argparse.Namespace, run_mode: RunMode) -> None:
    config = EngineConfig(
        symbol=args.symbol,
        timeframe=args.timeframe,
        market_type=MarketType(args.market_type),
        run_mode=run_mode,
        cash=args.cash,
        data_csv=args.csv,
    )
    strategy = EmaCrossStrategy(args.symbol)
    engine = TradingEngine(config, strategy)
    engine.run()


def main() -> None:
    configure_logging()
    parser = build_parser()
    args = parser.parse_args()
    if args.command == "backtest":
        run_engine(args, RunMode.BACKTEST)
    elif args.command == "paper":
        run_engine(args, RunMode.PAPER)
    elif args.command == "live":
        run_engine(args, RunMode.LIVE)


if __name__ == "__main__":
    main()
