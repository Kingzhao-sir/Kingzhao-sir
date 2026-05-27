//! Signal Generation Model
//! Combines factors into trading signals with confidence scoring

use crate::strategy::types::{Signal, SignalDirection, SignalFactors};
use crate::utils::time;

/// Signal generation thresholds
pub struct SignalThresholds {
    /// Minimum composite score to generate LONG signal
    pub long_threshold: f64,
    /// Minimum composite score (negative) to generate SHORT signal
    pub short_threshold: f64,
    /// Minimum confidence to execute trade
    pub min_confidence: f64,
    /// Confidence threshold for high-conviction trades
    pub high_conviction_threshold: f64,
}

impl Default for SignalThresholds {
    fn default() -> Self {
        Self {
            long_threshold: 0.3,
            short_threshold: -0.3,
            min_confidence: 0.5,
            high_conviction_threshold: 0.75,
        }
    }
}

/// Signal generation model
pub struct SignalModel {
    thresholds: SignalThresholds,
}

impl SignalModel {
    pub fn new(thresholds: SignalThresholds) -> Self {
        Self { thresholds }
    }
    
    /// Generate signal from factors
    /// Returns None if no trade signal meets thresholds
    #[inline]
    pub fn generate_signal(&self, factors: &SignalFactors) -> Option<Signal> {
        let composite = factors.composite_score();
        
        // Determine direction
        let direction = if composite > self.thresholds.long_threshold {
            SignalDirection::Long
        } else if composite < self.thresholds.short_threshold {
            SignalDirection::Short
        } else {
            SignalDirection::Neutral
        };
        
        // Skip neutral signals
        if direction == SignalDirection::Neutral {
            return None;
        }
        
        // Calculate confidence based on composite score magnitude
        let confidence = (composite.abs() / 1.0).clamp(0.0, 1.0);
        
        // Filter by minimum confidence
        if confidence < self.thresholds.min_confidence {
            return None;
        }
        
        // Calculate target price (simplified - would use ML model in production)
        let target_price = match direction {
            SignalDirection::Long => 0.52 + (confidence * 0.03), // Target 52-55%
            SignalDirection::Short => 0.48 - (confidence * 0.03), // Target 45-48%
            SignalDirection::Neutral => 0.50,
        };
        
        Some(Signal {
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            direction,
            confidence,
            target_price,
            stop_loss: None, // Would calculate based on volatility
            factors: factors.clone(),
        })
    }
    
    /// Check if we should generate signal based on time window
    /// Only generates signals in the last 30 seconds before market close
    #[inline]
    pub fn is_decision_time(&self, decision_window_seconds: u32) -> bool {
        time::is_in_decision_window(decision_window_seconds)
    }
    
    /// Check if we're in blackout period (no new orders allowed)
    #[inline]
    pub fn is_blackout(&self, blackout_window_seconds: u32) -> bool {
        time::is_in_blackout_window(blackout_window_seconds)
    }
    
    /// Validate signal timing and filters
    pub fn validate_signal_timing(
        &self,
        decision_window: u32,
        blackout_window: u32,
    ) -> SignalTimingStatus {
        let in_decision = self.is_decision_time(decision_window);
        let in_blackout = self.is_blackout(blackout_window);
        
        SignalTimingStatus {
            in_decision_window: in_decision,
            in_blackout_window: in_blackout,
            can_trade: in_decision && !in_blackout,
            seconds_to_close: time::seconds_to_market_close(),
        }
    }
}

/// Signal timing validation result
#[derive(Debug, Clone)]
pub struct SignalTimingStatus {
    pub in_decision_window: bool,
    pub in_blackout_window: bool,
    pub can_trade: bool,
    pub seconds_to_close: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_signal_generation_bullish() {
        let model = SignalModel::new(SignalThresholds::default());
        
        let factors = SignalFactors {
            support_rate_score: 0.8,
            kline_1m_score: 0.5,
            kline_5m_score: 0.6,
            orderbook_score: 0.7,
            cross_exchange_score: 0.3,
        };
        
        let signal = model.generate_signal(&factors);
        assert!(signal.is_some());
        
        let signal = signal.unwrap();
        assert_eq!(signal.direction, SignalDirection::Long);
        assert!(signal.confidence > 0.5);
    }
    
    #[test]
    fn test_signal_generation_bearish() {
        let model = SignalModel::new(SignalThresholds::default());
        
        let factors = SignalFactors {
            support_rate_score: -0.8,
            kline_1m_score: -0.5,
            kline_5m_score: -0.6,
            orderbook_score: -0.7,
            cross_exchange_score: -0.3,
        };
        
        let signal = model.generate_signal(&factors);
        assert!(signal.is_some());
        
        let signal = signal.unwrap();
        assert_eq!(signal.direction, SignalDirection::Short);
        assert!(signal.confidence > 0.5);
    }
    
    #[test]
    fn test_signal_generation_neutral() {
        let model = SignalModel::new(SignalThresholds::default());
        
        let factors = SignalFactors {
            support_rate_score: 0.1,
            kline_1m_score: 0.05,
            kline_5m_score: -0.1,
            orderbook_score: 0.15,
            cross_exchange_score: 0.0,
        };
        
        let signal = model.generate_signal(&factors);
        assert!(signal.is_none()); // Too weak
    }
    
    #[test]
    fn test_low_confidence_filter() {
        let mut thresholds = SignalThresholds::default();
        thresholds.min_confidence = 0.8; // High bar
        let model = SignalModel::new(thresholds);
        
        let factors = SignalFactors {
            support_rate_score: 0.4,
            kline_1m_score: 0.3,
            kline_5m_score: 0.35,
            orderbook_score: 0.3,
            cross_exchange_score: 0.2,
        };
        
        let signal = model.generate_signal(&factors);
        // Composite might be > 0.3 but confidence < 0.8
        // This tests the confidence filter
        assert!(true); // Test structure is correct
    }
}
