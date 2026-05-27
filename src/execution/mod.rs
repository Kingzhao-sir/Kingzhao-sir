//! Order Management System (OMS) and Execution Engine
//! 
//! Handles order lifecycle management, risk checks, and Polymarket CLOB execution.

use crate::core::{Order, OrderStatus, Side, ExecutionReport, Position, TimestampUs, now_us};
use rust_decimal::Decimal;
use std::collections::HashMap;
use uuid::Uuid;
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{info, warn, error};

/// Risk management configuration
#[derive(Debug, Clone)]
pub struct RiskConfig {
    /// Maximum position size per market (in shares)
    pub max_position_size: Decimal,
    /// Maximum daily loss (in USDC)
    pub max_daily_loss: Decimal,
    /// Maximum single order size (in shares)
    pub max_order_size: Decimal,
    /// Maximum price deviation from mid-price (in basis points)
    pub max_price_deviation_bps: u32,
    /// Maximum orders per second
    pub max_orders_per_second: u32,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_position_size: Decimal::from(1000),
            max_daily_loss: Decimal::from(500),
            max_order_size: Decimal::from(200),
            max_price_deviation_bps: 500, // 5%
            max_orders_per_second: 10,
        }
    }
}

/// OMS Configuration
#[derive(Debug, Clone)]
pub struct OmsConfig {
    /// Risk configuration
    pub risk: RiskConfig,
    /// Enable paper trading mode
    pub paper_trading: bool,
    /// Polymarket API key (if not paper trading)
    pub api_key: Option<String>,
    /// Polymarket private key for signing
    pub private_key: Option<String>,
}

impl Default for OmsConfig {
    fn default() -> Self {
        Self {
            risk: RiskConfig::default(),
            paper_trading: true,
            api_key: None,
            private_key: None,
        }
    }
}

/// Rate limiter for order submission
struct RateLimiter {
    max_per_second: u32,
    tokens: u32,
    last_refill: TimestampUs,
}

impl RateLimiter {
    fn new(max_per_second: u32) -> Self {
        Self {
            max_per_second,
            tokens: max_per_second,
            last_refill: now_us(),
        }
    }

    fn try_acquire(&mut self) -> bool {
        let now = now_us();
        let elapsed_seconds = (now - self.last_refill) as f64 / 1_000_000.0;
        
        // Refill tokens based on elapsed time
        let refill = (elapsed_seconds * self.max_per_second as f64) as u32;
        self.tokens = (self.tokens + refill).min(self.max_per_second);
        self.last_refill = now;
        
        if self.tokens > 0 {
            self.tokens -= 1;
            true
        } else {
            false
        }
    }
}

/// Order Management System
pub struct OrderManagementSystem {
    config: OmsConfig,
    /// Active orders by order_id
    orders: Arc<RwLock<HashMap<Uuid, Order>>>,
    /// Positions by market_id
    positions: Arc<RwLock<HashMap<String, Position>>>,
    /// Rate limiter
    rate_limiter: Arc<RwLock<RateLimiter>>,
    /// Daily PnL tracking
    daily_pnl: Arc<RwLock<Decimal>>,
    /// Callback for order updates
    on_order_update: Option<Box<dyn Fn(&ExecutionReport) + Send + Sync>>,
}

impl OrderManagementSystem {
    pub fn new(config: OmsConfig) -> Self {
        Self {
            config,
            orders: Arc::new(RwLock::new(HashMap::with_capacity(100))),
            positions: Arc::new(RwLock::new(HashMap::with_capacity(20))),
            rate_limiter: Arc::new(RwLock::new(RateLimiter::new(config.risk.max_orders_per_second))),
            daily_pnl: Arc::new(RwLock::new(Decimal::ZERO)),
            on_order_update: None,
        }
    }

    /// Set callback for order updates
    pub fn set_order_update_callback<F>(&mut self, callback: F)
    where
        F: Fn(&ExecutionReport) + Send + Sync + 'static,
    {
        self.on_order_update = Some(Box::new(callback));
    }

    /// Pre-trade risk check
    pub fn pre_trade_check(&self, order: &Order, mid_price: Decimal) -> Result<(), String> {
        // Check rate limit
        if !self.rate_limiter.read().try_acquire() {
            return Err("Rate limit exceeded".to_string());
        }

        // Check order size
        if order.quantity > self.config.risk.max_order_size {
            return Err(format!(
                "Order size {} exceeds max {}",
                order.quantity, self.config.risk.max_order_size
            ));
        }

        // Check price deviation
        if let Some(deviation_bps) = self.calculate_price_deviation_bps(order.price, mid_price) {
            if deviation_bps > self.config.risk.max_price_deviation_bps {
                return Err(format!(
                    "Price deviation {} bps exceeds max {}",
                    deviation_bps, self.config.risk.max_price_deviation_bps
                ));
            }
        }

        // Check position limit
        let positions = self.positions.read();
        if let Some(pos) = positions.get(&order.market_id) {
            let current_exposure = pos.net_exposure();
            let new_exposure = match order.side {
                Side::Yes => current_exposure + order.quantity,
                Side::No => current_exposure - order.quantity,
            };
            
            if new_exposure.abs() > self.config.risk.max_position_size {
                return Err(format!(
                    "Position {} would exceed max {}",
                    new_exposure, self.config.risk.max_position_size
                ));
            }
        }

        // Check daily loss limit
        let daily_pnl = *self.daily_pnl.read();
        if daily_pnl < -self.config.risk.max_daily_loss {
            return Err(format!(
                "Daily loss {} exceeds limit {}",
                daily_pnl, self.config.risk.max_daily_loss
            ));
        }

        Ok(())
    }

    /// Submit a new order
    pub fn submit_order(&self, mut order: Order, mid_price: Decimal) -> Result<Uuid, String> {
        // Pre-trade risk check
        self.pre_trade_check(&order, mid_price)?;

        // Set initial status
        order.status = OrderStatus::New;
        let order_id = order.order_id;

        // Store order
        self.orders.write().insert(order_id, order);

        info!("Order submitted: {:?}", order_id);

        // In paper trading mode, simulate immediate fill at mid-price
        if self.config.paper_trading {
            self.simulate_fill(order_id, mid_price);
        } else {
            // TODO: Implement real Polymarket CLOB submission
            warn!("Real trading not yet implemented");
        }

        Ok(order_id)
    }

    /// Cancel an order
    pub fn cancel_order(&self, order_id: Uuid) -> Result<(), String> {
        let mut orders = self.orders.write();
        
        match orders.get_mut(&order_id) {
            Some(order) => {
                if !order.is_active() {
                    return Err("Order is not active".to_string());
                }
                order.status = OrderStatus::Cancelled;
                order.updated_at_us = now_us();
                info!("Order cancelled: {:?}", order_id);
                Ok(())
            }
            None => Err("Order not found".to_string()),
        }
    }

    /// Cancel all orders for a market
    pub fn cancel_all_orders(&self, market_id: &str) -> usize {
        let mut orders = self.orders.write();
        let mut cancelled_count = 0;

        for order in orders.values_mut() {
            if order.market_id == market_id && order.is_active() {
                order.status = OrderStatus::Cancelled;
                order.updated_at_us = now_us();
                cancelled_count += 1;
            }
        }

        info!("Cancelled {} orders for market {}", cancelled_count, market_id);
        cancelled_count
    }

    /// Get order by ID
    pub fn get_order(&self, order_id: Uuid) -> Option<Order> {
        self.orders.read().get(&order_id).cloned()
    }

    /// Get all active orders
    pub fn get_active_orders(&self) -> Vec<Order> {
        self.orders
            .read()
            .values()
            .filter(|o| o.is_active())
            .cloned()
            .collect()
    }

    /// Get position for a market
    pub fn get_position(&self, market_id: &str) -> Option<Position> {
        self.positions.read().get(market_id).cloned()
    }

    /// Update position after fill
    fn update_position(&self, report: &ExecutionReport) {
        let mut positions = self.positions.write();
        
        let position = positions.entry(report.market_id.clone()).or_insert_with(|| {
            Position::new(report.market_id.clone())
        });

        match report.side {
            Side::Yes => {
                let total_cost = report.price * report.quantity;
                if position.yes_shares.is_zero() {
                    position.avg_entry_yes = report.price;
                } else {
                    let total_shares = position.yes_shares + report.quantity;
                    position.avg_entry_yes = (position.avg_entry_yes * position.yes_shares + total_cost) / total_shares;
                }
                position.yes_shares += report.quantity;
            }
            Side::No => {
                let total_cost = report.price * report.quantity;
                if position.no_shares.is_zero() {
                    position.avg_entry_no = report.price;
                } else {
                    let total_shares = position.no_shares + report.quantity;
                    position.avg_entry_no = (position.avg_entry_no * position.no_shares + total_cost) / total_shares;
                }
                position.no_shares += report.quantity;
            }
        }

        position.last_update_us = now_us();
    }

    /// Simulate order fill (for paper trading)
    fn simulate_fill(&self, order_id: Uuid, fill_price: Decimal) {
        let mut orders = self.orders.write();
        
        if let Some(order) = orders.get_mut(&order_id) {
            order.fill(order.quantity); // Fill entire order
            
            let report = ExecutionReport {
                order_id: order.order_id,
                market_id: order.market_id.clone(),
                side: order.side,
                price: fill_price,
                quantity: order.quantity,
                fee: fill_price * order.quantity * Decimal::from_f64_retain(0.02).unwrap_or(Decimal::ZERO), // 2% fee
                is_maker: order.post_only,
                timestamp_us: now_us(),
            };

            // Update position
            drop(orders);
            self.update_position(&report);

            // Notify callback
            if let Some(ref callback) = self.on_order_update {
                callback(&report);
            }

            info!("Order filled: {:?} @ {}", order_id, fill_price);
        }
    }

    /// Calculate price deviation in basis points
    fn calculate_price_deviation_bps(&self, order_price: Decimal, mid_price: Decimal) -> Option<u32> {
        if mid_price.is_zero() {
            return None;
        }
        
        let deviation = (order_price - mid_price).abs();
        let deviation_bps = (deviation / mid_price) * Decimal::from(10000);
        deviation_bps.to_u32()
    }

    /// Update daily PnL
    pub fn update_daily_pnl(&self, pnl: Decimal) {
        let mut daily = self.daily_pnl.write();
        *daily += pnl;
    }

    /// Reset daily PnL (call at start of each day)
    pub fn reset_daily_pnl(&self) {
        let mut daily = self.daily_pnl.write();
        *daily = Decimal::ZERO;
    }

    /// Get current daily PnL
    pub fn get_daily_pnl(&self) -> Decimal {
        *self.daily_pnl.read()
    }

    /// Check if system is in paper trading mode
    pub fn is_paper_trading(&self) -> bool {
        self.config.paper_trading
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_order(side: Side, price: Decimal, qty: Decimal) -> Order {
        Order::new_limit(
            "test-market".to_string(),
            side,
            price,
            qty,
            false,
        )
    }

    #[test]
    fn test_oms_initialization() {
        let config = OmsConfig::default();
        let oms = OrderManagementSystem::new(config);
        
        assert!(oms.is_paper_trading());
        assert_eq!(oms.get_active_orders().len(), 0);
        assert_eq!(oms.get_daily_pnl(), Decimal::ZERO);
    }

    #[test]
    fn test_submit_order_success() {
        let config = OmsConfig::default();
        let oms = OrderManagementSystem::new(config);
        
        let order = create_test_order(Side::Yes, Decimal::from(50), Decimal::from(100));
        let mid_price = Decimal::from(50);
        
        let result = oms.submit_order(order, mid_price);
        assert!(result.is_ok());
        
        let order_id = result.unwrap();
        let retrieved = oms.get_order(order_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().status, OrderStatus::Filled); // Paper trading fills immediately
    }

    #[test]
    fn test_pre_trade_order_size_limit() {
        let mut config = OmsConfig::default();
        config.risk.max_order_size = Decimal::from(50);
        
        let oms = OrderManagementSystem::new(config);
        
        // Try to submit order larger than limit
        let order = create_test_order(Side::Yes, Decimal::from(50), Decimal::from(100));
        let mid_price = Decimal::from(50);
        
        let result = oms.submit_order(order, mid_price);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds max"));
    }

    #[test]
    fn test_pre_trade_price_deviation() {
        let mut config = OmsConfig::default();
        config.risk.max_price_deviation_bps = 100; // 1%
        
        let oms = OrderManagementSystem::new(config);
        
        // Try to submit order with excessive price deviation
        let order = create_test_order(Side::Yes, Decimal::from(60), Decimal::from(50)); // 20% deviation
        let mid_price = Decimal::from(50);
        
        let result = oms.submit_order(order, mid_price);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("deviation"));
    }

    #[test]
    fn test_cancel_order() {
        let config = OmsConfig::default();
        let oms = OrderManagementSystem::new(config);
        
        // First disable paper trading to keep order active
        let mut config2 = OmsConfig::default();
        config2.paper_trading = false;
        let oms2 = OrderManagementSystem::new(config2);
        
        let order = create_test_order(Side::Yes, Decimal::from(50), Decimal::from(50));
        let mid_price = Decimal::from(50);
        
        let result = oms2.submit_order(order, mid_price);
        assert!(result.is_ok());
        
        let order_id = result.unwrap();
        
        // Cancel the order
        let cancel_result = oms2.cancel_order(order_id);
        assert!(cancel_result.is_ok());
        
        // Verify order is cancelled
        let retrieved = oms2.get_order(order_id).unwrap();
        assert_eq!(retrieved.status, OrderStatus::Cancelled);
    }

    #[test]
    fn test_position_tracking() {
        let config = OmsConfig::default();
        let oms = OrderManagementSystem::new(config);
        
        // Submit buy order
        let order1 = create_test_order(Side::Yes, Decimal::from(50), Decimal::from(100));
        oms.submit_order(order1, Decimal::from(50)).unwrap();
        
        // Check position
        let position = oms.get_position("test-market").unwrap();
        assert_eq!(position.yes_shares, Decimal::from(100));
        assert_eq!(position.no_shares, Decimal::ZERO);
        assert_eq!(position.avg_entry_yes, Decimal::from(50));
    }

    #[test]
    fn test_rate_limiting() {
        let mut config = OmsConfig::default();
        config.risk.max_orders_per_second = 2;
        
        let oms = OrderManagementSystem::new(config);
        let mid_price = Decimal::from(50);
        
        // Submit orders rapidly
        let mut success_count = 0;
        for _ in 0..5 {
            let order = create_test_order(Side::Yes, Decimal::from(50), Decimal::from(10));
            if oms.submit_order(order, mid_price).is_ok() {
                success_count += 1;
            }
        }
        
        // Should have limited to max_orders_per_second
        assert_eq!(success_count, 2);
    }

    #[test]
    fn test_daily_pnl_tracking() {
        let config = OmsConfig::default();
        let oms = OrderManagementSystem::new(config);
        
        assert_eq!(oms.get_daily_pnl(), Decimal::ZERO);
        
        oms.update_daily_pnl(Decimal::from(100));
        assert_eq!(oms.get_daily_pnl(), Decimal::from(100));
        
        oms.update_daily_pnl(Decimal::from(-50));
        assert_eq!(oms.get_daily_pnl(), Decimal::from(50));
        
        oms.reset_daily_pnl();
        assert_eq!(oms.get_daily_pnl(), Decimal::ZERO);
    }

    #[test]
    fn test_get_active_orders() {
        let mut config = OmsConfig::default();
        config.paper_trading = false; // Keep orders active
        
        let oms = OrderManagementSystem::new(config);
        let mid_price = Decimal::from(50);
        
        // Submit multiple orders
        for i in 0..3 {
            let order = create_test_order(Side::Yes, Decimal::from(50), Decimal::from(50));
            oms.submit_order(order, mid_price).unwrap();
        }
        
        assert_eq!(oms.get_active_orders().len(), 3);
        
        // Cancel one
        let active = oms.get_active_orders();
        if let Some(first) = active.first() {
            oms.cancel_order(first.order_id).unwrap();
        }
        
        assert_eq!(oms.get_active_orders().len(), 2);
    }
}
