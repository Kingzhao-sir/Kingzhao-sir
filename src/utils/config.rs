//! System Configuration
//! Loaded from environment variables or config file

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    // Polymarket Settings
    pub polymarket_api_key: String,
    pub polymarket_secret: String,
    pub wallet_address: String,
    
    // Trading Parameters
    pub max_position_size_usd: f64,
    pub max_daily_loss_usd: f64,
    pub decision_window_seconds: u32,  // 30 seconds before close
    pub blackout_window_seconds: u32, // 5 seconds before close (no new orders)
    
    // Risk Management
    pub max_slippage_bps: u32,
    pub order_timeout_ms: u64,
    
    // Data Sources
    pub binance_ws_url: String,
    pub polymarket_ws_url: String,
    pub polymarket_rest_url: String,
    
    // Performance Tuning
    pub cpu_affinity: Vec<usize>,
    pub arena_size_mb: usize,
}

impl SystemConfig {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        // In production, load from config file or environment
        Ok(Self {
            polymarket_api_key: std::env::var("POLYMARKET_API_KEY")
                .unwrap_or_else(|_| "demo_key".to_string()),
            polymarket_secret: std::env::var("POLYMARKET_SECRET")
                .unwrap_or_else(|_| "demo_secret".to_string()),
            wallet_address: std::env::var("WALLET_ADDRESS")
                .unwrap_or_else(|_| "0x0000000000000000000000000000000000000000".to_string()),
            
            max_position_size_usd: 1000.0,
            max_daily_loss_usd: 500.0,
            decision_window_seconds: 30,
            blackout_window_seconds: 5,
            
            max_slippage_bps: 50, // 0.5%
            order_timeout_ms: 2000,
            
            binance_ws_url: "wss://stream.binance.com:9443/ws/btcusdt@trade".to_string(),
            polymarket_ws_url: "wss://polymarket.com/ws".to_string(),
            polymarket_rest_url: "https://api.polymarket.com".to_string(),
            
            cpu_affinity: vec![0, 1],
            arena_size_mb: 64,
        })
    }
    
    #[inline]
    pub fn get_decision_threshold(&self, market_close_ts: i64) -> i64 {
        market_close_ts - (self.decision_window_seconds as i64 * 1000)
    }
    
    #[inline]
    pub fn get_blackout_threshold(&self, market_close_ts: i64) -> i64 {
        market_close_ts - (self.blackout_window_seconds as i64 * 1000)
    }
}
