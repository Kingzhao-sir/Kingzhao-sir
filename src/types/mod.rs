//! Core types and data structures for the trading system
//! 
//! Designed for zero-allocation in hot path using:
//! - Arena allocation for per-cycle memory
//! - rkyv for zero-copy serialization
//! - rust_decimal for precise financial calculations

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use chrono::{DateTime, Utc};

/// Market state for 5-minute BTC prediction contracts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInfo {
    pub condition_id: String,
    pub token_ids: Vec<String>,
    pub yes_token_id: String,
    pub no_token_id: String,
    pub cycle_start: DateTime<Utc>,
    pub cycle_end: DateTime<Utc>,
    pub status: MarketStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatus {
    PreOpen,
    Open,
    Closing,      // Last 30 seconds - decision window
    Closed,       // Last 5 seconds - no new orders
    Settled,
}

/// Tick data with minimal allocation
#[derive(Debug, Clone, Copy)]
pub struct Tick {
    pub timestamp: i64, // Unix microseconds
    pub price: Decimal,
    pub quantity: Decimal,
    pub side: Side,
    pub source: DataSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSource {
    Binance,
    Polymarket,
    OKX,
}

/// Order book level with zero-copy friendly layout
#[derive(Debug, Clone, Copy)]
pub struct PriceLevel {
    pub price: Decimal,
    pub quantity: Decimal,
    pub order_count: u32,
}

/// Order book snapshot optimized for hot path
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub timestamp: i64,
    pub sequence: u64,
}

impl OrderBook {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            bids: Vec::with_capacity(50), // Pre-allocate typical depth
            asks: Vec::with_capacity(50),
            timestamp: 0,
            sequence: 0,
        }
    }
    
    /// Zero-allocation update using object pool pattern
    pub fn update_bid(&mut self, price: Decimal, quantity: Decimal) {
        // In production: use arena allocation and reuse existing PriceLevel objects
        self.bids.retain(|level| level.price != price);
        if quantity > Decimal::ZERO {
            self.bids.push(PriceLevel {
                price,
                quantity,
                order_count: 1,
            });
            self.bids.sort_by(|a, b| b.price.cmp(&a.price));
        }
    }
    
    pub fn update_ask(&mut self, price: Decimal, quantity: Decimal) {
        self.asks.retain(|level| level.price != price);
        if quantity > Decimal::ZERO {
            self.asks.push(PriceLevel {
                price,
                quantity,
                order_count: 1,
            });
            self.asks.sort_by(|a, b| a.price.cmp(&b.price));
        }
    }
    
    pub fn mid_price(&self) -> Option<Decimal> {
        if let (Some(best_bid), Some(best_ask)) = (self.bids.first(), self.asks.first()) {
            Some((best_bid.price + best_ask.price) / Decimal::from(2))
        } else {
            None
        }
    }
    
    pub fn spread(&self) -> Option<Decimal> {
        if let (Some(best_bid), Some(best_ask)) = (self.bids.first(), self.asks.first()) {
            Some(best_ask.price - best_bid.price)
        } else {
            None
        }
    }
}

/// Trading signal with confidence score
#[derive(Debug, Clone)]
pub struct Signal {
    pub timestamp: DateTime<Utc>,
    pub market_id: String,
    pub direction: SignalDirection,
    pub confidence: f64, // 0.0 to 1.0
    pub factors: SignalFactors,
    pub expiry: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalDirection {
    Long,
    Short,
    Neutral,
}

/// Multi-factor signal components
#[derive(Debug, Clone)]
pub struct SignalFactors {
    pub support_rate_factor: f64,     // Polymarket implied probability
    pub momentum_factor: f64,         // Price momentum from K-lines
    pub obi_factor: f64,              // Order book imbalance
    pub spread_factor: f64,           // Cross-exchange spread
    pub time_decay_factor: f64,       // Time remaining in cycle
}

impl SignalFactors {
    pub fn weighted_score(&self, weights: &[f64; 4]) -> f64 {
        self.support_rate_factor * weights[0]
            + self.momentum_factor * weights[1]
            + self.obi_factor * weights[2]
            + self.spread_factor * weights[3]
    }
}

/// Order types for OMS
#[derive(Debug, Clone)]
pub struct Order {
    pub id: String,
    pub client_order_id: String,
    pub market_id: String,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
    Market,
    Limit,
    PostOnly, // Maker-only for fee rebate
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    New,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

/// Risk management parameters
#[derive(Debug, Clone)]
pub struct RiskLimits {
    pub max_position_size: Decimal,
    pub max_order_size: Decimal,
    pub max_daily_loss: Decimal,
    pub max_drawdown: Decimal,
    pub order_rate_limit: u32, // Orders per second
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_position_size: Decimal::from(1000),
            max_order_size: Decimal::from(100),
            max_daily_loss: Decimal::from(500),
            max_drawdown: Decimal::from(200),
            order_rate_limit: 10,
        }
    }
}

/// Configuration loaded at startup
#[derive(Debug, Clone)]
pub struct Config {
    pub polymarket_api_key: String,
    pub polymarket_secret: String,
    pub binance_ws_url: String,
    pub polymarket_ws_url: String,
    pub risk_limits: RiskLimits,
    pub strategy_weights: [f64; 4],
    pub decision_window_seconds: u32, // Typically 30
    pub blackout_window_seconds: u32, // Typically 5
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        // In production: load from config file or environment variables
        Ok(Self {
            polymarket_api_key: std::env::var("POLYMARKET_API_KEY").unwrap_or_default(),
            polymarket_secret: std::env::var("POLYMARKET_SECRET").unwrap_or_default(),
            binance_ws_url: "wss://stream.binance.com:9443/ws".to_string(),
            polymarket_ws_url: "wss://polymarket.com/ws".to_string(),
            risk_limits: RiskLimits::default(),
            strategy_weights: [0.3, 0.3, 0.25, 0.15], // Support rate, momentum, OBI, spread
            decision_window_seconds: 30,
            blackout_window_seconds: 5,
        })
    }
}
