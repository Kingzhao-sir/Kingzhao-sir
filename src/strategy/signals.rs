//! Signal generation engine
//! 
//! Combines multiple factors to generate trading signals:
//! - Only generates signals in decision window (last 30 seconds)
//! - No signals in blackout period (last 5 seconds)
//! - Confidence-weighted scoring system

use crate::types::{Signal, SignalDirection, SignalFactors, Config};
use crate::strategy::features::FeatureExtractor;
use chrono::{DateTime, Utc, Duration};
use tracing::{info, debug};

/// Generates trading signals from extracted features
pub struct SignalGenerator {
    config: Config,
    feature_extractor: FeatureExtractor,
}

impl SignalGenerator {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            feature_extractor: FeatureExtractor::new(),
        }
    }
    
    /// Generate signal based on current market conditions
    /// Returns None if outside decision window or confidence too low
    pub fn generate_signal(
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
        // Check if we're in decision window
        if seconds_remaining > self.config.decision_window_seconds as i64 {
            debug!("Outside decision window ({}s remaining)", seconds_remaining);
            return None;
        }
        
        // Check if we're in blackout period
        if seconds_remaining <= self.config.blackout_window_seconds as i64 {
            debug!("In blackout period ({}s remaining)", seconds_remaining);
            return None;
        }
        
        // Extract features
        let factors = self.feature_extractor.extract_features(
            support_rate,
            binance_prices,
            binance_volumes,
            obi,
            binance_price,
            polymarket_price,
            seconds_remaining,
        );
        
        // Calculate weighted score
        let score = factors.weighted_score(&self.config.strategy_weights);
        
        // Convert score to direction and confidence
        let (direction, confidence) = self.score_to_direction(score);
        
        // Filter low confidence signals
        if confidence < 0.3 {
            debug!("Signal confidence too low: {:.2}", confidence);
            return None;
        }
        
        let now = Utc::now();
        let expiry = now + Duration::seconds(seconds_remaining);
        
        Some(Signal {
            timestamp: now,
            market_id,
            direction,
            confidence,
            factors,
            expiry,
        })
    }
    
    /// Convert weighted score to direction and confidence
    fn score_to_direction(&self, score: f64) -> (SignalDirection, f64) {
        let abs_score = score.abs();
        let confidence = abs_score.min(1.0); // Clamp to [0, 1]
        
        let direction = if score > 0.1 {
            SignalDirection::Long
        } else if score < -0.1 {
            SignalDirection::Short
        } else {
            SignalDirection::Neutral
        };
        
        (direction, confidence)
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
    fn test_signal_generator_creation() {
        let config = create_test_config();
        let generator = SignalGenerator::new(config);
        assert!(true);
    }
    
    #[test]
    fn test_outside_decision_window() {
        let config = create_test_config();
        let generator = SignalGenerator::new(config);
        
        // 60 seconds remaining - outside decision window
        let signal = generator.generate_signal(
            "test-market".to_string(),
            0.6,
            &[100.0, 101.0, 102.0],
            &[1.0, 1.0, 1.0],
            0.1,
            102.0,
            101.0,
            60,
        );
        
        assert!(signal.is_none());
    }
    
    #[test]
    fn test_in_blackout_period() {
        let config = create_test_config();
        let generator = SignalGenerator::new(config);
        
        // 3 seconds remaining - blackout period
        let signal = generator.generate_signal(
            "test-market".to_string(),
            0.8,
            &[100.0, 101.0, 102.0],
            &[1.0, 1.0, 1.0],
            0.5,
            102.0,
            101.0,
            3,
        );
        
        assert!(signal.is_none());
    }
    
    #[test]
    fn test_strong_buy_signal() {
        let config = create_test_config();
        let generator = SignalGenerator::new(config);
        
        // 20 seconds remaining - in decision window
        // Strong bullish factors
        let signal = generator.generate_signal(
            "test-market".to_string(),
            0.8, // High support rate
            &[100.0, 101.0, 102.0, 103.0, 104.0], // Upward momentum
            &[1.0, 1.0, 1.0, 1.0, 1.0],
            0.6, // Bid-heavy order book
            104.0,
            102.0, // Binance > Polymarket
            20,
        );
        
        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.direction, SignalDirection::Long);
        assert!(signal.confidence > 0.3);
    }
    
    #[test]
    fn test_strong_sell_signal() {
        let config = create_test_config();
        let generator = SignalGenerator::new(config);
        
        // 15 seconds remaining - in decision window
        // Strong bearish factors
        let signal = generator.generate_signal(
            "test-market".to_string(),
            0.2, // Low support rate
            &[104.0, 103.0, 102.0, 101.0, 100.0], // Downward momentum
            &[1.0, 1.0, 1.0, 1.0, 1.0],
            -0.6, // Ask-heavy order book
            100.0,
            102.0, // Binance < Polymarket
            15,
        );
        
        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.direction, SignalDirection::Short);
        assert!(signal.confidence > 0.3);
    }
    
    #[test]
    fn test_low_confidence_filter() {
        let config = create_test_config();
        let generator = SignalGenerator::new(config);
        
        // Mixed signals should result in low confidence
        let signal = generator.generate_signal(
            "test-market".to_string(),
            0.5, // Neutral support rate
            &[100.0, 100.0, 100.0], // No momentum
            &[1.0, 1.0, 1.0],
            0.0, // Balanced order book
            100.0,
            100.0, // No spread
            20,
        );
        
        assert!(signal.is_none()); // Filtered due to low confidence
    }
}
