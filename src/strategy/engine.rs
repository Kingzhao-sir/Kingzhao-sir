//! Signal Engine - Orchestrates data ingestion, feature extraction, and signal generation
//! Runs in dedicated thread with CPU affinity for low-latency processing

use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn, error, debug};

use crate::strategy::types::{Signal, SignalFactors};
use crate::strategy::features::FeatureExtractor;
use crate::strategy::model::{SignalModel, SignalThresholds};
use crate::data::types::{OrderBookSnapshot, Kline1m, Kline5m};
use crate::utils::config::SystemConfig;
use crate::market::router::MarketRouter;

pub struct SignalEngine {
    signal_sender: broadcast::Sender<Signal>,
    config: SystemConfig,
    feature_extractor: FeatureExtractor,
    signal_model: SignalModel,
}

impl SignalEngine {
    pub fn new(signal_sender: broadcast::Sender<Signal>, config: SystemConfig) -> Self {
        let thresholds = SignalThresholds::default();
        
        Self {
            signal_sender,
            config: config.clone(),
            feature_extractor: FeatureExtractor::new(),
            signal_model: SignalModel::new(thresholds),
        }
    }
    
    /// Main entry point - runs the signal generation loop
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("🧠 Signal Engine starting...");
        info!("   Decision window: {}s before close", self.config.decision_window_seconds);
        info!("   Blackout window: {}s before close", self.config.blackout_window_seconds);
        
        // Simulation loop (would connect to real data streams in production)
        self.run_simulation().await?;
        
        Ok(())
    }
    
    /// Simulate signal generation (replace with real data in production)
    async fn run_simulation(&self) -> Result<(), Box<dyn std::error::Error>> {
        use tokio::time::{sleep, Duration};
        
        let mut cycle_count = 0;
        
        // Simulate multiple 5-minute cycles
        for cycle in 0..3 {
            info!("🔄 Simulating cycle {}", cycle + 1);
            
            // Simulate waiting until decision window (last 30 seconds)
            // In production, this would be real-time monitoring
            sleep(Duration::from_millis(100)).await;
            
            // Generate simulated inputs
            let implied_prob = 0.52 + (cycle as f64 * 0.02); // Varying support rate
            let klines_1m = self.generate_simulated_klines_1m();
            let klines_5m = self.generate_simulated_klines_5m();
            let orderbook = self.generate_simulated_orderbook(implied_prob);
            let binance_price = 95000.0 + (cycle as f64 * 100.0);
            
            // Extract features
            let factors = self.feature_extractor.extract_all_factors(
                implied_prob,
                &klines_1m,
                &klines_5m,
                &orderbook,
                binance_price,
                95000.0,
            );
            
            debug!("📊 Factors: composite={:.3}", factors.composite_score());
            
            // Generate signal
            if let Some(signal) = self.signal_model.generate_signal(&factors) {
                info!(
                    "📡 Signal generated: {:?} (confidence: {:.2}, target: {:.3})",
                    signal.direction, signal.confidence, signal.target_price
                );
                
                // Send signal to OMS
                let _ = self.signal_sender.send(signal);
                cycle_count += 1;
            } else {
                debug!("⏸️  No signal (below threshold)");
            }
        }
        
        info!("🧠 Signal Engine simulation complete. Generated {} signals.", cycle_count);
        Ok(())
    }
    
    fn generate_simulated_klines_1m(&self) -> Vec<Kline1m> {
        vec![
            Kline1m {
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                open: 95000.0,
                high: 95200.0,
                low: 94900.0,
                close: 95100.0,
                volume: 50.0,
            },
        ]
    }
    
    fn generate_simulated_klines_5m(&self) -> Vec<Kline5m> {
        vec![
            Kline5m {
                timestamp_ms: chrono::Utc::now().timestamp_millis() - 300_000,
                open: 94500.0,
                high: 95000.0,
                low: 94300.0,
                close: 94800.0,
                volume: 200.0,
            },
            Kline5m {
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                open: 94800.0,
                high: 95300.0,
                low: 94700.0,
                close: 95100.0,
                volume: 250.0,
            },
        ]
    }
    
    fn generate_simulated_orderbook(&self, mid: f64) -> OrderBookSnapshot {
        use crate::data::types::Level;
        
        let spread = 0.002;
        OrderBookSnapshot::new(
            chrono::Utc::now().timestamp_millis(),
            vec![
                Level { price: mid - spread / 2.0, size: 1000.0, order_count: 10 },
                Level { price: mid - spread, size: 2000.0, order_count: 15 },
            ],
            vec![
                Level { price: mid + spread / 2.0, size: 1200.0, order_count: 12 },
                Level { price: mid + spread, size: 1800.0, order_count: 8 },
            ],
        )
    }
    
    /// Get current timing status
    pub fn get_timing_status(&self) -> crate::strategy::model::SignalTimingStatus {
        self.signal_model.validate_signal_timing(
            self.config.decision_window_seconds,
            self.config.blackout_window_seconds,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;
    
    #[tokio::test]
    async fn test_signal_engine_creation() {
        let (tx, _rx) = broadcast::channel::<Signal>(100);
        let config = SystemConfig::load().unwrap();
        
        let engine = SignalEngine::new(tx, config);
        
        // Verify engine can be created
        let status = engine.get_timing_status();
        assert!(status.seconds_to_close < 300);
    }
    
    #[tokio::test]
    async fn test_signal_generation_flow() {
        let (tx, mut rx) = broadcast::channel::<Signal>(100);
        let config = SystemConfig::load().unwrap();
        
        let engine = SignalEngine::new(tx, config);
        
        // Manually test feature extraction and signal generation
        let factors = SignalFactors {
            support_rate_score: 0.7,
            kline_1m_score: 0.5,
            kline_5m_score: 0.6,
            orderbook_score: 0.4,
            cross_exchange_score: 0.3,
        };
        
        if let Some(signal) = engine.signal_model.generate_signal(&factors) {
            assert_eq!(signal.direction, crate::strategy::types::SignalDirection::Long);
            assert!(signal.confidence > 0.5);
        }
    }
}
