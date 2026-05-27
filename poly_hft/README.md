# Polymarket 5 分钟 BTC 涨跌预测交易系统

## 系统架构

### 核心设计原则
- **收盘前 30 秒决策**: 在每个 5 分钟周期结束前 30 秒给出买卖判断
- **最后 5 秒禁止开仓**: 避免结算期间的不确定性
- **多源数据融合**: 支持率 + 实时价格 + 1 分钟/5 分钟 K 线 + 订单簿

### 技术栈
- **核心引擎**: Python 3.12 + asyncio (高性能异步 IO)
- **数据处理**: numpy + pandas (向量化计算)
- **Web 后端**: FastAPI (异步 API)
- **前端**: Next.js + Lightweight Charts
- **数据库**: SQLite (回测) + Redis (实时缓存)

## 目录结构

```
poly_hft/
├── core/                  # 核心引擎
│   ├── __init__.py
│   ├── market_router.py   # 动态市场路由
│   ├── data_gateway.py    # 数据摄取网关
│   ├── order_book.py      # 订单簿管理
│   └── signal_engine.py   # 信号计算引擎
├── engine/                # 交易执行
│   ├── __init__.py
│   ├── risk_manager.py    # 风控引擎
│   ├── execution.py       # 订单执行
│   └── portfolio.py       # 持仓管理
├── backtest/              # 回测系统
│   ├── __init__.py
│   ├── data_loader.py     # 历史数据加载
│   ├── simulator.py       # 回测模拟器
│   └── analyzer.py        # 绩效分析
├── api/                   # Web API
│   ├── __init__.py
│   ├── main.py            # FastAPI 应用
│   └── routes.py          # API 路由
├── web/                   # 前端界面
│   ├── page.tsx
│   └── components/
├── config/                # 配置文件
│   └── settings.py
├── tests/                 # 测试用例
│   ├── test_market_router.py
│   ├── test_order_book.py
│   └── test_signal_engine.py
├── requirements.txt
└── README.md
```

## 开发路线图

### Phase 1: 数据与基建
- [x] 项目结构设计
- [ ] 实现 Market Router
- [ ] 实现 Data Gateway
- [ ] 实现 Order Book
- [ ] 单元测试

### Phase 2: 信号与执行
- [ ] 实现 Signal Engine
- [ ] 实现 Risk Manager
- [ ] 实现 Execution Engine
- [ ] 集成测试

### Phase 3: 回测系统
- [ ] 实现数据加载器
- [ ] 实现回测模拟器
- [ ] 实现绩效分析
- [ ] 策略优化

### Phase 4: Web 界面
- [ ] FastAPI 后端
- [ ] Next.js 前端
- [ ] 实时监控面板
- [ ] 部署测试
