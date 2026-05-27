//! Market Router - Dynamic market rotation for 5-minute Polymarket contracts
//! 
//! Handles the critical task of seamlessly switching between expiring and new markets,
//! with pre-heating mechanism to ensure zero downtime.

use crate::core::{MarketInfo, MarketState, TimestampUs, now_us};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, warn, error};

/// Callback type for market state changes
pub type MarketStateCallback = Box<dyn Fn(&MarketInfo) + Send + Sync>;

/// Market Router configuration
#[derive(Debug, Clone)]
pub struct MarketRouterConfig {
    /// How many seconds before market close to start preparing next market
    pub预热_seconds: u32,
    /// How many seconds before close to stop opening new positions
    pub cooling_period_seconds: u32,
    /// Decision window start (seconds before close)
    pub decision_window_seconds: u32,
}

impl Default for MarketRouterConfig {
    fn default() -> Self {
        Self {
            预热_seconds: 60,      // Start preparing 60s before close
            cooling_period_seconds: 5,  // No new positions in last 5s
            decision_window_seconds: 30, // Decision window starts 30s before close
        }
    }
}

/// Market Router - manages lifecycle of 5-minute prediction markets
pub struct MarketRouter {
    config: MarketRouterConfig,
    current_market: Arc<RwLock<Option<MarketInfo>>>,
    next_market: Arc<RwLock<Option<MarketInfo>>>,
    state_callbacks: Vec<Arc<MarketStateCallback>>,
}

impl MarketRouter {
    pub fn new(config: MarketRouterConfig) -> Self {
        Self {
            config,
            current_market: Arc::new(RwLock::new(None)),
            next_market: Arc::new(RwLock::new(None)),
            state_callbacks: Vec::new(),
        }
    }

    /// Register a callback for market state changes
    pub fn register_state_callback(&mut self, callback: MarketStateCallback) {
        self.state_callbacks.push(Arc::new(callback));
    }

    /// Update the current market information
    pub fn update_current_market(&self, market: MarketInfo) {
        let mut current = self.current_market.write();
        *current = Some(market);
        
        if let Some(ref m) = *current {
            self.notify_state_change(m);
        }
    }

    /// Pre-load the next market (called during pre-heat phase)
    pub fn preload_next_market(&self, market: MarketInfo) {
        let mut next = self.next_market.write();
        *next = Some(market);
        info!("Pre-loaded next market: {:?}", next.as_ref().unwrap().market_id);
    }

    /// Get the current active market
    pub fn get_current_market(&self) -> Option<MarketInfo> {
        self.current_market.read().clone()
    }

    /// Get the next market (if pre-loaded)
    pub fn get_next_market(&self) -> Option<MarketInfo> {
        self.next_market.read().clone()
    }

    /// Check if we should switch to the next market
    pub fn should_switch_market(&self) -> bool {
        let current = match self.get_current_market() {
            Some(m) => m,
            None => return false,
        };

        let next = match self.get_next_market() {
            Some(m) => m,
            None => return false,
        };

        // Switch if current market is closed or in cooling period and next market is open
        if current.is_in_cooling_period() && next.state == MarketState::Open {
            return true;
        }

        false
    }

    /// Execute the market switch
    pub fn switch_to_next_market(&self) -> Result<(), String> {
        let next_market = self.get_next_market()
            .ok_or("No next market available")?;

        let mut current = self.current_market.write();
        let mut next = self.next_market.write();

        info!(
            "Switching from market {} to {}",
            current.as_ref().map(|m| &m.market_id).unwrap_or(&"none".to_string()),
            next_market.market_id
        );

        *current = Some(next_market.clone());
        *next = None;

        self.notify_state_change(&next_market);

        Ok(())
    }

    /// Get the operational state based on current time
    pub fn get_operational_state(&self) -> OperationalState {
        let current = match self.get_current_market() {
            Some(m) => m,
            None => return OperationalState::NoMarket,
        };

        let remaining = current.remaining_seconds();

        if remaining <= 0.0 {
            OperationalState::MarketClosed
        } else if remaining <= self.config.cooling_period_seconds as f64 {
            OperationalState::CoolingPeriod
        } else if remaining <= self.config.decision_window_seconds as f64 {
            OperationalState::DecisionWindow
        } else if remaining <= self.config.预热_seconds as f64 {
            OperationalState::PreHeat
        } else {
            OperationalState::NormalTrading
        }
    }

    /// Check if we can open new positions
    pub fn can_open_positions(&self) -> bool {
        matches!(
            self.get_operational_state(),
            OperationalState::NormalTrading | OperationalState::PreHeat | OperationalState::DecisionWindow
        )
    }

    /// Notify all registered callbacks of state change
    fn notify_state_change(&self, market: &MarketInfo) {
        for callback in &self.state_callbacks {
            callback(market);
        }
    }

    /// Get time remaining in current market
    pub fn time_remaining(&self) -> Option<f64> {
        self.get_current_market().map(|m| m.remaining_seconds())
    }
}

/// Current operational state of the trading system
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationalState {
    NormalTrading,
    PreHeat,
    DecisionWindow,
    CoolingPeriod,
    MarketClosed,
    NoMarket,
}

impl OperationalState {
    pub fn as_str(&self) -> &'static str {
        match self {
            OperationalState::NormalTrading => "normal_trading",
            OperationalState::PreHeat => "pre_heat",
            OperationalState::DecisionWindow => "decision_window",
            OperationalState::CoolingPeriod => "cooling_period",
            OperationalState::MarketClosed => "market_closed",
            OperationalState::NoMarket => "no_market",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_market(offset_seconds: f64) -> MarketInfo {
        let now = now_us();
        let offset_us = (offset_seconds * 1_000_000.0) as u64;
        
        MarketInfo {
            market_id: format!("test-market-{}", offset_seconds),
            token_id_yes: "yes".to_string(),
            token_id_no: "no".to_string(),
            condition_id: "cond".to_string(),
            question: "Test Question".to_string(),
            start_time_us: now - 300_000_000, // Started 5 min ago
            end_time_us: now + offset_us,
            state: MarketState::Open,
            implied_probability_yes: 0.55,
            volume_24h: rust_decimal::Decimal::from(10000),
            liquidity_yes: rust_decimal::Decimal::from(5000),
            liquidity_no: rust_decimal::Decimal::from(5000),
        }
    }

    #[test]
    fn test_router_initialization() {
        let config = MarketRouterConfig::default();
        let router = MarketRouter::new(config);
        
        assert_eq!(router.get_current_market(), None);
        assert_eq!(router.get_next_market(), None);
        assert_eq!(router.get_operational_state(), OperationalState::NoMarket);
    }

    #[test]
    fn test_market_update() {
        let config = MarketRouterConfig::default();
        let router = MarketRouter::new(config);
        
        let market = create_test_market(120.0); // 2 minutes remaining
        router.update_current_market(market.clone());
        
        assert_eq!(router.get_current_market().unwrap().market_id, market.market_id);
        assert_eq!(router.get_operational_state(), OperationalState::NormalTrading);
        assert!(router.can_open_positions());
    }

    #[test]
    fn test_decision_window_detection() {
        let config = MarketRouterConfig::default();
        let router = MarketRouter::new(config);
        
        // 25 seconds remaining - should be in decision window
        let market = create_test_market(25.0);
        router.update_current_market(market);
        
        assert_eq!(router.get_operational_state(), OperationalState::DecisionWindow);
        assert!(router.can_open_positions());
    }

    #[test]
    fn test_cooling_period_detection() {
        let config = MarketRouterConfig::default();
        let router = MarketRouter::new(config);
        
        // 3 seconds remaining - should be in cooling period
        let market = create_test_market(3.0);
        router.update_current_market(market);
        
        assert_eq!(router.get_operational_state(), OperationalState::CoolingPeriod);
        assert!(!router.can_open_positions());
    }

    #[test]
    fn test_pre_heat_state() {
        let config = MarketRouterConfig::default();
        let router = MarketRouter::new(config);
        
        // 45 seconds remaining - should be in pre-heat (between 60s and 30s)
        let market = create_test_market(45.0);
        router.update_current_market(market);
        
        assert_eq!(router.get_operational_state(), OperationalState::PreHeat);
        assert!(router.can_open_positions());
    }

    #[test]
    fn test_market_switching() {
        let config = MarketRouterConfig::default();
        let router = MarketRouter::new(config);
        
        // Set up current market (about to close)
        let current = create_test_market(3.0);
        router.update_current_market(current);
        
        // Pre-load next market
        let next = create_test_market(300.0); // 5 minutes remaining
        router.preload_next_market(next.clone());
        
        // Should trigger switch
        assert!(router.should_switch_market());
        
        // Execute switch
        router.switch_to_next_market().unwrap();
        
        // Verify switch
        assert_eq!(router.get_current_market().unwrap().market_id, next.market_id);
        assert_eq!(router.get_next_market(), None);
    }

    #[test]
    fn test_state_callback() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        
        let config = MarketRouterConfig::default();
        let mut router = MarketRouter::new(config);
        
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();
        
        router.register_state_callback(Box::new(move |_| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        }));
        
        let market = create_test_market(120.0);
        router.update_current_market(market);
        
        // Wait a bit for async processing if needed
        std::thread::sleep(std::time::Duration::from_millis(10));
        
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_time_remaining() {
        let config = MarketRouterConfig::default();
        let router = MarketRouter::new(config);
        
        let market = create_test_market(45.5);
        router.update_current_market(market);
        
        let remaining = router.time_remaining().unwrap();
        assert!(remaining > 44.0 && remaining < 47.0); // Allow some timing variance
    }

    #[test]
    fn test_no_position_during_cooling() {
        let config = MarketRouterConfig::default();
        let router = MarketRouter::new(config);
        
        // Test multiple times to ensure consistency
        for _ in 0..10 {
            let market = create_test_market(2.0);
            router.update_current_market(market);
            assert!(!router.can_open_positions());
        }
    }
}
