//! Dynamic Market Router
//! Handles 5-minute market lifecycle and seamless rollover

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

use crate::utils::config::SystemConfig;
use crate::market::types::{MarketInfo, MarketStatus};
use crate::utils::time::{get_current_market_window, seconds_to_market_close};

pub struct MarketRouter {
    config: SystemConfig,
    current_market: Arc<RwLock<MarketInfo>>,
    next_market: Arc<RwLock<Option<MarketInfo>>>,
}

impl MarketRouter {
    pub async fn new(config: SystemConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let (start_ms, end_ms) = get_current_market_window();
        
        let current_market = MarketInfo {
            condition_id: format!("btc-5m-{}", start_ms),
            token_id_yes: format!("yes-{}", start_ms),
            token_id_no: format!("no-{}", start_ms),
            close_time_ms: end_ms,
            open_time_ms: start_ms,
            status: MarketStatus::Open,
        };
        
        Ok(Self {
            config,
            current_market: Arc::new(RwLock::new(current_market)),
            next_market: Arc::new(RwLock::new(None)),
        })
    }
    
    /// Get current market ID for trading
    #[inline]
    pub fn get_current_market_id(&self) -> String {
        // In production, use atomic read without full lock
        let market = self.current_market.try_read().unwrap();
        market.token_id_yes.clone()
    }
    
    /// Get seconds until market rollover
    #[inline]
    pub fn get_seconds_to_rollover(&self) -> u32 {
        seconds_to_market_close()
    }
    
    /// Check if we should prepare next market (60 seconds before close)
    #[inline]
    pub fn should_prepare_next_market(&self) -> bool {
        seconds_to_market_close() <= 60
    }
    
    /// Check if we're in decision window (last 30 seconds)
    #[inline]
    pub fn is_in_decision_window(&self) -> bool {
        seconds_to_market_close() <= self.config.decision_window_seconds
    }
    
    /// Check if we're in blackout window (last 5 seconds - no new orders)
    #[inline]
    pub fn is_in_blackout_window(&self) -> bool {
        seconds_to_market_close() <= self.config.blackout_window_seconds
    }
    
    /// Get current market implied probability (support rate)
    pub async fn get_implied_probability(&self) -> Option<f64> {
        // In production, this would fetch from live order book
        let market = self.current_market.try_read().unwrap();
        // Placeholder - would be calculated from bid/ask
        Some(0.52) // Example: 52% support rate
    }
    
    /// Prepare next market connection (called 60s before close)
    pub async fn prepare_next_market(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.next_market.try_read().unwrap().is_some() {
            return Ok(()); // Already prepared
        }
        
        let (start_ms, end_ms) = get_current_market_window();
        let next_start = start_ms + 300_000;
        let next_end = end_ms + 300_000;
        
        let next_market = MarketInfo {
            condition_id: format!("btc-5m-{}", next_start),
            token_id_yes: format!("yes-{}", next_start),
            token_id_no: format!("no-{}", next_start),
            close_time_ms: next_end,
            open_time_ms: next_start,
            status: MarketStatus::Open,
        };
        
        info!("🔄 Prepared next market: {}", next_market.token_id_yes);
        *self.next_market.try_write().unwrap() = Some(next_market);
        Ok(())
    }
    
    /// Execute market rollover (at close time)
    pub async fn rollover(&self) -> Result<(), Box<dyn std::error::Error>> {
        let next = {
            let mut next_guard = self.next_market.try_write().unwrap();
            next_guard.take()
        };
        
        if let Some(next_market) = next {
            info!("🔄 Rolling over to market: {}", next_market.token_id_yes);
            
            // Update current market
            *self.current_market.try_write().unwrap() = next_market;
            
            // Reset for next cycle
            *self.next_market.try_write().unwrap() = None;
            
            Ok(())
        } else {
            warn!("⚠️  Rollover triggered but next market not prepared");
            Err("Next market not prepared".into())
        }
    }
    
    /// Get current market status
    pub async fn get_status(&self) -> MarketStatus {
        self.current_market.try_read().unwrap().status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_router_initialization() {
        let config = SystemConfig::load().unwrap();
        let router = MarketRouter::new(config).await.unwrap();
        
        assert!(!router.get_current_market_id().is_empty());
        assert!(router.get_seconds_to_rollover() < 300);
    }
    
    #[tokio::test]
    async fn test_decision_window_detection() {
        let config = SystemConfig::load().unwrap();
        let router = MarketRouter::new(config).await.unwrap();
        
        // This test depends on actual time, so we just verify it doesn't panic
        let _in_window = router.is_in_decision_window();
        let _in_blackout = router.is_in_blackout_window();
    }
    
    #[tokio::test]
    async fn test_market_preparation() {
        let config = SystemConfig::load().unwrap();
        let router = MarketRouter::new(config).await.unwrap();
        
        // Force preparation
        router.prepare_next_market().await.unwrap();
        
        assert!(router.next_market.try_read().unwrap().is_some());
    }
}
