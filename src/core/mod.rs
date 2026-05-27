//! Core types and data structures for the Polymarket HFT system
//! 
//! This module defines all fundamental types used throughout the system,
//! optimized for zero-allocation hot paths.

use serde::{Deserialize, Serialize};
use rust_decimal::Decimal;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Market state for 5-minute prediction contracts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketState {
    /// Market is accepting orders
    Open,
    /// Market is closed, waiting for resolution
    Closed,
    /// Market has been resolved
    Resolved,
    /// Market is being prepared (pre-heating for next cycle)
    Preparing,
}

/// Side of a trade or order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Yes,
    No,
}

impl Side {
    pub fn opposite(&self) -> Self {
        match self {
            Side::Yes => Side::No,
            Side::No => Side::Yes,
        }
    }
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    PostOnly, // Maker-only order to earn rebates
}

/// Order status in the OMS state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderStatus {
    PendingNew,
    New,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Expired,
}

/// High-precision timestamp in microseconds since epoch
pub type TimestampUs = u64;

/// Get current timestamp in microseconds
#[inline]
pub fn now_us() -> TimestampUs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Tick data - the fundamental unit of market data
/// Designed for zero-copy deserialization where possible
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tick {
    pub timestamp_us: TimestampUs,
    pub price: Decimal,      // Price in USDC cents (e.g., 52 = $0.52)
    pub quantity: Decimal,   // Quantity in shares
    pub side: Side,
    pub market_id: String,
}

impl Tick {
    pub fn new(price: Decimal, quantity: Decimal, side: Side, market_id: String) -> Self {
        Self {
            timestamp_us: now_us(),
            price,
            quantity,
            side,
            market_id,
        }
    }
}

/// Order book level (price level)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level {
    pub price: Decimal,
    pub quantity: Decimal,
    pub order_count: u32,
}

/// Order book snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub market_id: String,
    pub timestamp_us: TimestampUs,
    pub bids: Vec<Level>, // Sorted descending by price
    pub asks: Vec<Level>, // Sorted ascending by price
}

impl OrderBook {
    pub fn new(market_id: String) -> Self {
        Self {
            market_id,
            timestamp_us: now_us(),
            bids: Vec::with_capacity(20), // Pre-allocate common depth
            asks: Vec::with_capacity(20),
        }
    }

    /// Calculate order book imbalance (OBI)
    /// OBI = (bid_volume - ask_volume) / (bid_volume + ask_volume)
    /// Range: [-1, 1], positive means buy pressure
    pub fn calculate_obi(&self, depth_levels: usize) -> f64 {
        let bid_vol: f64 = self.bids.iter().take(depth_levels).map(|l| l.quantity.to_f64().unwrap_or(0.0)).sum();
        let ask_vol: f64 = self.asks.iter().take(depth_levels).map(|l| l.quantity.to_f64().unwrap_or(0.0)).sum();
        
        let total = bid_vol + ask_vol;
        if total < 1e-10 {
            return 0.0;
        }
        
        (bid_vol - ask_vol) / total
    }

    /// Get best bid price
    pub fn best_bid(&self) -> Option<Decimal> {
        self.bids.first().map(|l| l.price)
    }

    /// Get best ask price
    pub fn best_ask(&self) -> Option<Decimal> {
        self.asks.first().map(|l| l.price)
    }

    /// Get mid price
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::from(2)),
            _ => None,
        }
    }

    /// Get spread in basis points
    pub fn spread_bps(&self) -> Option<u32> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => {
                let spread = ask - bid;
                let mid = (bid + ask) / Decimal::from(2);
                if mid.is_zero() {
                    None
                } else {
                    Some(((spread / mid) * Decimal::from(10000)).to_u32().unwrap_or(0))
                }
            },
            _ => None,
        }
    }
}

/// OHLCV candlestick data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub timestamp_us: TimestampUs,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub period_seconds: u32, // 60 for 1m, 300 for 5m
}

impl Candle {
    pub fn new(period_seconds: u32) -> Self {
        Self {
            timestamp_us: now_us(),
            open: Decimal::ZERO,
            high: Decimal::ZERO,
            low: Decimal::ZERO,
            close: Decimal::ZERO,
            volume: Decimal::ZERO,
            period_seconds,
        }
    }
}

/// Polymarket-specific market information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInfo {
    pub market_id: String,
    pub token_id_yes: String,
    pub token_id_no: String,
    pub condition_id: String,
    pub question: String,
    pub start_time_us: TimestampUs,
    pub end_time_us: TimestampUs,
    pub state: MarketState,
    pub implied_probability_yes: f64, // Current "support rate" from Yes price
    pub volume_24h: Decimal,
    pub liquidity_yes: Decimal,
    pub liquidity_no: Decimal,
}

impl MarketInfo {
    /// Check if we're in the decision window (last 30 seconds before close)
    pub fn is_in_decision_window(&self) -> bool {
        let now = now_us();
        let time_to_close = self.end_time_us.saturating_sub(now);
        time_to_close <= 30_000_000 && time_to_close > 5_000_000 // 30s to 5s
    }

    /// Check if we're in the no-new-positions window (last 5 seconds)
    pub fn is_in_cooling_period(&self) -> bool {
        let now = now_us();
        let time_to_close = self.end_time_us.saturating_sub(now);
        time_to_close <= 5_000_000 // 5 seconds
    }

    /// Get remaining time in seconds
    pub fn remaining_seconds(&self) -> f64 {
        let now = now_us();
        let remaining = self.end_time_us.saturating_sub(now);
        remaining as f64 / 1_000_000.0
    }
}

/// Order representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id: Uuid,
    pub client_order_id: String,
    pub market_id: String,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Decimal,
    pub quantity: Decimal,
    pub filled_quantity: Decimal,
    pub status: OrderStatus,
    pub created_at_us: TimestampUs,
    pub updated_at_us: TimestampUs,
    pub post_only: bool,
}

impl Order {
    pub fn new_limit(
        market_id: String,
        side: Side,
        price: Decimal,
        quantity: Decimal,
        post_only: bool,
    ) -> Self {
        let now = now_us();
        Self {
            order_id: Uuid::new_v4(),
            client_order_id: format!("{}-{}", side.opposite() as u8, now),
            market_id,
            side,
            order_type: if post_only { OrderType::PostOnly } else { OrderType::Limit },
            price,
            quantity,
            filled_quantity: Decimal::ZERO,
            status: OrderStatus::PendingNew,
            created_at_us: now,
            updated_at_us: now,
            post_only,
        }
    }

    pub fn fill(&mut self, fill_qty: Decimal) {
        self.filled_quantity += fill_qty;
        self.updated_at_us = now_us();
        
        if self.filled_quantity >= self.quantity {
            self.status = OrderStatus::Filled;
        } else if self.filled_quantity > Decimal::ZERO {
            self.status = OrderStatus::PartiallyFilled;
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::PendingNew | OrderStatus::New | OrderStatus::PartiallyFilled
        )
    }
}

/// Trade execution report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub order_id: Uuid,
    pub market_id: String,
    pub side: Side,
    pub price: Decimal,
    pub quantity: Decimal,
    pub fee: Decimal,
    pub is_maker: bool,
    pub timestamp_us: TimestampUs,
}

/// Position in a specific market
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub market_id: String,
    pub yes_shares: Decimal,
    pub no_shares: Decimal,
    pub avg_entry_yes: Decimal,
    pub avg_entry_no: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub last_update_us: TimestampUs,
}

impl Position {
    pub fn new(market_id: String) -> Self {
        Self {
            market_id,
            yes_shares: Decimal::ZERO,
            no_shares: Decimal::ZERO,
            avg_entry_yes: Decimal::ZERO,
            avg_entry_no: Decimal::ZERO,
            realized_pnl: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            last_update_us: now_us(),
        }
    }

    pub fn net_exposure(&self) -> Decimal {
        self.yes_shares - self.no_shares
    }
}

/// Signal generated by the strategy engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub market_id: String,
    pub timestamp_us: TimestampUs,
    pub side: Side,
    pub confidence: f64,        // 0.0 to 1.0
    pub target_quantity: Decimal,
    pub fair_value: Decimal,    // Calculated fair value
    pub factors: SignalFactors, // Breakdown of contributing factors
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SignalFactors {
    pub support_rate_factor: f64,    // Based on implied probability
    pub momentum_factor: f64,         // Based on recent price movement
    pub obi_factor: f64,              // Order book imbalance
    pub cross_exchange_factor: f64,   // Binance/OKX price divergence
    pub kline_pattern_factor: f64,    // 1m/5m candlestick patterns
}

impl Signal {
    pub fn should_execute(&self, min_confidence: f64) -> bool {
        self.confidence >= min_confidence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_book_obi() {
        let mut book = OrderBook::new("market1".to_string());
        
        // Add bid levels
        book.bids.push(Level { price: Decimal::from(50), quantity: Decimal::from(100), order_count: 5 });
        book.bids.push(Level { price: Decimal::from(49), quantity: Decimal::from(50), order_count: 3 });
        
        // Add ask levels
        book.asks.push(Level { price: Decimal::from(52), quantity: Decimal::from(80), order_count: 4 });
        book.asks.push(Level { price: Decimal::from(53), quantity: Decimal::from(40), order_count: 2 });
        
        let obi = book.calculate_obi(2);
        // Bid vol = 150, Ask vol = 120, OBI = (150-120)/(150+120) = 30/270 ≈ 0.111
        assert!((obi - 0.111).abs() < 0.001);
    }

    #[test]
    fn test_market_info_decision_window() {
        let now = now_us();
        let market = MarketInfo {
            market_id: "test".to_string(),
            token_id_yes: "yes".to_string(),
            token_id_no: "no".to_string(),
            condition_id: "cond".to_string(),
            question: "Test".to_string(),
            start_time_us: now - 300_000_000, // 5 minutes ago
            end_time_us: now + 20_000_000,    // 20 seconds from now
            state: MarketState::Open,
            implied_probability_yes: 0.55,
            volume_24h: Decimal::from(10000),
            liquidity_yes: Decimal::from(5000),
            liquidity_no: Decimal::from(5000),
        };

        assert!(market.is_in_decision_window());
        assert!(!market.is_in_cooling_period());
        assert!(market.remaining_seconds() <= 20.0);
    }

    #[test]
    fn test_order_state_machine() {
        let mut order = Order::new_limit(
            "market1".to_string(),
            Side::Yes,
            Decimal::from(50),
            Decimal::from(100),
            true,
        );

        assert_eq!(order.status, OrderStatus::PendingNew);
        assert!(order.is_active());

        order.fill(Decimal::from(60));
        assert_eq!(order.status, OrderStatus::PartiallyFilled);
        assert!(order.is_active());

        order.fill(Decimal::from(40));
        assert_eq!(order.status, OrderStatus::Filled);
        assert!(!order.is_active());
    }
}
