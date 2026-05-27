//! Feature Extraction Engine
//! Extracts predictive features from market data

use crate::data::types::{OrderBookSnapshot, Kline1m, Kline5m};
use crate::strategy::types::SignalFactors;

/// Feature extractor for strategy signals
pub struct FeatureExtractor {
    /// Threshold for strong order book imbalance
    obi_threshold: f64,
    /// Threshold for significant support rate deviation
    support_rate_threshold: f64,
}

impl FeatureExtractor {
    pub fn new() -> Self {
        Self {
            obi_threshold: 0.3,      // 30% imbalance
            support_rate_threshold: 0.05, // 5% deviation from 50%
        }
    }
    
    /// Calculate support rate score from Polymarket implied probability
    /// Score range: -1.0 (strong short) to +1.0 (strong long)
    #[inline]
    pub fn calculate_support_rate_score(&self, implied_prob: f64) -> f64 {
        // If implied prob > 0.5, bullish; < 0.5, bearish
        let deviation = implied_prob - 0.5;
        
        // Normalize to [-1, 1] range
        (deviation / self.support_rate_threshold).clamp(-1.0, 1.0)
    }
    
    /// Calculate 1-minute K-line pattern score
    /// Analyzes recent momentum and candle patterns
    #[inline]
    pub fn calculate_kline_1m_score(&self, klines: &[Kline1m]) -> f64 {
        if klines.is_empty() {
            return 0.0;
        }
        
        // Simple momentum: compare current close to open
        let latest = klines.last().unwrap();
        let price_change = (latest.close - latest.open) / latest.open;
        
        // Normalize and clamp
        (price_change * 100.0).clamp(-1.0, 1.0)
    }
    
    /// Calculate 5-minute K-line trend score
    /// Analyzes longer-term trend direction
    #[inline]
    pub fn calculate_kline_5m_score(&self, klines: &[Kline5m]) -> f64 {
        if klines.len() < 2 {
            return 0.0;
        }
        
        // Compare current 5m candle to previous
        let current = klines.last().unwrap();
        let previous = &klines[klines.len() - 2];
        
        let trend = if current.close > previous.close {
            1.0
        } else if current.close < previous.close {
            -1.0
        } else {
            0.0
        };
        
        // Weight by volume
        let volume_ratio = if previous.volume > 0.0 {
            current.volume / previous.volume
        } else {
            1.0
        };
        
        (trend * volume_ratio.clamp(0.5, 2.0)).clamp(-1.0, 1.0)
    }
    
    /// Calculate order book imbalance score
    /// Positive = buy pressure, Negative = sell pressure
    #[inline]
    pub fn calculate_orderbook_score(&self, ob: &OrderBookSnapshot) -> f64 {
        let obi = ob.calculate_obi();
        (obi / self.obi_threshold).clamp(-1.0, 1.0)
    }
    
    /// Calculate cross-exchange divergence score
    /// Compares Binance price to Polymarket implied price
    #[inline]
    pub fn calculate_cross_exchange_score(
        &self,
        binance_price: f64,
        polymarket_implied: f64,
        fair_value: f64,
    ) -> f64 {
        // Convert Polymarket probability to implied BTC price
        let poly_implied_price = fair_value * polymarket_implied / 0.5;
        
        // Calculate divergence
        let divergence = (binance_price - poly_implied_price) / binance_price;
        
        // If Binance > Polymarket, expect Polymarket to catch up (bullish)
        (divergence * 100.0).clamp(-1.0, 1.0)
    }
    
    /// Generate complete signal factors from all inputs
    pub fn extract_all_factors(
        &self,
        implied_prob: f64,
        klines_1m: &[Kline1m],
        klines_5m: &[Kline5m],
        orderbook: &OrderBookSnapshot,
        binance_price: f64,
        fair_value: f64,
    ) -> SignalFactors {
        SignalFactors {
            support_rate_score: self.calculate_support_rate_score(implied_prob),
            kline_1m_score: self.calculate_kline_1m_score(klines_1m),
            kline_5m_score: self.calculate_kline_5m_score(klines_5m),
            orderbook_score: self.calculate_orderbook_score(orderbook),
            cross_exchange_score: self.calculate_cross_exchange_score(
                binance_price,
                implied_prob,
                fair_value,
            ),
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
    use crate::data::types::Level;
    
    #[test]
    fn test_support_rate_score() {
        let extractor = FeatureExtractor::new();
        
        // Strong bullish (> 55% support)
        let score = extractor.calculate_support_rate_score(0.60);
        assert!(score > 0.5);
        
        // Strong bearish (< 45% support)
        let score = extractor.calculate_support_rate_score(0.40);
        assert!(score < -0.5);
        
        // Neutral (~50%)
        let score = extractor.calculate_support_rate_score(0.50);
        assert!(score.abs() < 0.1);
    }
    
    #[test]
    fn test_orderbook_score() {
        let extractor = FeatureExtractor::new();
        
        // Buy-heavy order book
        let ob_buy = OrderBookSnapshot::new(
            1000,
            vec![Level { price: 0.51, size: 300.0, order_count: 10 }],
            vec![Level { price: 0.53, size: 100.0, order_count: 5 }],
        );
        let score = extractor.calculate_orderbook_score(&ob_buy);
        assert!(score > 0.5);
        
        // Sell-heavy order book
        let ob_sell = OrderBookSnapshot::new(
            2000,
            vec![Level { price: 0.49, size: 100.0, order_count: 5 }],
            vec![Level { price: 0.51, size: 300.0, order_count: 10 }],
        );
        let score = extractor.calculate_orderbook_score(&ob_sell);
        assert!(score < -0.5);
    }
    
    #[test]
    fn test_composite_factors() {
        let extractor = FeatureExtractor::new();
        
        let klines_1m = vec![Kline1m {
            timestamp_ms: 1000,
            open: 95000.0,
            high: 95500.0,
            low: 94800.0,
            close: 95300.0,
            volume: 100.0,
        }];
        
        let klines_5m = vec![
            Kline5m {
                timestamp_ms: 1000,
                open: 94500.0,
                high: 95500.0,
                low: 94000.0,
                close: 95000.0,
                volume: 500.0,
            },
            Kline5m {
                timestamp_ms: 2000,
                open: 95000.0,
                high: 96000.0,
                low: 94800.0,
                close: 95800.0,
                volume: 600.0,
            },
        ];
        
        let ob = OrderBookSnapshot::new(
            1000,
            vec![Level { price: 0.51, size: 200.0, order_count: 8 }],
            vec![Level { price: 0.53, size: 150.0, order_count: 6 }],
        );
        
        let factors = extractor.extract_all_factors(
            0.55,  // 55% support rate
            &klines_1m,
            &klines_5m,
            &ob,
            95000.0,
            95000.0,
        );
        
        let composite = factors.composite_score();
        assert!(composite > 0.0); // Should be bullish overall
    }
}
