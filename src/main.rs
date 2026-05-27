//! Polymarket HFT Trading Engine - Main Entry Point
//! 
//! This is the main binary that runs the trading engine.

use poly_hft::core::{MarketState, OrderBook, Candle, Decimal};
use poly_hft::data::{MarketRouter, MarketRouterConfig};
use poly_hft::strategy::{StrategyEngine, StrategyConfig};
use poly_hft::execution::{OrderManagementSystem, OmsConfig};
use tracing::{info, warn, error, Level};
use tracing_subscriber::FmtSubscriber;

fn init_logging() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    init_logging();
    
    info!("Starting Polymarket HFT Trading Engine...");
    
    // Initialize components
    let router_config = MarketRouterConfig::default();
    let mut router = MarketRouter::new(router_config);
    
    let strategy_config = StrategyConfig::default();
    let strategy_engine = StrategyEngine::new(strategy_config);
    
    let oms_config = OmsConfig::default(); // Paper trading enabled by default
    let mut oms = OrderManagementSystem::new(oms_config);
    
    info!("Components initialized:");
    info!("  - Market Router: Ready");
    info!("  - Strategy Engine: Ready (min_confidence={})", strategy_config.min_confidence);
    info!("  - OMS: Ready (paper_trading={})", oms.is_paper_trading());
    
    // Register state change callback
    router.register_state_callback(Box::new(|market| {
        info!("Market state changed: {} - {:?}", market.market_id, market.state);
    }));
    
    // Set order update callback
    oms.set_order_update_callback(|report| {
        info!(
            "Order update: {:?} {} {} @ {} (qty: {})",
            report.order_id,
            if report.is_maker { "MAKER" } else { "TAKER" },
            match report.side {
                poly_hft::core::Side::Yes => "YES",
                poly_hft::core::Side::No => "NO",
            },
            report.price,
            report.quantity
        );
    });
    
    info!("\n=== System Ready ===");
    info!("Mode: PAPER TRADING (Simulation)");
    info!("The system is ready to trade but will not submit real orders.");
    info!("To enable real trading, configure API keys in OmsConfig.");
    info!("===================\n");
    
    // In a real implementation, this would:
    // 1. Connect to Polymarket WebSocket for market data
    // 2. Connect to Binance/OKX for cross-exchange prices
    // 3. Start the main event loop
    // 4. Monitor market state and execute trades
    
    // For now, we'll just demonstrate the system structure
    demo_system(&mut router, &strategy_engine, &mut oms);
    
    Ok(())
}

/// Demonstrate system capabilities
fn demo_system(
    router: &mut MarketRouter,
    strategy: &StrategyEngine,
    oms: &mut OrderManagementSystem,
) {
    use poly_hft::core::{MarketInfo, now_us};
    
    info!("Running system demonstration...\n");
    
    // Create a simulated market in decision window
    let now = now_us();
    let market = MarketInfo {
        market_id: "demo-btc-5m".to_string(),
        token_id_yes: "yes-token".to_string(),
        token_id_no: "no-token".to_string(),
        condition_id: "cond-123".to_string(),
        question: "Will BTC be up in 5 minutes?".to_string(),
        start_time_us: now - 270_000_000,  // Started 4.5 min ago
        end_time_us: now + 25_000_000,     // 25 seconds remaining (decision window)
        state: MarketState::Open,
        implied_probability_yes: 0.58,     // 58% support rate
        volume_24h: Decimal::from(10000),
        liquidity_yes: Decimal::from(5000),
        liquidity_no: Decimal::from(5000),
    };
    
    info!("1. Market Information:");
    info!("   Market ID: {}", market.market_id);
    info!("   Question: {}", market.question);
    info!("   Support Rate (Implied Prob): {:.1}%", market.implied_probability_yes * 100.0);
    info!("   Time Remaining: {:.1}s", market.remaining_seconds());
    info!("   In Decision Window: {}", market.is_in_decision_window());
    info!("   In Cooling Period: {}", market.is_in_cooling_period());
    
    // Update router with market
    router.update_current_market(market.clone());
    
    info!("\n2. Operational State:");
    info!("   Current State: {:?}", router.get_operational_state());
    info!("   Can Open Positions: {}", router.can_open_positions());
    
    // Create simulated order book
    let mut order_book = OrderBook::new(market.market_id.clone());
    order_book.bids.push(poly_hft::core::Level {
        price: Decimal::from(57),
        quantity: Decimal::from(100),
        order_count: 5,
    });
    order_book.bids.push(poly_hft::core::Level {
        price: Decimal::from(56),
        quantity: Decimal::from(150),
        order_count: 8,
    });
    order_book.asks.push(poly_hft::core::Level {
        price: Decimal::from(59),
        quantity: Decimal::from(80),
        order_count: 4,
    });
    order_book.asks.push(poly_hft::core::Level {
        price: Decimal::from(60),
        quantity: Decimal::from(120),
        order_count: 6,
    });
    
    info!("\n3. Order Book Analysis:");
    info!("   Best Bid: {:?}", order_book.best_bid());
    info!("   Best Ask: {:?}", order_book.best_ask());
    info!("   Mid Price: {:?}", order_book.mid_price());
    info!("   Spread: {:?} bps", order_book.spread_bps());
    info!("   OBI (5 levels): {:.3}", order_book.calculate_obi(5));
    
    // Create simulated candles
    let candles = vec![
        Candle {
            timestamp_us: now - 120_000_000,
            open: Decimal::from(55),
            high: Decimal::from(57),
            low: Decimal::from(54),
            close: Decimal::from(56),
            volume: Decimal::from(1000),
            period_seconds: 60,
        },
        Candle {
            timestamp_us: now - 60_000_000,
            open: Decimal::from(56),
            high: Decimal::from(58),
            low: Decimal::from(55),
            close: Decimal::from(57),
            volume: Decimal::from(1200),
            period_seconds: 60,
        },
        Candle {
            timestamp_us: now,
            open: Decimal::from(57),
            high: Decimal::from(59),
            low: Decimal::from(56),
            close: Decimal::from(58),
            volume: Decimal::from(1100),
            period_seconds: 60,
        },
    ];
    
    info!("\n4. Strategy Signal Generation:");
    match strategy.generate_signal(&market, &order_book, &candles, &candles) {
        Some(signal) => {
            info!("   Signal Generated!");
            info!("   Side: {:?}", signal.side);
            info!("   Confidence: {:.1}%", signal.confidence * 100.0);
            info!("   Target Quantity: {}", signal.target_quantity);
            info!("   Fair Value: {}", signal.fair_value);
            info!("   Factors:");
            info!("     - Support Rate: {:.3}", signal.factors.support_rate_factor);
            info!("     - OBI: {:.3}", signal.factors.obi_factor);
            info!("     - Momentum: {:.3}", signal.factors.momentum_factor);
            info!("     - Cross-Exchange: {:.3}", signal.factors.cross_exchange_factor);
            info!("     - K-Line Pattern: {:.3}", signal.factors.kline_pattern_factor);
            
            // Execute signal (in paper trading mode)
            if signal.should_execute(0.65) && router.can_open_positions() {
                info!("\n5. Order Execution:");
                let order = poly_hft::core::Order::new_limit(
                    signal.market_id.clone(),
                    signal.side,
                    signal.fair_value,
                    signal.target_quantity,
                    true, // Post-only
                );
                
                let mid_price = order_book.mid_price().unwrap_or(Decimal::from(50));
                match oms.submit_order(order, mid_price) {
                    Ok(order_id) => {
                        info!("   Order Submitted: {:?}", order_id);
                        info!("   Status: FILLED (Paper Trading)");
                        
                        if let Some(pos) = oms.get_position(&market.market_id) {
                            info!("\n6. Position Update:");
                            info!("   YES Shares: {}", pos.yes_shares);
                            info!("   NO Shares: {}", pos.no_shares);
                            info!("   Avg Entry (YES): {}", pos.avg_entry_yes);
                        }
                    }
                    Err(e) => {
                        warn!("   Order Failed: {}", e);
                    }
                }
            }
        }
        None => {
            info!("   No signal generated (confidence below threshold or outside decision window)");
        }
    }
    
    info!("\n=== Demonstration Complete ===");
}
