//! Data Gateway module - handles market data ingestion

pub mod market_router;

pub use market_router::{MarketRouter, MarketRouterConfig, OperationalState};
