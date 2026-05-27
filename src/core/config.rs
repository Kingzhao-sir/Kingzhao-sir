//! Configuration management
//! 
//! Loads trading parameters from environment or config file

use crate::types::{Config, RiskLimits};

impl Config {
    /// Load configuration with validation
    pub fn load() -> Result<Self, ConfigError> {
        let config = Self {
            polymarket_api_key: std::env::var("POLYMARKET_API_KEY")
                .map_err(|_| ConfigError::MissingApiKey)?,
            polymarket_secret: std::env::var("POLYMARKET_SECRET")
                .map_err(|_| ConfigError::MissingSecret)?,
            binance_ws_url: std::env::var("BINANCE_WS_URL")
                .unwrap_or_else(|_| "wss://stream.binance.com:9443/ws".to_string()),
            polymarket_ws_url: std::env::var("POLYMARKET_WS_URL")
                .unwrap_or_else(|_| "wss://polymarket.com/ws".to_string()),
            risk_limits: RiskLimits::default(),
            strategy_weights: [0.3, 0.3, 0.25, 0.15],
            decision_window_seconds: 30,
            blackout_window_seconds: 5,
        };
        
        config.validate()?;
        Ok(config)
    }
    
    /// Validate configuration parameters
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.polymarket_api_key.is_empty() {
            return Err(ConfigError::InvalidApiKey);
        }
        if self.polymarket_secret.is_empty() {
            return Err(ConfigError::InvalidSecret);
        }
        if self.decision_window_seconds < 10 || self.decision_window_seconds > 60 {
            return Err(ConfigError::InvalidDecisionWindow);
        }
        if self.blackout_window_seconds >= self.decision_window_seconds {
            return Err(ConfigError::InvalidBlackoutWindow);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing Polymarket API key")]
    MissingApiKey,
    #[error("Missing Polymarket secret")]
    MissingSecret,
    #[error("Invalid API key format")]
    InvalidApiKey,
    #[error("Invalid secret format")]
    InvalidSecret,
    #[error("Decision window must be between 10-60 seconds")]
    InvalidDecisionWindow,
    #[error("Blackout window must be less than decision window")]
    InvalidBlackoutWindow,
}
