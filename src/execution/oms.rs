//! Order Management System (OMS)
//! Handles order lifecycle, risk checks, and execution routing

use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::{info, warn, error, debug};

use crate::execution::types::{Order, OrderCommand, OrderAction, OrderSide, OrderStatus, Fill, Position};
use crate::utils::config::SystemConfig;
use crate::strategy::types::Signal;

/// Order Management System
pub struct OrderManagementSystem {
    order_sender: broadcast::Sender<OrderCommand>,
    config: SystemConfig,
    orders: HashMap<String, Order>,
    positions: HashMap<String, Position>,
    daily_pnl: f64,
}

impl OrderManagementSystem {
    pub fn new(order_sender: broadcast::Sender<OrderCommand>, config: SystemConfig) -> Self {
        Self {
            order_sender,
            config,
            orders: HashMap::new(),
            positions: HashMap::new(),
            daily_pnl: 0.0,
        }
    }
    
    /// Main entry point - runs the OMS loop
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("📦 Order Management System starting...");
        info!("   Max position size: ${}", self.config.max_position_size_usd);
        info!("   Max daily loss: ${}", self.config.max_daily_loss_usd);
        
        // In production, this would:
        // 1. Listen for OrderCommands from signal engine
        // 2. Apply pre-trade risk checks
        // 3. Route orders to Polymarket CLOB
        // 4. Track fills and update positions
        
        // Simulation for demonstration
        self.run_simulation().await?;
        
        Ok(())
    }
    
    /// Simulate OMS operation
    async fn run_simulation(&self) -> Result<(), Box<dyn std::error::Error>> {
        use tokio::time::{sleep, Duration};
        
        info!("📦 OMS simulation running...");
        
        // Simulate processing orders
        sleep(Duration::from_millis(50)).await;
        
        // Test pre-trade risk check
        assert!(self.pre_trade_risk_check(100.0));
        assert!(!self.pre_trade_risk_check(self.config.max_position_size_usd + 1000.0));
        
        info!("📦 OMS simulation complete");
        Ok(())
    }
    
    /// Pre-trade risk check
    #[inline]
    pub fn pre_trade_risk_check(&self, quantity_usd: f64) -> bool {
        // Check 1: Position size limit
        if quantity_usd > self.config.max_position_size_usd {
            warn!("⚠️  Order exceeds max position size");
            return false;
        }
        
        // Check 2: Daily loss limit
        if self.daily_pnl < -self.config.max_daily_loss_usd {
            warn!("⚠️  Daily loss limit reached");
            return false;
        }
        
        // Check 3: Slippage check (would compare to fair value in production)
        
        true
    }
    
    /// Convert signal to order command
    pub fn signal_to_order_command(&self, signal: &Signal, market_id: String) -> OrderCommand {
        use chrono::Utc;
        
        OrderCommand {
            command_id: format!("cmd_{}", Utc::now().timestamp_millis()),
            action: OrderAction::Open,
            market_id,
            side: signal.direction.into(),
            quantity_usd: self.config.max_position_size_usd * signal.confidence,
            limit_price: None,
            signal_confidence: signal.confidence,
        }
    }
    
    /// Create new order from command
    pub fn create_order(&mut self, cmd: &OrderCommand) -> Order {
        let mut order = Order::new(
            cmd.market_id.clone(),
            cmd.side,
            cmd.quantity_usd,
        );
        
        if let Some(limit_price) = cmd.limit_price {
            order.price = Some(limit_price);
            order.order_type = crate::execution::types::OrderType::Limit;
        }
        
        order.status = OrderStatus::Submitted;
        self.orders.insert(order.order_id.clone(), order.clone());
        
        order
    }
    
    /// Process fill report
    pub fn process_fill(&mut self, fill: &Fill) {
        if let Some(order) = self.orders.get_mut(&fill.order_id) {
            order.filled_quantity += fill.quantity;
            order.avg_fill_price = Some(fill.price);
            
            if order.filled_quantity >= order.quantity_usd {
                order.status = OrderStatus::Filled;
            } else {
                order.status = OrderStatus::PartiallyFilled;
            }
            
            order.updated_at_ms = fill.timestamp_ms;
            
            // Update position
            self.update_position_from_fill(order, fill);
            
            info!("✅ Fill processed: {} @ ${}", fill.quantity, fill.price);
        }
    }
    
    /// Update position from fill
    fn update_position_from_fill(&mut self, order: &Order, fill: &Fill) {
        let position = self.positions.entry(order.market_id.clone()).or_insert_with(|| {
            Position::new(
                order.market_id.clone(),
                order.side,
                fill.quantity,
                fill.price,
            )
        });
        
        position.quantity_shares += fill.quantity;
        position.avg_entry_price = fill.price;
        position.current_value = position.quantity_shares * fill.price;
    }
    
    /// Get current position for market
    pub fn get_position(&self, market_id: &str) -> Option<&Position> {
        self.positions.get(market_id)
    }
    
    /// Get active orders count
    pub fn active_orders_count(&self) -> usize {
        self.orders.values().filter(|o| o.is_active()).count()
    }
    
    /// Cancel all open orders (for market rollover)
    pub fn cancel_all_orders(&mut self) -> Vec<String> {
        let mut cancelled = Vec::new();
        
        for order in self.orders.values_mut() {
            if order.is_active() {
                order.status = OrderStatus::Cancelled;
                cancelled.push(order.order_id.clone());
            }
        }
        
        info!("❌ Cancelled {} orders", cancelled.len());
        cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;
    
    #[tokio::test]
    async fn test_oms_creation() {
        let (tx, _rx) = broadcast::channel::<OrderCommand>(100);
        let config = SystemConfig::load().unwrap();
        
        let oms = OrderManagementSystem::new(tx, config);
        
        assert_eq!(oms.active_orders_count(), 0);
        assert_eq!(oms.daily_pnl, 0.0);
    }
    
    #[test]
    fn test_pre_trade_risk_check() {
        let (tx, _rx) = broadcast::channel::<OrderCommand>(100);
        let config = SystemConfig::load().unwrap();
        
        let oms = OrderManagementSystem::new(tx, config);
        
        // Small order should pass
        assert!(oms.pre_trade_risk_check(100.0));
        
        // Large order should fail
        assert!(!oms.pre_trade_risk_check(2000.0));
    }
    
    #[test]
    fn test_order_creation() {
        let (tx, _rx) = broadcast::channel::<OrderCommand>(100);
        let config = SystemConfig::load().unwrap();
        
        let mut oms = OrderManagementSystem::new(tx, config);
        
        let cmd = OrderCommand {
            command_id: "test_cmd".to_string(),
            action: OrderAction::Open,
            market_id: "test_market".to_string(),
            side: OrderSide::BuyYes,
            quantity_usd: 100.0,
            limit_price: None,
            signal_confidence: 0.8,
        };
        
        let order = oms.create_order(&cmd);
        assert_eq!(order.quantity_usd, 100.0);
        assert_eq!(order.status, OrderStatus::Submitted);
    }
    
    #[test]
    fn test_signal_to_order_conversion() {
        use crate::strategy::types::{SignalDirection, SignalFactors};
        
        let (tx, _rx) = broadcast::channel::<OrderCommand>(100);
        let config = SystemConfig::load().unwrap();
        
        let oms = OrderManagementSystem::new(tx, config.clone());
        
        let signal = Signal {
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            direction: SignalDirection::Long,
            confidence: 0.75,
            target_price: 0.55,
            stop_loss: None,
            factors: SignalFactors {
                support_rate_score: 0.6,
                kline_1m_score: 0.5,
                kline_5m_score: 0.4,
                orderbook_score: 0.3,
                cross_exchange_score: 0.2,
            },
        };
        
        let cmd = oms.signal_to_order_command(&signal, "test_market".to_string());
        
        assert_eq!(cmd.action, OrderAction::Open);
        assert_eq!(cmd.side, OrderSide::BuyYes);
        // Quantity scaled by confidence
        assert!(cmd.quantity_usd <= config.max_position_size_usd);
    }
}
