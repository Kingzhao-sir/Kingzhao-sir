from __future__ import annotations

import logging
from dataclasses import asdict
from pathlib import Path
from typing import Optional

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import HTMLResponse, JSONResponse
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from pydantic import BaseModel

from ..config import EngineConfig, MarketType, RunMode
from ..engine import TradingEngine
from ..logging_utils import configure_logging
from ..strategy import EmaCrossStrategy

logger = logging.getLogger(__name__)
configure_logging()

BASE_DIR = Path(__file__).resolve().parent

app = FastAPI(title="Quant Trading Framework")
app.mount("/static", StaticFiles(directory=BASE_DIR / "static"), name="static")
templates = Jinja2Templates(directory=BASE_DIR / "templates")

engine: Optional[TradingEngine] = None


class StartRequest(BaseModel):
    symbol: str
    timeframe: str
    market_type: MarketType
    run_mode: RunMode
    cash: float = 10_000.0


@app.get("/", response_class=HTMLResponse)
def index(request: Request):
    return templates.TemplateResponse(
        request=request,
        name="index.html",
        context={},
    )


@app.post("/api/start")
def start(req: StartRequest):
    global engine
    strategy = EmaCrossStrategy(req.symbol)
    config = EngineConfig(
        symbol=req.symbol,
        timeframe=req.timeframe,
        market_type=req.market_type,
        run_mode=req.run_mode,
        cash=req.cash,
    )
    engine = TradingEngine(config, strategy)
    state = engine.run()
    return {"status": "started", "equity_points": len(state.equity_curve)}


@app.get("/api/status")
def status():
    if not engine:
        raise HTTPException(status_code=404, detail="Engine not started")
    state = engine.state
    return {
        "symbol": engine.config.symbol,
        "run_mode": engine.config.run_mode,
        "market_type": engine.config.market_type,
        "last_price": state.last_price,
        "equity_curve": [
            {"time": t.isoformat(), "equity": equity} for t, equity in state.equity_curve[-50:]
        ],
    }


@app.get("/api/config")
def config():
    if not engine:
        raise HTTPException(status_code=404, detail="Engine not started")
    return JSONResponse(asdict(engine.config))
