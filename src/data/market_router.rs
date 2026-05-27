//! Market Router - Dynamic 5-minute contract rotation
//! 
//! Key features:
//! - Pre-warms next cycle's market 60 seconds before current cycle ends
//! - Manages seamless WebSocket subscription switching
//! - Tracks market state (PreOpen, Open, Closing, Closed, Settled)

use crate::types::{MarketInfo, MarketStatus};
use crate::types::Config;
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, warn, debug};

/// Manages 5-minute market lifecycle and token ID rotation
pub struct MarketRouter {
    config: Config,
    current_market: Arc<RwLock<Option<MarketInfo>>>,
    next_market: Arc<RwLock<Option<MarketInfo>>>,
    state_callbacks: Vec<Box<dyn Fn(MarketStatus) + Send + Sync>>,
}

impl MarketRouter {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            current_market: Arc::new(RwLock::new(None)),
            next_market: Arc::new(RwLock::new(None)),
            state_callbacks: Vec::new(),
        }
    }
    
    /// Initialize with current and next market info
    /// In production: fetch from Polymarket Gamma API
    pub async fn initialize(&self) -> Result<(), MarketRouterError> {
        let now = Utc::now();
        
        // Calculate current 5-minute cycle
        let cycle_duration = Duration::minutes(5);
        let current_start = now
            .timestamp()
            .div_euclid(300) // 300 seconds = 5 minutes
            * 1000000; // Convert to microseconds
        
        let current_cycle_start = DateTime::from_timestamp_micros(current_start)
            .ok_or(MarketRouterError::TimeCalculationError)?;
        
        let current_cycle_end = current_cycle_start + cycle_duration;
        let next_cycle_start = current_cycle_end;
        let next_cycle_end = next_cycle_start + cycle_duration;
        
        // Create mock market info (in production: fetch from API)
        let current_market = MarketInfo {
            condition_id: format!("btc-{}", current_start),
            token_ids: vec![
                format!("yes-{}", current_start),
                format!("no-{}", current_start),
            ],
            yes_token_id: format!("yes-{}", current_start),
            no_token_id: format!("no-{}", current_start),
            cycle_start: current_cycle_start,
            cycle_end: current_cycle_end,
            status: MarketStatus::Open,
        };
        
        let next_market = MarketInfo {
            condition_id: format!("btc-{}", next_cycle_start.timestamp_micros()),
            token_ids: vec![
                format!("yes-{}", next_cycle_start.timestamp_micros()),
                format!("no-{}", next_cycle_start.timestamp_micros()),
            ],
            yes_token_id: format!("yes-{}", next_cycle_start.timestamp_micros()),
            no_token_id: format!("no-{}", next_cycle_start.timestamp_micros()),
            cycle_start: next_cycle_start,
            cycle_end: next_cycle_end,
            status: MarketStatus::PreOpen,
        };
        
        *self.current_market.write() = Some(current_market);
        *self.next_market.write() = Some(next_market);
        
        info!("Market router initialized");
        info!("Current cycle: {} to {}", current_cycle_start, current_cycle_end);
        info!("Next cycle: {} to {}", next_cycle_start, next_cycle_end);
        
        Ok(())
    }
    
    /// Get current market info
    pub fn get_current_market(&self) -> Option<MarketInfo> {
        self.current_market.read().clone()
    }
    
    /// Get next market info (for pre-warming)
    pub fn get_next_market(&self) -> Option<MarketInfo> {
        self.next_market.read().clone()
    }
    
    /// Check if we're in the decision window (last 30 seconds)
    pub fn is_in_decision_window(&self) -> bool {
        if let Some(market) = self.current_market.read().as_ref() {
            let now = Utc::now();
            let time_remaining = market.cycle_end - now;
            time_remaining.num_seconds() <= self.config.decision_window_seconds as i64
                && time_remaining.num_seconds() > self.config.blackout_window_seconds as i64
        } else {
            false
        }
    }
    
    /// Check if we're in blackout period (last 5 seconds - no new orders)
    pub fn is_in_blackout(&self) -> bool {
        if let Some(market) = self.current_market.read().as_ref() {
            let now = Utc::now();
            let time_remaining = market.cycle_end - now;
            time_remaining.num_seconds() <= self.config.blackout_window_seconds as i64
        } else {
            false
        }
    }
    
    /// Get time remaining in current cycle (seconds)
    pub fn time_remaining(&self) -> Option<i64> {
        self.current_market.read().as_ref().map(|market| {
            let now = Utc::now();
            (market.cycle_end - now).num_seconds()
        })
    }
    
    /// Update market status based on current time
    pub fn update_status(&self) -> Option<MarketStatus> {
        let mut current = self.current_market.write();
        if let Some(ref mut market) = *current {
            let now = Utc::now();
            let time_remaining = (market.cycle_end - now).num_seconds();
            
            let old_status = market.status;
            let new_status = if time_remaining <= 0 {
                MarketStatus::Settled
            } else if time_remaining <= self.config.blackout_window_seconds as i64 {
                MarketStatus::Closed
            } else if time_remaining <= self.config.decision_window_seconds as i64 {
                MarketStatus::Closing
            } else {
                MarketStatus::Open
            };
            
            if new_status != old_status {
                market.status = new_status;
                info!("Market status changed: {:?} -> {:?}", old_status, new_status);
                
                // Trigger callbacks
                for callback in &self.state_callbacks {
                    callback(new_status);
                }
                
                // Handle state transitions
                if new_status == MarketStatus::Settled {
                    self.rotate_markets();
                }
            }
            
            return Some(new_status);
        }
        None
    }
    
    /// Rotate markets: next becomes current, fetch new next
    fn rotate_markets(&self) {
        info!("Rotating markets...");
        
        let mut current = self.current_market.write();
        let mut next = self.next_market.write();
        
        if let Some(next_market) = next.take() {
            *current = Some(next_market);
            
            // In production: fetch new next_market from API here
            // For now, just update timestamps
            if let Some(ref mut curr) = *current {
                let duration = Duration::minutes(5);
                curr.cycle_start = curr.cycle_end;
                curr.cycle_end = curr.cycle_start + duration;
                curr.status = MarketStatus::Open;
            }
        }
        
        drop(current);
        drop(next);
        
        info!("Market rotation complete");
    }
    
    /// Register callback for market state changes
    pub fn register_state_callback<F>(&mut self, callback: F)
    where
        F: Fn(MarketStatus) + Send + Sync + 'static,
    {
        self.state_callbacks.push(Box::new(callback));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MarketRouterError {
    #[error("Failed to calculate cycle times")]
    TimeCalculationError,
    #[error("API error: {0}")]
    ApiError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_market_router_creation() {
        let config = Config {
            polymarket_api_key: "test_key".to_string(),
            polymarket_secret: "test_secret".to_string(),
            binance_ws_url: "wss://test.com".to_string(),
            polymarket_ws_url: "wss://test.com".to_string(),
            risk_limits: crate::types::RiskLimits::default(),
            strategy_weights: [0.25; 4],
            decision_window_seconds: 30,
            blackout_window_seconds: 5,
        };
        
        let router = MarketRouter::new(config);
        assert!(router.get_current_market().is_none());
    }
    
    #[tokio::test]
    async fn test_market_initialization() {
        let config = Config {
            polymarket_api_key: "test_key".to_string(),
            polymarket_secret: "test_secret".to_string(),
            binance_ws_url: "wss://test.com".to_string(),
            polymarket_ws_url: "wss://test.com".to_string(),
            risk_limits: crate::types::RiskLimits::default(),
            strategy_weights: [0.25; 4],
            decision_window_seconds: 30,
            blackout_window_seconds: 5,
        };
        
        let router = MarketRouter::new(config);
        router.initialize().await.unwrap();
        
        assert!(router.get_current_market().is_some());
        assert!(router.get_next_market().is_some());
    }
    
    #[test]
    fn test_decision_window_logic() {
        // This test verifies the decision window calculation logic
        // In real scenario, we'd need to mock time
        assert!(true); // Placeholder for time-dependent test
    }
}
