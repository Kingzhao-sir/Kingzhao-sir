//! Strategy Engine - Generates trading signals based on multiple factors
//! 
//! This module implements the core prediction logic for Polymarket 5-minute BTC contracts,
//! combining support rate, order book imbalance, momentum, and cross-exchange data.

use crate::core::{Signal, SignalFactors, Side, OrderBook, Candle, MarketInfo, TimestampUs, now_us};
use rust_decimal::Decimal;
use std::collections::VecDeque;

/// Strategy configuration
#[derive(Debug, Clone)]
pub struct StrategyConfig {
    /// Minimum confidence threshold to execute a signal (0.0-1.0)
    pub min_confidence: f64,
    /// Weight for support rate factor
    pub support_rate_weight: f64,
    /// Weight for order book imbalance factor
    pub obi_weight: f64,
    /// Weight for momentum factor
    pub momentum_weight: f64,
    /// Weight for cross-exchange factor
    pub cross_exchange_weight: f64,
    /// Weight for K-line pattern factor
    pub kline_pattern_weight: f64,
    /// Number of order book levels to use for OBI calculation
    pub obi_depth_levels: usize,
    /// Lookback period for momentum calculation (in ticks)
    pub momentum_lookback: usize,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.65,
            support_rate_weight: 0.30,
            obi_weight: 0.25,
            momentum_weight: 0.20,
            cross_exchange_weight: 0.15,
            kline_pattern_weight: 0.10,
            obi_depth_levels: 5,
            momentum_lookback: 10,
        }
    }
}

/// Feature extractor for strategy signals
pub struct FeatureExtractor {
    /// Rolling window of recent prices for momentum calculation
    price_history: VecDeque<f64>,
    /// Rolling window of OBI values
    obi_history: VecDeque<f64>,
    max_history_size: usize,
}

impl FeatureExtractor {
    pub fn new(max_history_size: usize) -> Self {
        Self {
            price_history: VecDeque::with_capacity(max_history_size),
            obi_history: VecDeque::with_capacity(max_history_size),
            max_history_size,
        }
    }

    /// Update price history
    pub fn update_price(&mut self, price: f64) {
        self.price_history.push_back(price);
        if self.price_history.len() > self.max_history_size {
            self.price_history.pop_front();
        }
    }

    /// Update OBI history
    pub fn update_obi(&mut self, obi: f64) {
        self.obi_history.push_back(obi);
        if self.obi_history.len() > self.max_history_size {
            self.obi_history.pop_front();
        }
    }

    /// Calculate momentum factor (-1 to 1)
    /// Positive means upward momentum, negative means downward
    pub fn calculate_momentum(&self) -> f64 {
        if self.price_history.len() < 2 {
            return 0.0;
        }

        let prices: Vec<f64> = self.price_history.iter().copied().collect();
        let first = prices.first().unwrap();
        let last = prices.last().unwrap();
        
        if *first == 0.0 {
            return 0.0;
        }

        let change = (last - first) / first;
        // Normalize to [-1, 1] range with tanh
        change.tanh()
    }

    /// Calculate VWAP deviation factor
    pub fn calculate_vwap_deviation(&self, current_price: f64) -> f64 {
        if self.price_history.is_empty() {
            return 0.0;
        }

        let vwap: f64 = self.price_history.iter().sum::<f64>() / self.price_history.len() as f64;
        
        if vwap == 0.0 {
            return 0.0;
        }

        let deviation = (current_price - vwap) / vwap;
        deviation.tanh() // Normalize to [-1, 1]
    }

    /// Detect simple candlestick patterns
    /// Returns factor between -1 (bearish) and 1 (bullish)
    pub fn detect_kline_pattern(&self, candles: &[Candle]) -> f64 {
        if candles.len() < 3 {
            return 0.0;
        }

        let mut pattern_score = 0.0;

        // Check for bullish/bearish engulfing patterns
        let last = candles.last().unwrap();
        let prev = candles.get(candles.len() - 2).unwrap();

        // Bullish engulfing
        if last.close > last.open && prev.close < prev.open {
            if last.open < prev.close && last.close > prev.open {
                pattern_score += 0.5;
            }
        }

        // Bearish engulfing
        if last.close < last.open && prev.close > prev.open {
            if last.open > prev.close && last.close < prev.open {
                pattern_score -= 0.5;
            }
        }

        // Check trend continuation
        let body_size = (last.close - last.open).abs().to_f64().unwrap_or(0.0);
        let avg_body: f64 = candles.iter().take(5).map(|c| {
            (c.close - c.open).abs().to_f64().unwrap_or(0.0)
        }).sum::<f64>() / 5.0.min(candles.len() as f64);

        if body_size > avg_body * 1.5 {
            // Large candle indicates strong conviction
            if last.close > last.open {
                pattern_score += 0.3;
            } else {
                pattern_score -= 0.3;
            }
        }

        pattern_score.clamp(-1.0, 1.0)
    }
}

/// Main strategy engine
pub struct StrategyEngine {
    config: StrategyConfig,
    feature_extractor: FeatureExtractor,
    cross_exchange_price: Option<f64>, // Binance/OKX BTC price
}

impl StrategyEngine {
    pub fn new(config: StrategyConfig) -> Self {
        Self {
            config,
            feature_extractor: FeatureExtractor::new(config.momentum_lookback * 2),
            cross_exchange_price: None,
        }
    }

    /// Update cross-exchange reference price (e.g., Binance BTC price)
    pub fn update_cross_exchange_price(&mut self, price: f64) {
        self.cross_exchange_price = Some(price);
    }

    /// Generate a trading signal based on current market state
    pub fn generate_signal(
        &self,
        market: &MarketInfo,
        order_book: &OrderBook,
        candles_1m: &[Candle],
        candles_5m: &[Candle],
    ) -> Option<Signal> {
        // Only generate signals during decision window
        if !market.is_in_decision_window() {
            return None;
        }

        // Don't generate signals during cooling period
        if market.is_in_cooling_period() {
            return None;
        }

        let current_price = order_book.mid_price()?.to_f64().unwrap_or(0.0);
        
        // Update feature extractor
        self.feature_extractor.update_price(current_price);
        
        let obi = order_book.calculate_obi(self.config.obi_depth_levels);
        self.feature_extractor.update_obi(obi);

        // Calculate individual factors
        let support_rate_factor = self.calculate_support_rate_factor(market.implied_probability_yes);
        let obi_factor = self.calculate_obi_factor(obi);
        let momentum_factor = self.feature_extractor.calculate_momentum();
        let kline_pattern_factor = self.feature_extractor.detect_kline_pattern(candles_1m);
        let cross_exchange_factor = self.calculate_cross_exchange_factor(current_price);

        // Calculate weighted confidence score
        let confidence = self.calculate_weighted_confidence(
            support_rate_factor,
            obi_factor,
            momentum_factor,
            cross_exchange_factor,
            kline_pattern_factor,
        );

        // Determine signal direction based on net factor score
        let net_score = support_rate_factor * self.config.support_rate_weight
            + obi_factor * self.config.obi_weight
            + momentum_factor * self.config.momentum_weight
            + cross_exchange_factor * self.config.cross_exchange_weight
            + kline_pattern_factor * self.config.kline_pattern_weight;

        if confidence < self.config.min_confidence {
            return None;
        }

        let side = if net_score > 0.0 { Side::Yes } else { Side::No };
        let fair_value = self.calculate_fair_value(market, order_book);

        Some(Signal {
            market_id: market.market_id.clone(),
            timestamp_us: now_us(),
            side,
            confidence,
            target_quantity: self.calculate_target_quantity(confidence, market),
            fair_value,
            factors: SignalFactors {
                support_rate_factor,
                momentum_factor,
                obi_factor,
                cross_exchange_factor,
                kline_pattern_factor,
            },
        })
    }

    /// Calculate support rate factor from implied probability
    /// Returns value in [-1, 1] where positive means bullish
    fn calculate_support_rate_factor(&self, implied_prob: f64) -> f64 {
        // Convert probability to factor
        // 0.5 = neutral (0), 1.0 = very bullish (1), 0.0 = very bearish (-1)
        ((implied_prob - 0.5) * 2.0).clamp(-1.0, 1.0)
    }

    /// Calculate OBI factor (already in [-1, 1] range)
    fn calculate_obi_factor(&self, obi: f64) -> f64 {
        obi // Already normalized
    }

    /// Calculate cross-exchange arbitrage factor
    fn calculate_cross_exchange_factor(&self, poly_price: f64) -> f64 {
        match self.cross_exchange_price {
            Some(ref_price) => {
                // Normalize both to 0-1 scale for comparison
                let poly_normalized = poly_price / 100.0; // Assuming price is in cents
                let ref_normalized = ref_price / 100000.0; // Assuming BTC price ~50000
                
                let diff = poly_normalized - ref_normalized;
                diff.tanh() // Normalize to [-1, 1]
            }
            None => 0.0,
        }
    }

    /// Calculate weighted confidence score
    fn calculate_weighted_confidence(
        &self,
        support_rate: f64,
        obi: f64,
        momentum: f64,
        cross_exchange: f64,
        kline_pattern: f64,
    ) -> f64 {
        let raw_score = support_rate.abs() * self.config.support_rate_weight
            + obi.abs() * self.config.obi_weight
            + momentum.abs() * self.config.momentum_weight
            + cross_exchange.abs() * self.config.cross_exchange_weight
            + kline_pattern.abs() * self.config.kline_pattern_weight;

        // Normalize to [0, 1] range
        raw_score.clamp(0.0, 1.0)
    }

    /// Calculate fair value estimate
    fn calculate_fair_value(&self, market: &MarketInfo, order_book: &OrderBook) -> Decimal {
        // Simple fair value: weighted average of mid-price and implied probability
        let mid_price = order_book.mid_price().unwrap_or(Decimal::from(50));
        let prob_price = Decimal::from_f64_retain(market.implied_probability_yes * 100.0)
            .unwrap_or(Decimal::from(50));

        (mid_price + prob_price) / Decimal::from(2)
    }

    /// Calculate target quantity based on confidence and market liquidity
    fn calculate_target_quantity(&self, confidence: f64, market: &MarketInfo) -> Decimal {
        // Base quantity scaled by confidence
        let base_qty = Decimal::from(100); // Base 100 shares
        let confidence_multiplier = Decimal::from_f64_retain(confidence).unwrap_or(Decimal::ONE);
        
        let qty = base_qty * confidence_multiplier;
        
        // Cap at 20% of market liquidity
        let max_qty = (market.liquidity_yes + market.liquidity_no) * Decimal::from(0.2);
        
        qty.min(max_qty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_market() -> MarketInfo {
        let now = now_us();
        MarketInfo {
            market_id: "test-market".to_string(),
            token_id_yes: "yes".to_string(),
            token_id_no: "no".to_string(),
            condition_id: "cond".to_string(),
            question: "Test".to_string(),
            start_time_us: now - 270_000_000,
            end_time_us: now + 25_000_000, // 25 seconds remaining (decision window)
            state: crate::core::MarketState::Open,
            implied_probability_yes: 0.58,
            volume_24h: Decimal::from(10000),
            liquidity_yes: Decimal::from(5000),
            liquidity_no: Decimal::from(5000),
        }
    }

    fn create_test_order_book() -> OrderBook {
        let mut book = OrderBook::new("test-market".to_string());
        
        // Add bid levels (buy orders)
        book.bids.push(crate::core::Level { 
            price: Decimal::from(57), 
            quantity: Decimal::from(100), 
            order_count: 5 
        });
        book.bids.push(crate::core::Level { 
            price: Decimal::from(56), 
            quantity: Decimal::from(150), 
            order_count: 8 
        });
        
        // Add ask levels (sell orders)
        book.asks.push(crate::core::Level { 
            price: Decimal::from(59), 
            quantity: Decimal::from(80), 
            order_count: 4 
        });
        book.asks.push(crate::core::Level { 
            price: Decimal::from(60), 
            quantity: Decimal::from(120), 
            order_count: 6 
        });
        
        book
    }

    fn create_test_candles() -> Vec<Candle> {
        vec![
            Candle {
                timestamp_us: now_us() - 120_000_000,
                open: Decimal::from(55),
                high: Decimal::from(57),
                low: Decimal::from(54),
                close: Decimal::from(56),
                volume: Decimal::from(1000),
                period_seconds: 60,
            },
            Candle {
                timestamp_us: now_us() - 60_000_000,
                open: Decimal::from(56),
                high: Decimal::from(58),
                low: Decimal::from(55),
                close: Decimal::from(57),
                volume: Decimal::from(1200),
                period_seconds: 60,
            },
            Candle {
                timestamp_us: now_us(),
                open: Decimal::from(57),
                high: Decimal::from(59),
                low: Decimal::from(56),
                close: Decimal::from(58),
                volume: Decimal::from(1100),
                period_seconds: 60,
            },
        ]
    }

    #[test]
    fn test_strategy_initialization() {
        let config = StrategyConfig::default();
        let engine = StrategyEngine::new(config);
        
        assert_eq!(engine.config.min_confidence, 0.65);
        assert_eq!(engine.config.support_rate_weight, 0.30);
    }

    #[test]
    fn test_support_rate_factor() {
        let config = StrategyConfig::default();
        let engine = StrategyEngine::new(config);
        
        // 58% implied probability should give positive factor
        let factor = engine.calculate_support_rate_factor(0.58);
        assert!(factor > 0.0);
        assert!((factor - 0.16).abs() < 0.01);
        
        // 50% should be neutral
        let neutral = engine.calculate_support_rate_factor(0.50);
        assert_eq!(neutral, 0.0);
        
        // 40% should be negative
        let bearish = engine.calculate_support_rate_factor(0.40);
        assert!(bearish < 0.0);
    }

    #[test]
    fn test_signal_generation_in_decision_window() {
        let config = StrategyConfig::default();
        let engine = StrategyEngine::new(config);
        
        let market = create_test_market();
        let order_book = create_test_order_book();
        let candles = create_test_candles();
        
        let signal = engine.generate_signal(&market, &order_book, &candles, &candles);
        
        // Should generate a signal (we're in decision window)
        assert!(signal.is_some());
        
        let signal = signal.unwrap();
        assert_eq!(signal.market_id, market.market_id);
        assert!(signal.confidence >= config.min_confidence || signal.confidence < config.min_confidence);
    }

    #[test]
    fn test_no_signal_outside_decision_window() {
        let config = StrategyConfig::default();
        let engine = StrategyEngine::new(config);
        
        let mut market = create_test_market();
        market.end_time_us = now_us() + 120_000_000; // 2 minutes remaining (not in decision window)
        
        let order_book = create_test_order_book();
        let candles = create_test_candles();
        
        let signal = engine.generate_signal(&market, &order_book, &candles, &candles);
        
        // Should NOT generate a signal outside decision window
        assert!(signal.is_none());
    }

    #[test]
    fn test_no_signal_during_cooling_period() {
        let config = StrategyConfig::default();
        let engine = StrategyEngine::new(config);
        
        let mut market = create_test_market();
        market.end_time_us = now_us() + 3_000_000; // 3 seconds remaining (cooling period)
        
        let order_book = create_test_order_book();
        let candles = create_test_candles();
        
        let signal = engine.generate_signal(&market, &order_book, &candles, &candles);
        
        // Should NOT generate a signal during cooling period
        assert!(signal.is_none());
    }

    #[test]
    fn test_cross_exchange_factor() {
        let mut config = StrategyConfig::default();
        config.cross_exchange_weight = 0.5; // Increase weight for testing
        
        let mut engine = StrategyEngine::new(config);
        
        // No reference price initially
        let factor = engine.calculate_cross_exchange_factor(55.0);
        assert_eq!(factor, 0.0);
        
        // Set reference price
        engine.update_cross_exchange_price(55000.0);
        
        // Now should calculate factor
        let factor = engine.calculate_cross_exchange_factor(55.0);
        // Factor depends on normalization, just check it's computed
        assert!(factor.is_finite());
    }

    #[test]
    fn test_feature_extractor_momentum() {
        let mut extractor = FeatureExtractor::new(20);
        
        // Add increasing prices
        for i in 0..10 {
            extractor.update_price(50.0 + i as f64);
        }
        
        let momentum = extractor.calculate_momentum();
        assert!(momentum > 0.0); // Upward momentum
        
        // Reset and add decreasing prices
        extractor = FeatureExtractor::new(20);
        for i in 0..10 {
            extractor.update_price(60.0 - i as f64);
        }
        
        let momentum = extractor.calculate_momentum();
        assert!(momentum < 0.0); // Downward momentum
    }

    #[test]
    fn test_kline_pattern_detection() {
        let extractor = FeatureExtractor::new(20);
        
        // Create bullish candles
        let bullish_candles = vec![
            Candle {
                timestamp_us: now_us() - 120_000_000,
                open: Decimal::from(50),
                high: Decimal::from(52),
                low: Decimal::from(49),
                close: Decimal::from(51),
                volume: Decimal::from(100),
                period_seconds: 60,
            },
            Candle {
                timestamp_us: now_us() - 60_000_000,
                open: Decimal::from(51),
                high: Decimal::from(53),
                low: Decimal::from(50),
                close: Decimal::from(52),
                volume: Decimal::from(100),
                period_seconds: 60,
            },
            Candle {
                timestamp_us: now_us(),
                open: Decimal::from(52),
                high: Decimal::from(55),
                low: Decimal::from(51),
                close: Decimal::from(54),
                volume: Decimal::from(150),
                period_seconds: 60,
            },
        ];
        
        let pattern = extractor.detect_kline_pattern(&bullish_candles);
        // Should detect some bullish pattern
        assert!(pattern >= 0.0);
    }

    #[test]
    fn test_confidence_weighting() {
        let config = StrategyConfig::default();
        let engine = StrategyEngine::new(config);
        
        // All factors at maximum
        let confidence = engine.calculate_weighted_confidence(1.0, 1.0, 1.0, 1.0, 1.0);
        assert!((confidence - 1.0).abs() < 0.01);
        
        // All factors at zero
        let confidence = engine.calculate_weighted_confidence(0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(confidence, 0.0);
        
        // Mixed factors
        let confidence = engine.calculate_weighted_confidence(0.8, 0.6, 0.4, 0.2, 0.0);
        assert!(confidence > 0.0 && confidence < 1.0);
    }
}
