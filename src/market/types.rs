//! Market types for Polymarket 5-minute BTC contracts

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInfo {
    pub condition_id: String,
    pub token_id_yes: String,
    pub token_id_no: String,
    pub close_time_ms: i64,
    pub open_time_ms: i64,
    pub status: MarketStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatus {
    Open,
    Closing,
    Closed,
    Resolving,
    Resolved,
}

/// Implied probability (support rate) from market price
#[derive(Debug, Clone, Copy)]
pub struct MarketProbabilities {
    pub yes_price: f64,      // 0.0 - 1.0
    pub no_price: f64,       // 0.0 - 1.0
    pub implied_prob_yes: f64, // Same as yes_price in binary markets
    pub spread_bps: u32,
}

impl MarketProbabilities {
    pub fn from_prices(yes_price: f64, no_price: f64) -> Self {
        let spread = ((yes_price - no_price).abs() * 10000.0) as u32;
        Self {
            yes_price,
            no_price,
            implied_prob_yes: yes_price,
            spread_bps: spread,
        }
    }
}
