//! Feature extraction for trading signals
//! 
//! Extracts features from:
//! - Polymarket support rate (implied probability)
//! - 1-minute and 5-minute K-line patterns
//! - Order book imbalance (OBI)
//! - Cross-exchange spread (Binance vs Polymarket)

use crate::types::{Decimal, SignalFactors, OrderBook};
use rust_decimal::prelude::*;

/// Extracts alpha factors for signal generation
pub struct FeatureExtractor {
    // Ring buffers for efficient sliding window calculations
    price_history_1m: Vec<f64>,
    price_history_5m: Vec<f64>,
    support_rate_history: Vec<f64>,
}

impl FeatureExtractor {
    pub fn new() -> Self {
        Self {
            // Pre-allocate ring buffers (no dynamic allocation in hot path)
            price_history_1m: Vec::with_capacity(300), // 5 minutes of 1-second data
            price_history_5m: Vec::with_capacity(300),
            support_rate_history: Vec::with_capacity(300),
        }
    }
    
    /// Calculate momentum factor from K-line data
    /// Positive = bullish momentum, Negative = bearish
    pub fn calculate_momentum(&self, prices: &[f64], lookback: usize) -> f64 {
        if prices.len() < lookback + 1 {
            return 0.0;
        }
        
        let current = prices[prices.len() - 1];
        let past = prices[prices.len() - 1 - lookback];
        
        if past == 0.0 {
            return 0.0;
        }
        
        // Normalize to [-1, 1] range
        let momentum = (current - past) / past;
        momentum.clamp(-1.0, 1.0)
    }
    
    /// Calculate VWAP deviation
    pub fn calculate_vwap_deviation(&self, prices: &[f64], volumes: &[f64]) -> f64 {
        if prices.is_empty() || volumes.is_empty() || prices.len() != volumes.len() {
            return 0.0;
        }
        
        let mut total_pv = 0.0;
        let mut total_volume = 0.0;
        
        for (price, volume) in prices.iter().zip(volumes.iter()) {
            total_pv += price * volume;
            total_volume += volume;
        }
        
        if total_volume == 0.0 {
            return 0.0;
        }
        
        let vwap = total_pv / total_volume;
        let current_price = prices[prices.len() - 1];
        
        // Normalized deviation
        (current_price - vwap) / vwap
    }
    
    /// Calculate order book imbalance factor
    /// Range: [-1, 1] where positive = bid pressure
    pub fn calculate_obi_factor(&self, obi: f64) -> f64 {
        // OBI is already in [-1, 1] range
        obi.clamp(-1.0, 1.0)
    }
    
    /// Calculate support rate factor from Polymarket implied probability
    /// Support rate > 0.5 suggests "Yes" (price up)
    pub fn calculate_support_rate_factor(&self, support_rate: f64) -> f64 {
        // Convert [0, 1] to [-1, 1]
        // 0.5 -> 0.0 (neutral)
        // 1.0 -> 1.0 (strong buy)
        // 0.0 -> -1.0 (strong sell)
        (support_rate - 0.5) * 2.0
    }
    
    /// Calculate cross-exchange spread factor
    /// Positive when Binance price > Polymarket (arbitrage opportunity)
    pub fn calculate_spread_factor(&self, binance_price: f64, polymarket_price: f64) -> f64 {
        if polymarket_price == 0.0 {
            return 0.0;
        }
        
        let spread = (binance_price - polymarket_price) / polymarket_price;
        spread.clamp(-1.0, 1.0)
    }
    
    /// Calculate time decay factor
    /// Increases urgency as cycle end approaches
    pub fn calculate_time_decay_factor(&self, seconds_remaining: i64, total_seconds: i64) -> f64 {
        if total_seconds <= 0 {
            return 0.0;
        }
        
        let ratio = seconds_remaining as f64 / total_seconds as f64;
        // Higher factor when less time remains
        (1.0 - ratio).clamp(0.0, 1.0)
    }
    
    /// Extract all features and return SignalFactors
    pub fn extract_features(
        &self,
        support_rate: f64,
        binance_prices: &[f64],
        binance_volumes: &[f64],
        obi: f64,
        binance_price: f64,
        polymarket_price: f64,
        seconds_remaining: i64,
    ) -> SignalFactors {
        SignalFactors {
            support_rate_factor: self.calculate_support_rate_factor(support_rate),
            momentum_factor: self.calculate_momentum(binance_prices, 5), // 5-period momentum
            obi_factor: self.calculate_obi_factor(obi),
            spread_factor: self.calculate_spread_factor(binance_price, polymarket_price),
            time_decay_factor: self.calculate_time_decay_factor(seconds_remaining, 300), // 5 min = 300s
        }
    }
}

impl Default for FeatureExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_feature_extractor_creation() {
        let extractor = FeatureExtractor::new();
        assert!(extractor.price_history_1m.capacity() >= 300);
    }
    
    #[test]
    fn test_momentum_calculation() {
        let extractor = FeatureExtractor::new();
        let prices = vec![100.0, 101.0, 102.0, 103.0, 104.0];
        
        let momentum = extractor.calculate_momentum(&prices, 1);
        assert!(momentum > 0.0); // Upward momentum
        
        let momentum_4 = extractor.calculate_momentum(&prices, 4);
        assert!(momentum_4 > momentum); // Longer lookback shows more momentum
    }
    
    #[test]
    fn test_support_rate_factor() {
        let extractor = FeatureExtractor::new();
        
        // Neutral support rate
        assert!((extractor.calculate_support_rate_factor(0.5) - 0.0).abs() < 0.001);
        
        // Strong buy signal
        assert!(extractor.calculate_support_rate_factor(1.0) > 0.9);
        
        // Strong sell signal
        assert!(extractor.calculate_support_rate_factor(0.0) < -0.9);
    }
    
    #[test]
    fn test_obi_factor() {
        let extractor = FeatureExtractor::new();
        
        // Balanced book
        assert!((extractor.calculate_obi_factor(0.0) - 0.0).abs() < 0.001);
        
        // Bid-heavy
        assert!(extractor.calculate_obi_factor(0.8) > 0.7);
        
        // Ask-heavy
        assert!(extractor.calculate_obi_factor(-0.8) < -0.7);
    }
    
    #[test]
    fn test_time_decay_factor() {
        let extractor = FeatureExtractor::new();
        
        // Start of cycle (300s remaining)
        let factor_start = extractor.calculate_time_decay_factor(300, 300);
        assert!((factor_start - 0.0).abs() < 0.001);
        
        // End of cycle (0s remaining)
        let factor_end = extractor.calculate_time_decay_factor(0, 300);
        assert!((factor_end - 1.0).abs() < 0.001);
        
        // Middle of cycle
        let factor_mid = extractor.calculate_time_decay_factor(150, 300);
        assert!((factor_mid - 0.5).abs() < 0.001);
    }
}
