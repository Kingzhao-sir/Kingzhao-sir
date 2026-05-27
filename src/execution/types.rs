//! Execution types for orders and fills

use serde::{Deserialize, Serialize};
use crate::strategy::types::SignalDirection;

/// Order side (Buy Yes / Buy No)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    BuyYes,  // Bet on UP
    BuyNo,   // Bet on DOWN
    SellYes, // Close UP position
    SellNo,  // Close DOWN position
}

impl From<SignalDirection> for OrderSide {
    fn from(direction: SignalDirection) -> Self {
        match direction {
            SignalDirection::Long => OrderSide::BuyYes,
            SignalDirection::Short => OrderSide::BuyNo,
            SignalDirection::Neutral => OrderSide::BuyYes, // Default
        }
    }
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    PostOnly, // Maker-only for rebates
}

/// Order status in lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    Pending,
    Submitted,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

/// Trading order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id: String,
    pub client_order_id: String,
    pub market_id: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub price: Option<f64>,
    pub quantity_usd: f64,
    pub status: OrderStatus,
    pub filled_quantity: f64,
    pub avg_fill_price: Option<f64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl Order {
    pub fn new(market_id: String, side: OrderSide, quantity_usd: f64) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            order_id: format!("ord_{}", now),
            client_order_id: format!("client_{}_{}", market_id, now),
            market_id,
            side,
            order_type: OrderType::Market,
            price: None,
            quantity_usd,
            status: OrderStatus::Pending,
            filled_quantity: 0.0,
            avg_fill_price: None,
            created_at_ms: now,
            updated_at_ms: now,
        }
    }
    
    #[inline]
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::Pending | OrderStatus::Submitted | OrderStatus::PartiallyFilled
        )
    }
}

/// Order fill execution report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub fill_id: String,
    pub order_id: String,
    pub price: f64,
    pub quantity: f64,
    pub fee_usd: f64,
    pub timestamp_ms: i64,
}

/// Order command from signal engine to OMS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCommand {
    pub command_id: String,
    pub action: OrderAction,
    pub market_id: String,
    pub side: OrderSide,
    pub quantity_usd: f64,
    pub limit_price: Option<f64>,
    pub signal_confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderAction {
    Open,
    Close,
    Cancel,
    CancelAll,
}

/// Position state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub market_id: String,
    pub side: OrderSide,
    pub quantity_shares: f64,
    pub avg_entry_price: f64,
    pub current_value: f64,
    pub unrealized_pnl: f64,
}

impl Position {
    pub fn new(market_id: String, side: OrderSide, shares: f64, entry_price: f64) -> Self {
        Self {
            market_id,
            side,
            quantity_shares: shares,
            avg_entry_price: entry_price,
            current_value: shares * entry_price,
            unrealized_pnl: 0.0,
        }
    }
}
