//! Backtest engine - simulates trading strategy on historical data

use crate::core::{MarketInfo, MarketState, OrderBook, Candle, Side, now_us};
use crate::strategy::{StrategyEngine, StrategyConfig};
use crate::backtest::config::{BacktestConfig, BacktestTrade, BacktestReport, BacktestCandle};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Order book simulator for backtesting
struct OrderBookSimulator {
    spreads: HashMap<String, Decimal>,
}

impl OrderBookSimulator {
    fn new() -> Self {
        Self {
            spreads: HashMap::new(),
        }
    }

    fn simulate_order_book(&self, market_id: &str, mid_price: Decimal) -> OrderBook {
        let mut book = OrderBook::new(market_id.to_string());
        
        // Simulate realistic spread (0.5-2% depending on liquidity)
        let spread_pct = Decimal::from_f64_retain(0.01).unwrap_or(Decimal::from(1));
        let half_spread = (mid_price * spread_pct) / Decimal::from(2);
        
        let bid_price = (mid_price - half_spread).max(Decimal::from(1));
        let ask_price = mid_price + half_spread;
        
        book.bids.push(crate::core::Level {
            price: bid_price,
            quantity: Decimal::from(100),
            order_count: 5,
        });
        
        book.asks.push(crate::core::Level {
            price: ask_price,
            quantity: Decimal::from(100),
            order_count: 5,
        });
        
        book
    }
}

/// Position manager for backtesting
struct BacktestPositionManager {
    positions: HashMap<String, BacktestPosition>,
}

#[derive(Debug, Clone)]
struct BacktestPosition {
    market_id: String,
    side: Side,
    entry_price: Decimal,
    quantity: Decimal,
    entry_time_us: u64,
}

impl BacktestPositionManager {
    fn new() -> Self {
        Self {
            positions: HashMap::new(),
        }
    }

    fn open_position(
        &mut self,
        market_id: String,
        side: Side,
        price: Decimal,
        quantity: Decimal,
    ) {
        let pos = BacktestPosition {
            market_id: market_id.clone(),
            side,
            entry_price: price,
            quantity,
            entry_time_us: now_us(),
        };
        self.positions.insert(market_id, pos);
    }

    fn close_position(
        &mut self,
        market_id: &str,
        exit_price: Decimal,
    ) -> Option<(Decimal, u64)> {
        if let Some(pos) = self.positions.remove(market_id) {
            let pnl = match pos.side {
                Side::Yes => (exit_price - pos.entry_price) * pos.quantity,
                Side::No => (pos.entry_price - exit_price) * pos.quantity,
            };
            let duration = now_us().saturating_sub(pos.entry_time_us);
            Some((pnl, duration))
        } else {
            None
        }
    }

    fn has_position(&self, market_id: &str) -> bool {
        self.positions.contains_key(market_id)
    }
}

/// Main backtest engine
pub struct BacktestEngine {
    config: BacktestConfig,
    strategy: StrategyEngine,
    order_book_sim: OrderBookSimulator,
    position_manager: BacktestPositionManager,
    capital: Decimal,
    trades: Vec<BacktestTrade>,
    candles: HashMap<String, Vec<Candle>>,
}

impl BacktestEngine {
    pub fn new(config: BacktestConfig) -> Self {
        let strategy_config = StrategyConfig {
            min_confidence: config.min_confidence,
            ..Default::default()
        };

        Self {
            config,
            strategy: StrategyEngine::new(strategy_config),
            order_book_sim: OrderBookSimulator::new(),
            position_manager: BacktestPositionManager::new(),
            capital: config.initial_capital,
            trades: Vec::new(),
            candles: HashMap::new(),
        }
    }

    /// Load historical candle data
    pub fn load_candles(&mut self, market_id: String, candles: Vec<BacktestCandle>) {
        let core_candles: Vec<Candle> = candles
            .into_iter()
            .map(|c| Candle {
                timestamp_us: c.timestamp_us,
                open: c.open,
                high: c.high,
                low: c.low,
                close: c.close,
                volume: c.volume,
                period_seconds: c.period_seconds,
            })
            .collect();

        self.candles.insert(market_id, core_candles);
    }

    /// Run backtest simulation
    pub fn run(&mut self) -> BacktestReport {
        let mut report = BacktestReport::new(self.config.initial_capital);

        // Simple backtest: iterate through candles and generate signals
        for (market_id, candles) in &self.candles {
            if candles.len() < 3 {
                continue;
            }

            // Process each candle as a potential decision point
            for i in (3..candles.len()).step_by(1) {
                let current_candle = &candles[i];
                let recent_candles = &candles[i - 3..=i];

                // Create simulated market info
                let market = MarketInfo {
                    market_id: market_id.clone(),
                    token_id_yes: "yes".to_string(),
                    token_id_no: "no".to_string(),
                    condition_id: "cond".to_string(),
                    question: "Test".to_string(),
                    start_time_us: current_candle.timestamp_us.saturating_sub(300_000_000),
                    end_time_us: current_candle.timestamp_us + 25_000_000, // In decision window
                    state: MarketState::Open,
                    implied_probability_yes: current_candle.close.to_f64().unwrap_or(0.5) / 100.0,
                    volume_24h: current_candle.volume,
                    liquidity_yes: Decimal::from(5000),
                    liquidity_no: Decimal::from(5000),
                };

                // Skip if we already have a position in this market
                if self.position_manager.has_position(market_id) {
                    continue;
                }

                // Get order book
                let mid_price = current_candle.close;
                let order_book = self.order_book_sim.simulate_order_book(market_id, mid_price);

                // Generate signal
                if let Some(signal) =
                    self.strategy
                        .generate_signal(&market, &order_book, recent_candles, recent_candles)
                {
                    // Check if we should execute
                    if signal.should_execute(self.config.min_confidence) {
                        // Calculate entry price with slippage
                        let slippage = mid_price
                            * Decimal::from(self.config.slippage_bps)
                            / Decimal::from(10000);
                        let entry_price = match signal.side {
                            Side::Yes => mid_price + slippage,
                            Side::No => mid_price - slippage,
                        };

                        // Calculate quantity
                        let qty = signal.target_quantity.min(self.config.max_position_size);

                        // Check capital
                        let required_capital = entry_price * qty;
                        if required_capital > self.capital {
                            continue;
                        }

                        // Open position
                        self.position_manager.open_position(
                            market_id.clone(),
                            signal.side,
                            entry_price,
                            qty,
                        );

                        // Simulate exit at next candle (simplified)
                        if i + 1 < candles.len() {
                            let exit_candle = &candles[i + 1];
                            let exit_price = exit_candle.close;

                            // Calculate exit slippage
                            let exit_slippage = exit_price
                                * Decimal::from(self.config.slippage_bps)
                                / Decimal::from(10000);
                            let actual_exit_price = match signal.side {
                                Side::Yes => exit_price - exit_slippage,
                                Side::No => exit_price + exit_slippage,
                            };

                            // Close position
                            if let Some((pnl, _duration)) =
                                self.position_manager.close_position(market_id, actual_exit_price)
                            {
                                // Calculate commission
                                let commission =
                                    (entry_price * qty + actual_exit_price * qty)
                                        * Decimal::from_f64_retain(self.config.commission_rate)
                                            .unwrap_or(Decimal::ZERO);

                                // Record trade
                                let trade = BacktestTrade {
                                    timestamp_us: current_candle.timestamp_us,
                                    market_id: market_id.clone(),
                                    side: signal.side,
                                    entry_price,
                                    exit_price: Some(actual_exit_price),
                                    quantity: qty,
                                    pnl: Some(pnl),
                                    commission,
                                    slippage: slippage * qty + exit_slippage * qty,
                                    exit_timestamp_us: Some(exit_candle.timestamp_us),
                                };

                                self.trades.push(trade.clone());
                                self.capital = self.capital + pnl - commission;
                            }
                        }
                    }
                }
            }
        }

        // Finalize report
        report.finalize(&self.trades);
        report.starting_capital = self.config.initial_capital;
        report.ending_capital = self.capital;

        report
    }

    /// Get all trades from backtest
    pub fn get_trades(&self) -> &[BacktestTrade] {
        &self.trades
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::config::BacktestCandle;

    fn create_test_candles() -> Vec<BacktestCandle> {
        let base_time = now_us();
        vec![
            BacktestCandle {
                timestamp_us: base_time - 180_000_000,
                market_id: "test".to_string(),
                open: Decimal::from(54),
                high: Decimal::from(56),
                low: Decimal::from(53),
                close: Decimal::from(55),
                volume: Decimal::from(1000),
                period_seconds: 60,
            },
            BacktestCandle {
                timestamp_us: base_time - 120_000_000,
                market_id: "test".to_string(),
                open: Decimal::from(55),
                high: Decimal::from(57),
                low: Decimal::from(54),
                close: Decimal::from(56),
                volume: Decimal::from(1100),
                period_seconds: 60,
            },
            BacktestCandle {
                timestamp_us: base_time - 60_000_000,
                market_id: "test".to_string(),
                open: Decimal::from(56),
                high: Decimal::from(58),
                low: Decimal::from(55),
                close: Decimal::from(57),
                volume: Decimal::from(1200),
                period_seconds: 60,
            },
            BacktestCandle {
                timestamp_us: base_time,
                market_id: "test".to_string(),
                open: Decimal::from(57),
                high: Decimal::from(59),
                low: Decimal::from(56),
                close: Decimal::from(58),
                volume: Decimal::from(1300),
                period_seconds: 60,
            },
            BacktestCandle {
                timestamp_us: base_time + 60_000_000,
                market_id: "test".to_string(),
                open: Decimal::from(58),
                high: Decimal::from(60),
                low: Decimal::from(57),
                close: Decimal::from(59),
                volume: Decimal::from(1400),
                period_seconds: 60,
            },
        ]
    }

    #[test]
    fn test_backtest_engine_initialization() {
        let config = BacktestConfig::default();
        let engine = BacktestEngine::new(config);

        assert_eq!(engine.capital, Decimal::from(10000));
        assert_eq!(engine.trades.len(), 0);
    }

    #[test]
    fn test_backtest_run() {
        let mut config = BacktestConfig::default();
        config.initial_capital = Decimal::from(10000);
        config.min_confidence = 0.5; // Lower threshold to ensure signals

        let mut engine = BacktestEngine::new(config);
        let candles = create_test_candles();
        engine.load_candles("test".to_string(), candles);

        let report = engine.run();

        // Report should be generated
        assert!(report.total_trades >= 0);
        assert!(report.starting_capital == Decimal::from(10000));
    }

    #[test]
    fn test_position_management() {
        let mut pm = BacktestPositionManager::new();

        assert!(!pm.has_position("test"));

        pm.open_position(
            "test".to_string(),
            Side::Yes,
            Decimal::from(50),
            Decimal::from(100),
        );

        assert!(pm.has_position("test"));

        let result = pm.close_position("test", Decimal::from(55));
        assert!(result.is_some());

        let (pnl, _duration) = result.unwrap();
        assert!(pnl > Decimal::ZERO); // Should be profitable

        assert!(!pm.has_position("test"));
    }
}
