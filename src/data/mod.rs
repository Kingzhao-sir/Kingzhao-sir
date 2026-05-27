//! Data ingestion module - market data gateway and routing
//! 
//! Handles:
//! - 5-minute market contract rotation (pre-warming next cycle)
//! - WebSocket connections to Binance and Polymarket
//! - Zero-copy tick data processing

pub mod gateway;
pub mod market_router;
pub mod order_book;

pub use gateway::DataGateway;
pub use market_router::MarketRouter;
pub use order_book::OrderBookManager;
