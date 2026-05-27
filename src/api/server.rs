//! API Server - HTTP and WebSocket endpoints for monitoring and control

use axum::{
    Router,
    routing::get,
    extract::State,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    // Will be populated with references to core components when integrated
    pub system_status: Arc<tokio::sync::RwLock<SystemStatus>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemStatus {
    pub is_running: bool,
    pub current_market_id: Option<String>,
    pub operational_state: String,
    pub paper_trading: bool,
    pub total_trades: u32,
    pub daily_pnl: f64,
    pub last_signal_time: Option<u64>,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: u64,
    pub version: String,
}

/// Market status response
#[derive(Debug, Serialize)]
pub struct MarketStatusResponse {
    pub market_id: Option<String>,
    pub state: String,
    pub time_remaining_sec: f64,
    pub can_trade: bool,
}

/// Create the API router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/status", get(get_system_status))
        .route("/api/market", get(get_market_status))
        .route("/api/trades", get(get_trades))
        .with_state(state)
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    let response = HealthResponse {
        status: "healthy".to_string(),
        timestamp: crate::core::now_us(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    Json(response)
}

/// Get system status
async fn get_system_status(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.system_status.read().await;
    Json(status.clone())
}

/// Get current market status
async fn get_market_status(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.system_status.read().await;
    
    let response = MarketStatusResponse {
        market_id: status.current_market_id.clone(),
        state: status.operational_state.clone(),
        time_remaining_sec: 0.0, // Would be calculated from real market data
        can_trade: status.is_running && !status.operational_state.contains("cooling"),
    };
    
    Json(response)
}

/// Get trade history
async fn get_trades() -> impl IntoResponse {
    // Placeholder - would return actual trade history
    Json(serde_json::json!({
        "trades": [],
        "total": 0
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_status_default() {
        let status = SystemStatus::default();
        
        assert!(!status.is_running);
        assert!(status.current_market_id.is_none());
        assert_eq!(status.total_trades, 0);
        assert!((status.daily_pnl - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_health_response() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            timestamp: 1234567890,
            version: "0.1.0".to_string(),
        };
        
        assert_eq!(response.status, "healthy");
        assert_eq!(response.version, "0.1.0");
    }
}
