# Quant Trading Framework (Binance + CCXT)

一个面向数字货币现货与合约（USDT 永续）量化交易的 Python 框架，支持 **回测 / 模拟 / 实盘**，并提供简单的 Web 界面与 API。项目以 **CCXT + Binance** 为核心，强调模块化与可扩展性。

## 功能概览

- **交易类型**：现货 / 合约（USDT Perp）
- **运行模式**：回测、模拟（paper）、实盘
- **数据源**：CSV 历史数据、CCXT 行情
- **策略**：内置 EMA 交叉示例，可扩展自定义策略
- **风控与成本**：手续费、滑点、最大仓位、单笔风险
- **Web 管理**：运行状态、日志、启动/停止任务

## 快速开始

### 1. 安装依赖

```bash
pip install -r requirements.txt
```

### 2. 配置环境变量

```bash
export BINANCE_API_KEY=your_key
export BINANCE_API_SECRET=your_secret
```

### 3. 回测

```bash
python main.py backtest \
  --symbol BTC/USDT \
  --timeframe 1h \
  --csv data/btcusdt_1h.csv \
  --cash 10000
```

### 4. 模拟盘

```bash
python main.py paper \
  --symbol BTC/USDT \
  --timeframe 1m \
  --cash 10000
```

### 5. 实盘

```bash
python main.py live \
  --symbol BTC/USDT \
  --timeframe 1m
```

### 6. Web 界面

```bash
uvicorn quant.web.app:app --reload --port 8000
```

访问 `http://localhost:8000`。

## 目录结构

```
quant/
  broker.py            # 回测/模拟/实盘撮合与订单
  config.py            # 统一配置
  data.py              # 数据源
  engine.py            # 运行引擎
  exchange.py          # CCXT + Binance 封装
  logging_utils.py     # 统一日志
  models.py            # 订单/持仓/Bar
  risk.py              # 风控
  strategy.py          # 策略基类
  strategies/          # 内置策略
  web/                 # Web UI
```

## 说明

此框架提供完整“骨架”与关键处理细节，便于后续扩展：

- 多品种、多周期、持久化与更多策略可按需添加。
- 可对接数据库以持久化成交与回测结果。
- 更复杂的撮合（部分成交、深度成交、限价滑点）可在 SimBroker 中实现。
