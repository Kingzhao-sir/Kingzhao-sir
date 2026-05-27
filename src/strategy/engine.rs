//! Strategy engine - orchestrates feature extraction and signal generation
//! 
//! Main entry point for strategy calculations

use crate::types::{Signal, Config};
use crate::strategy::signals::SignalGenerator;
use crate::data::order_book::OrderBookManager;
use crossbeam_channel::{bounded, Receiver, Sender};
use tracing::{info, debug};

/// Main strategy engine coordinating all alpha calculations
pub struct StrategyEngine {
    config: Config,
    signal_generator: SignalGenerator,
    order_book_manager: OrderBookManager,
    signal_tx: Sender<Signal>,
    signal_rx: Receiver<Signal>,
}

impl StrategyEngine {
    pub fn new(config: Config) -> Self {
        let (signal_tx, signal_rx) = bounded(1000);
        
        Self {
            config: config.clone(),
            signal_generator: SignalGenerator::new(config),
            order_book_manager: OrderBookManager::new(),
            signal_tx,
            signal_rx,
        }
    }
    
    /// Get receiver for signals (used by execution engine)
    pub fn get_signal_receiver(&self) -> Receiver<Signal> {
        self.signal_rx.clone()
    }
    
    /// Get order book manager for updates
    pub fn get_order_book_manager(&self) -> &OrderBookManager {
        &self.order_book_manager
    }
    
    /// Process market data and generate signals
    /// Called in hot path - must be zero-allocation
    pub fn process_tick(
        &self,
        market_id: String,
        support_rate: f64,
        binance_prices: &[f64],
        binance_volumes: &[f64],
        obi: f64,
        binance_price: f64,
        polymarket_price: f64,
        seconds_remaining: i64,
    ) -> Option<Signal> {
        // Generate signal
        if let Some(signal) = self.signal_generator.generate_signal(
            market_id,
            support_rate,
            binance_prices,
            binance_volumes,
            obi,
            binance_price,
            polymarket_price,
            seconds_remaining,
        ) {
            debug!("Signal generated: {:?} with confidence {:.2}", 
                   signal.direction, signal.confidence);
            
            // Send to execution engine via lock-free channel
            if self.signal_tx.send(signal.clone()).is_ok() {
                return Some(signal);
            }
        }
        
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RiskLimits;
    
    fn create_test_config() -> Config {
        Config {
            polymarket_api_key: "test".to_string(),
            polymarket_secret: "test".to_string(),
            binance_ws_url: "wss://test.com".to_string(),
            polymarket_ws_url: "wss://test.com".to_string(),
            risk_limits: RiskLimits::default(),
            strategy_weights: [0.3, 0.3, 0.25, 0.15],
            decision_window_seconds: 30,
            blackout_window_seconds: 5,
        }
    }
    
    #[test]
    fn test_strategy_engine_creation() {
        let config = create_test_config();
        let engine = StrategyEngine::new(config);
        assert!(true);
    }
    
    #[test]
    fn test_signal_generation_in_decision_window() {
        let config = create_test_config();
        let engine = StrategyEngine::new(config);
        
        let signal = engine.process_tick(
            "test-market".to_string(),
            0.7,
            &[100.0, 101.0, 102.0, 103.0],
            &[1.0, 1.0, 1.0, 1.0],
            0.3,
            103.0,
            101.0,
            20, // In decision window
        );
        
        assert!(signal.is_some());
    }
    
    #[test]
    fn test_no_signal_outside_window() {
        let config = create_test_config();
        let engine = StrategyEngine::new(config);
        
        let signal = engine.process_tick(
            "test-market".to_string(),
            0.7,
            &[100.0, 101.0, 102.0],
            &[1.0, 1.0, 1.0],
            0.3,
            102.0,
            101.0,
            60, // Outside decision window
        );
        
        assert!(signal.is_none());
    }
}
