//! Data types for ticks, trades, and order book updates

use serde::{Deserialize, Serialize};

/// Price side (Bid/Ask or Buy/Sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

/// Tick data - minimal structure for zero-copy processing
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Tick {
    pub timestamp_ms: i64,
    pub price: f64,
    pub quantity: f64,
    pub side: Side,
    pub source: DataSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataSource {
    Binance,
    Polymarket,
    OKX,
}

/// Order book level (price + size)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Level {
    pub price: f64,
    pub size: f64,
    pub order_count: u32,
}

/// Snapshot of order book (top N levels)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub timestamp_ms: i64,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
    pub spread_bps: u32,
    pub mid_price: f64,
}

impl OrderBookSnapshot {
    pub fn new(timestamp_ms: i64, bids: Vec<Level>, asks: Vec<Level>) -> Self {
        let mid = if !bids.is_empty() && !asks.is_empty() {
            (bids[0].price + asks[0].price) / 2.0
        } else if !bids.is_empty() {
            bids[0].price
        } else if !asks.is_empty() {
            asks[0].price
        } else {
            0.0
        };
        
        let spread = if !bids.is_empty() && !asks.is_empty() {
            ((asks[0].price - bids[0].price) / mid * 10000.0) as u32
        } else {
            0
        };
        
        Self {
            timestamp_ms,
            bids,
            asks,
            spread_bps: spread,
            mid_price: mid,
        }
    }
    
    /// Calculate Order Book Imbalance (OBI)
    /// Positive = more buy pressure, Negative = more sell pressure
    #[inline]
    pub fn calculate_obi(&self) -> f64 {
        let bid_volume: f64 = self.bids.iter().map(|l| l.size).sum();
        let ask_volume: f64 = self.asks.iter().map(|l| l.size).sum();
        let total = bid_volume + ask_volume;
        
        if total == 0.0 {
            0.0
        } else {
            (bid_volume - ask_volume) / total
        }
    }
}

/// 1-minute K-line (OHLCV)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Kline1m {
    pub timestamp_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// 5-minute K-line (OHLCV)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Kline5m {
    pub timestamp_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}
