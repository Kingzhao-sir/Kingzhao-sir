# Polymarket 5分钟 BTC 涨跌预测交易系统设计方案

## 一、需求核心分析

### 业务特征
- **交易标的**：Polymarket 5分钟BTC涨跌二元期权（Yes/No Shares）
- **决策时点**：每个周期收盘前30秒给出最终判断并执行
- **关键因子**：
  - Polymarket支持率（Yes份额价格，隐含概率）
  - Polymarket实时订单簿（买卖档深度、价差）
  - 1分钟K线（短期动量）
  - 5分钟K线（周期趋势）
  - 外部交易所实时价格（Binance/OKX作为领先指标）
- **核心挑战**：市场每5分钟轮换新的Token ID，需无缝切换

### 性能目标
- **内存占用**：< 200MB（核心引擎）
- **决策延迟**：从数据接收到信号生成 < 5ms
- **回测速度**：1个月历史数据回测 < 30秒
- **Web并发**：支持50+实时监控连接

---

## 二、精简架构设计（务实版HFT）

摒弃过度设计的"微秒级优化"，聚焦**毫秒级可靠决策**和**策略有效性验证**。

```
┌─────────────────────────────────────────────────────────────────┐
│                    Web 监控与控制面 (Go + Next.js)               │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐   │
│  │ Next.js Dashboard│◄──►│ Go REST API │◄──►│ Redis (状态缓存) │   │
│  └──────────────┘    └──────────────┘    └──────────────────┘   │
└────────────────────────────┬────────────────────────────────────┘
                             │ gRPC / WebSocket
┌────────────────────────────▼────────────────────────────────────┐
│                  核心交易引擎 (Rust + Tokio)                     │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────────┐     │
│  │ 数据聚合层   │──►│ 特征工程层   │──►│ 信号决策引擎     │     │
│  │ (Data Agg)   │   │ (Features)   │   │ (Signal Engine)  │     │
│  └──────┬───────┘   └──────┬───────┘   └────────┬─────────┘     │
│         │                  │                     │               │
│  ┌──────▼───────┐   ┌──────▼───────┐   ┌────────▼─────────┐     │
│  │ 市场路由器   │   │ 风控检查     │   │ 订单执行器       │     │
│  │ (5min轮动)   │   │ (Risk Check) │   │ (Polymarket API) │     │
│  └──────────────┘   └──────────────┘   └──────────────────┘     │
└─────────────────────────────────────────────────────────────────┘
         │                                        │
         ▼                                        ▼
┌─────────────────┐                    ┌─────────────────────┐
│ 外部数据源       │                    │ Polymarket API      │
│ - Binance WS    │                    │ - REST (下单/查询)  │
│ - OKX WS        │                    │ - WS (订单簿)       │
│ - Gamma API     │                    │ - Gamma (市场元数据)│
└─────────────────┘                    └─────────────────────┘
         │
         ▼
┌─────────────────┐
│ QuestDB (可选)   │
│ 存储Tick/K线数据 │
│ 用于回测         │
└─────────────────┘
```

---

## 三、技术栈选型（平衡性能与开发效率）

| 模块 | 技术选型 | 理由 |
|------|----------|------|
| **核心引擎** | Rust (Tokio异步运行时) | 无GC停顿，内存安全，二进制小(<30MB)，生态成熟 |
| **Web后端** | Go (Chi路由 + gorilla/websocket) | 高并发WS连接，开发效率高，内存占用低 |
| **前端** | Next.js + TradingView Lightweight Charts | Canvas渲染，避免DOM瓶颈，实时K线/订单簿可视化 |
| **数据存储** | SQLite (回测) + Redis (实时状态) | 简单可靠，无需重型DB；Redis做热点缓存和Pub/Sub |
| **序列化** | MessagePack (rmp-serde) | 比JSON快3-5倍，比Protobuf开发友好，足够轻量 |
| **内部通信** | Tokio MPSC通道 + gRPC (引擎↔Web) | 无锁队列传递数据，gRPC用于状态查询 |

**关键取舍**：
- ❌ 不用ONNX：5分钟周期决策不需要复杂模型推理，规则+统计特征足够
- ❌ 不用CPU Pinning：云服务器虚拟化环境下效果有限，增加运维复杂度
- ❌ 不用零拷贝分配器：标准分配器在毫秒级场景完全够用
- ✅ 保留对象池：订单簿重建时复用数据结构，减少GC压力
- ✅ 保留环形缓冲区：K线计算用固定大小RingBuffer，避免动态扩容

---

## 四、核心模块详细设计

### 4.1 动态市场路由器（5分钟轮动核心）

**问题**：Polymarket每5分钟生成新Token ID，旧市场结算后无法交易。

**解决方案**：
```rust
struct MarketRouter {
    current_market: MarketInfo,      // 当前交易的市场
    next_market: Option<MarketInfo>, // 预热的下一个市场
    switch_countdown: u64,           // 距离切换的秒数
}

struct MarketInfo {
    condition_id: String,
    yes_token_id: String,
    no_token_id: String,
    start_time: u64,                 // 周期开始时间戳(秒)
    end_time: u64,                   // 周期结束时间戳(秒)
    ws_subscribed: bool,             // WS是否已订阅
}

// 工作流程：
// T-60秒: 通过Gamma API获取下一个周期的condition_id和token_ids
// T-30秒: 预建立下一个市场的WS连接，订阅订单簿
// T-5秒:  触发"撤消所有挂单"，停止新开仓
// T-0秒:  切换到next_market作为current_market，开始新周期交易
// T+30秒: 旧市场结算完成，释放资源
```

**实现要点**：
- 使用`tokio::time::interval`精确计时，误差<10ms
- Gamma API调用加重试机制（指数退避）
- WS连接池复用，避免频繁握手

---

### 4.2 数据聚合层（多源数据融合）

**输入数据流**：
1. **Binance/OKX WebSocket**：BTC/USDT实时成交和订单簿（领先指标）
2. **Polymarket WebSocket**：Yes/No订单簿更新（L2数据）
3. **Polymarket REST**：当前用户持仓、挂单状态（轮询，1秒/次）
4. **本地K线生成器**：基于外部交易所数据合成1分钟/5分钟K线

**数据处理管道**：
```rust
// 使用Tokio Stream组合多个数据源
let binance_stream = connect_binance_ws();
let polymarket_stream = connect_polymarket_ws();
let kline_generator = KlineGenerator::new();

// 合并流，按时间戳排序
let merged_stream = select_all(vec![binance_stream, polymarket_stream]);

// 实时计算特征
merged_stream
    .map(|tick| compute_features(tick, &kline_state))
    .filter(|features| features.is_valid())
    .forward(signal_engine);
```

**关键特征计算**（每个Tick更新）：
```rust
struct Features {
    // Polymarket特有
    support_rate: f64,          // Yes份额价格 (0.0-1.0)
    orderbook_imbalance: f64,   // (买量-卖量)/(买量+卖量)
    bid_ask_spread: f64,        // 相对价差
    
    // 跨市场价差
    price_premium: f64,         // (Polymarket隐含价格 - Binance价格) / Binance价格
    
    // K线特征 (1分钟)
    kline_1m_momentum: f64,     // 当前价/1分钟前价 - 1
    kline_1m_volume_ratio: f64, // 当前分钟成交量/过去5分钟均量
    
    // K线特征 (5分钟)
    kline_5m_trend: i8,         // +1上涨, -1下跌, 0震荡
    
    // 时间特征
    seconds_to_close: u64,      // 距离收盘秒数
}
```

---

### 4.3 信号决策引擎（核心策略逻辑）

**决策时机**：收盘前30秒开始评估，最后5秒禁止开新仓。

**策略框架**（可扩展多策略）：
```rust
enum Signal {
    BuyYes(f64),    // 买入Yes，参数为仓位比例(0.0-1.0)
    BuyNo(f64),     // 买入No
    Hold,           // 持有不动
    CloseAll,       // 平仓所有
}

struct SignalEngine {
    rules: Vec<Box<dyn StrategyRule>>,
}

trait StrategyRule {
    fn evaluate(&self, features: &Features, state: &TradingState) -> Option<Signal>;
    fn priority(&self) -> u8; // 高优先级规则先执行
}

// 示例规则1: 支持率均值回归
struct SupportRateMeanReversion {
    threshold_low: f64,   // 如0.35
    threshold_high: f64,  // 如0.65
};

impl StrategyRule for SupportRateMeanReversion {
    fn evaluate(&self, features: &Features, _state: &TradingState) -> Option<Signal> {
        if features.seconds_to_close > 30 || features.seconds_to_close < 5 {
            return None; // 不在决策窗口内
        }
        
        if features.support_rate < self.threshold_low {
            Some(Signal::BuyYes(0.5)) // 支持率过低，做多反弹
        } else if features.support_rate > self.threshold_high {
            Some(Signal::BuyNo(0.5))  // 支持率过高，做空回调
        } else {
            None
        }
    }
}

// 示例规则2: 跨市场价差套利
struct CrossExchangeArbitrage {
    min_premium: f64, // 最小价差阈值
};

impl StrategyRule for CrossExchangeArbitrage {
    fn evaluate(&self, features: &Features, _state: &TradingState) -> Option<Signal> {
        if features.price_premium > self.min_premium {
            Some(Signal::BuyNo(0.3)) // Polymarket溢价过高，买No
        } else if features.price_premium < -self.min_premium {
            Some(Signal::BuyYes(0.3)) // Polymarket折价，买Yes
        } else {
            None
        }
    }
}

// 示例规则3: K线动量突破
struct KlineMomentumBreakout {
    momentum_threshold: f64,
};

impl StrategyRule for KlineMomentumBreakout {
    fn evaluate(&self, features: &Features, _state: &TradingState) -> Option<Signal> {
        if features.kline_1m_momentum > self.momentum_threshold {
            Some(Signal::BuyYes(0.4))
        } else if features.kline_1m_momentum < -self.momentum_threshold {
            Some(Signal::BuyNo(0.4))
        } else {
            None
        }
    }
}
```

**信号融合逻辑**：
- 按优先级执行规则，第一个返回非None信号的规则生效
- 或采用投票机制：多个规则同向信号叠加仓位
- 风控覆盖：任何信号都需通过事前风控检查

---

### 4.4 订单执行器（Polymarket CLOB集成）

**核心功能**：
1. EIP-712签名生成（使用`ethers-signers`或`alloy`库）
2. REST API下单/撤单（带重试和速率限制）
3. 订单状态跟踪（Pending→Filled/Cancelled）

**EIP-712签名优化**：
```rust
// 预计算Nonce，减少签名时的链上查询
struct OrderSigner {
    private_key: SigningKey,
    nonce: AtomicU64,
    chain_id: u64,
    verifying_contract: Address,
}

impl OrderSigner {
    async fn sign_order(&self, order: &Order) -> Signature {
        let nonce = self.nonce.fetch_add(1, Ordering::SeqCst);
        
        // 构建EIP-712 Domain
        let domain = Eip712Domain {
            name: "PolyMarket".to_string(),
            version: "1".to_string(),
            chain_id: self.chain_id,
            verifying_contract: self.verifying_contract,
        };
        
        // 构建Order Struct Hash
        let order_hash = hash_order(order, nonce);
        
        // 签名 (使用k256库，单次<1ms)
        sign_hash(&self.private_key, &order_hash)
    }
}
```

**执行策略**：
- **Maker优先**：默认使用Post-Only挂单，赚取返佣
- **紧急Taker**：收盘前10秒如需平仓，使用Immediate-or-Cancel市价单
- **拆单逻辑**：单笔>500 USDC时，拆分为3-5笔小单，间隔100ms

---

### 4.5 风控引擎（三层防护）

```rust
struct RiskEngine {
    daily_loss_limit: f64,      // 日最大亏损 (USDC)
    single_order_limit: f64,    // 单笔最大金额
    max_position: f64,          // 最大总持仓
    rate_limiter: RateLimiter,  // API速率限制
}

// 事前风控 (Pre-Trade)
fn pre_trade_check(&self, order: &Order, state: &TradingState) -> Result<(), RiskError> {
    // 1. 检查单笔金额
    if order.amount > self.single_order_limit {
        return Err(RiskError::OrderTooLarge);
    }
    
    // 2. 检查日亏损
    if state.daily_pnl < -self.daily_loss_limit {
        return Err(RiskError::DailyLossExceeded);
    }
    
    // 3. 检查总持仓
    if state.total_position + order.amount > self.max_position {
        return Err(RiskError::PositionLimitExceeded);
    }
    
    // 4. 检查API速率
    if !self.rate_limiter.check() {
        return Err(RiskError::RateLimitExceeded);
    }
    
    // 5. 检查价格偏离 (防止乌龙指)
    if (order.price - state.fair_price).abs() / state.fair_price > 0.1 {
        return Err(RiskError::PriceDeviationTooHigh);
    }
    
    Ok(())
}

// 事中风控 (Intra-Trade): 监控成交滑点，异常则暂停
// 事后风控 (Post-Trade): 每5分钟对账本地OMS与交易所实际持仓
```

---

### 4.6 回测系统（历史数据驱动）

**架构**：
```rust
struct Backtester {
    data_source: Box<dyn TickDataSource>, // SQLite/CSV
    strategy: Box<dyn StrategyRule>,
    initial_capital: f64,
}

impl Backtester {
    async fn run(&mut self, start: u64, end: u64) -> BacktestResult {
        let mut state = TradingState::new(self.initial_capital);
        let mut trades = Vec::new();
        
        // 按时间顺序回放Tick数据
        let mut stream = self.data_source.get_stream(start, end);
        
        while let Some(tick) = stream.next().await {
            // 1. 更新K线
            state.update_klines(&tick);
            
            // 2. 计算特征
            let features = compute_features(&tick, &state);
            
            // 3. 生成信号
            if let Some(signal) = self.strategy.evaluate(&features, &state) {
                // 4. 模拟撮合 (使用当时订单簿)
                let fill = simulate_fill(&signal, &tick.orderbook);
                
                // 5. 更新状态
                state.apply_fill(&fill);
                trades.push(fill);
            }
            
            // 6. 处理市场轮换
            if tick.timestamp % 300 == 0 {
                state.rotate_market();
            }
        }
        
        BacktestResult {
            total_pnl: state.capital - self.initial_capital,
            win_rate: calculate_win_rate(&trades),
            sharpe_ratio: calculate_sharpe(&trades),
            max_drawdown: calculate_max_drawdown(&trades),
            trade_count: trades.len(),
        }
    }
}
```

**回测数据准备**：
1. 使用脚本录制1-2周真实Tick数据（Binance + Polymarket）
2. 存入SQLite（按市场ID分表）
3. 回测时按时间戳精确回放，模拟真实延迟（+50ms网络延迟）

**输出指标**：
- 总收益、胜率、夏普比率、最大回撤
- 按时间段分析（亚洲/欧洲/美洲时段表现差异）
- 特征重要性分析（哪个因子贡献最大）

---

## 五、Web监控系统设计

### 5.1 Go后端API

```go
// 主要端点
GET  /api/status          // 系统状态（运行中/停止、当前市场、PnL）
GET  /api/positions       // 当前持仓
GET  /api/signals         // 最近10个信号及结果
WS   /ws/live             // WebSocket推送实时数据
POST /api/start           // 启动交易引擎
POST /api/stop            // 停止交易引擎
GET  /api/backtest        // 触发回测并返回结果
```

**WebSocket推送数据结构**：
```json
{
  "timestamp": 1704067200,
  "market": {
    "condition_id": "0x...",
    "seconds_to_close": 45,
    "yes_price": 0.52,
    "no_price": 0.48
  },
  "features": {
    "support_rate": 0.52,
    "orderbook_imbalance": 0.15,
    "price_premium": 0.003,
    "kline_1m_momentum": 0.002
  },
  "position": {
    "yes_amount": 100,
    "no_amount": 0,
    "unrealized_pnl": 2.5
  },
  "last_signal": {
    "type": "BuyYes",
    "confidence": 0.7,
    "timestamp": 1704067180
  }
}
```

### 5.2 Next.js前端Dashboard

**页面布局**：
```
┌────────────────────────────────────────────────────────────┐
│  [系统状态] 运行中 ●  |  当前PnL: +12.5 USDC  |  胜率: 65%  │
├────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────────────────────┐  │
│  │   实时K线图     │  │      订单簿可视化 (L2)           │  │
│  │  (1分钟 + 5分钟)│  │  买档 ████████  卖档 ████       │  │
│  └─────────────────┘  └─────────────────────────────────┘  │
├────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────┐   │
│  │              关键特征仪表盘                          │   │
│  │  支持率: 52%  │  价差: 0.8%  │  跨市场价差: +0.3%   │   │
│  └─────────────────────────────────────────────────────┘   │
├────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────────────────────┐  │
│  │   最近信号列表   │  │      持仓详情                   │  │
│  │  [14:30] BuyYes │  │  Yes: 100 shares @ 0.48        │  │
│  │  [14:25] Hold   │  │  未实现盈亏: +2.5 USDC         │  │
│  └─────────────────┘  └─────────────────────────────────┘  │
├────────────────────────────────────────────────────────────┤
│  [回测面板] 选择日期范围 → 运行回测 → 显示收益曲线/指标     │
└────────────────────────────────────────────────────────────┘
```

**技术实现**：
- 使用`recharts`或`lightweight-charts`绘制K线和收益曲线
- `Socket.IO`或原生WebSocket接收实时推送
- `TanStack Query`管理API状态和缓存
- 深色主题，减少长时间监控的眼疲劳

---

## 六、部署架构（低成本高性能）

### 6.1 服务器配置推荐

**方案A：单服务器（个人开发者）**
```
AWS EC2: c6i.xlarge (4 vCPU, 8GB RAM) 或同等配置
区域: ap-southeast-1 (新加坡) 或 ap-northeast-1 (东京)
成本: ~$60/月

部署内容:
- Rust交易引擎 (Docker容器，限制2核2GB)
- Go Web后端 (Docker容器，限制1核512MB)
- Redis (Docker容器，限制512MB)
- SQLite文件 (本地磁盘)
- Next.js前端 (Vercel免费部署，不占服务器资源)
```

**方案B：双机高可用（生产环境）**
```
主节点: c6i.xlarge (新加坡) - 运行交易引擎 + Redis
备节点: c6i.large (新加坡) - 热备份，通过Redis哨兵模式选举
前端: Vercel/Cloudflare Pages
成本: ~$90/月
```

### 6.2 Docker Compose配置示例

```yaml
version: '3.8'
services:
  trading-engine:
    build: ./engine
    image: polymarket-trader:latest
    container_name: pm-trader
    restart: unless-stopped
    cpus: '2.0'
    memory: 2g
    environment:
      - RUST_LOG=info
      - BINANCE_WS_URL=wss://stream.binance.com:9443/ws
      - POLYMARKET_API_KEY=${POLYMARKET_API_KEY}
      - PRIVATE_KEY=${PRIVATE_KEY}
    volumes:
      - ./config:/app/config
      - ./data:/app/data
    networks:
      - trader-net

  web-backend:
    build: ./web-backend
    image: pm-web:latest
    container_name: pm-web
    restart: unless-stopped
    cpus: '1.0'
    memory: 512m
    ports:
      - "8080:8080"
    environment:
      - REDIS_URL=redis://redis:6379
      - TRADER_GRPC_URL=trading-engine:50051
    depends_on:
      - redis
      - trading-engine
    networks:
      - trader-net

  redis:
    image: redis:7-alpine
    container_name: pm-redis
    restart: unless-stopped
    cpus: '0.5'
    memory: 512m
    command: redis-server --appendonly yes
    volumes:
      - redis-data:/data
    networks:
      - trader-net

networks:
  trader-net:
    driver: bridge

volumes:
  redis-data:
```

---

## 七、开发路线图（8周完成MVP）

### Week 1-2: 基础设施搭建
- [ ] Rust项目初始化，集成Tokio、serde、reqwest、tungstenite
- [ ] 实现Binance WebSocket连接器（订阅BTC/USDT）
- [ ] 实现Polymarket Gamma API客户端（获取市场列表）
- [ ] 实现市场路由器（5分钟轮动逻辑）
- [ ] 编写单元测试和集成测试

### Week 3-4: 核心交易逻辑
- [ ] 实现Polymarket EIP-712签名和REST下单
- [ ] 实现订单簿重建和数据聚合层
- [ ] 实现特征计算模块（K线、支持率、价差等）
- [ ] 实现基础策略规则（支持率均值回归）
- [ ] 实现风控引擎（事前检查）
- [ ] 模拟盘测试（Paper Trading）

### Week 5: 回测系统
- [ ] 设计SQLite数据存储格式
- [ ] 编写数据录制脚本（记录1周真实数据）
- [ ] 实现回测引擎（Tick级回放）
- [ ] 回测策略参数优化
- [ ] 生成回测报告（收益曲线、指标）

### Week 6: Web后端
- [ ] Go项目初始化，集成Chi、gorilla/websocket
- [ ] 实现gRPC客户端（连接Rust引擎）
- [ ] 实现REST API端点
- [ ] 实现WebSocket实时推送
- [ ] 集成Redis状态缓存

### Week 7: 前端Dashboard
- [ ] Next.js项目初始化
- [ ] 实现实时K线图（Lightweight Charts）
- [ ] 实现订单簿可视化
- [ ] 实现信号列表和持仓展示
- [ ] 实现回测结果展示
- [ ] 响应式设计和深色主题

### Week 8: 联调与上线
- [ ] 端到端测试（数据流→信号→下单→监控）
- [ ] 性能压测（并发连接、内存占用）
- [ ] 部署到云服务器
- [ ] 配置监控告警（Prometheus + Grafana）
- [ ] 小资金实盘测试（<100 USDC）
- [ ] 文档编写和代码审查

---

## 八、风险与应对

| 风险 | 影响 | 应对措施 |
|------|------|----------|
| Polymarket API限流 | 无法及时下单/撤单 | 实现令牌桶限流，优先保证撤单请求；降级为轮询模式 |
| 网络延迟突增 | 错过最佳成交价 | 设置最大允许延迟阈值，超时就放弃该周期交易 |
| 策略失效 | 连续亏损 | 设置日亏损熔断（-5%自动停止），定期重新回测优化参数 |
| 智能合约风险 | 资金损失 | 仅使用官方CLOB，不交互未知合约；单市场最大投入<10%总资金 |
| 5分钟轮换失败 | 空窗期无法交易 | 提前60秒预热，失败时自动跳过该周期，记录日志告警 |

---

## 九、预期性能指标

| 指标 | 目标值 | 测量方法 |
|------|--------|----------|
| 核心引擎内存占用 | < 150 MB | `htop`监控RSS |
| 决策延迟 (P99) | < 10 ms | OpenTelemetry追踪 |
| Web并发连接数 | > 50 | wrk压测 |
| 回测速度 (1个月数据) | < 60秒 | 实际运行计时 |
| 系统正常运行时间 | > 99% | Prometheus uptime监控 |
| 单周期交易成本 | < 0.5 USDC | 统计Gas费+滑点 |

---

## 十、总结

本设计方案针对**Polymarket 5分钟BTC涨跌预测**场景，在**高性能**和**开发可行性**之间取得平衡：

✅ **务实的技术选型**：Rust核心保证性能，Go/Next.js快速开发Web界面  
✅ **聚焦关键因子**：支持率、订单簿、K线、跨市场价差四大核心特征  
✅ **完整的回测支持**：从数据录制到策略验证的闭环  
✅ **生产级可靠性**：三层风控、市场轮动、断线重连  
✅ **低占用部署**：单服务器<200MB内存，月成本<$60  

**下一步行动建议**：
1. 先用1天时间编写一个**网络探针程序**，测量Polymarket API的真实延迟和订单簿深度
2. 如果平均延迟>200ms或订单簿价差>2%，则需要调整策略预期（放弃高频，转向低频统计套利）
3. 确认数据可行后，按8周路线图逐步实现MVP

这是一个**可落地、可验证、可扩展**的专业级交易系统设计。
