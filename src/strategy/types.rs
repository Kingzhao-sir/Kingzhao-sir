//! Strategy types for signals and decisions

use serde::{Deserialize, Serialize};

/// Trading signal direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalDirection {
    Long,  // Predict UP
    Short, // Predict DOWN
    Neutral,
}

/// Trading signal with confidence score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub timestamp_ms: i64,
    pub direction: SignalDirection,
    pub confidence: f64,      // 0.0 - 1.0
    pub target_price: f64,    // Expected closing price
    pub stop_loss: Option<f64>,
    pub factors: SignalFactors,
}

/// Individual factor scores contributing to signal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalFactors {
    pub support_rate_score: f64,    // Based on Polymarket implied probability
    pub kline_1m_score: f64,        // 1-minute candle pattern
    pub kline_5m_score: f64,        // 5-minute trend
    pub orderbook_score: f64,       // Order book imbalance
    pub cross_exchange_score: f64,  // Binance/OKX price divergence
}

impl SignalFactors {
    /// Calculate weighted composite score
    #[inline]
    pub fn composite_score(&self) -> f64 {
        // Weights sum to 1.0
        self.support_rate_score * 0.30 +
        self.kline_1m_score * 0.20 +
        self.kline_5m_score * 0.20 +
        self.orderbook_score * 0.20 +
        self.cross_exchange_score * 0.10
    }
}

/// Backtest result for a single trade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestTrade {
    pub entry_time_ms: i64,
    pub exit_time_ms: i64,
    pub direction: SignalDirection,
    pub entry_price: f64,
    pub exit_price: f64,
    pub pnl_usd: f64,
    pub pnl_percent: f64,
    pub signal_confidence: f64,
}

/// Aggregated backtest report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestReport {
    pub total_trades: u32,
    pub winning_trades: u32,
    pub losing_trades: u32,
    pub win_rate: f64,
    pub total_pnl_usd: f64,
    pub avg_pnl_per_trade: f64,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
    pub profit_factor: f64,
    pub trades: Vec<BacktestTrade>,
}

impl BacktestReport {
    pub fn new(trades: Vec<BacktestTrade>) -> Self {
        let total = trades.len() as u32;
        let winners = trades.iter().filter(|t| t.pnl_usd > 0.0).count() as u32;
        let losers = trades.iter().filter(|t| t.pnl_usd <= 0.0).count() as u32;
        
        let total_pnl: f64 = trades.iter().map(|t| t.pnl_usd).sum();
        let gross_profit: f64 = trades.iter().filter(|t| t.pnl_usd > 0.0).map(|t| t.pnl_usd).sum();
        let gross_loss: f64 = trades.iter().filter(|t| t.pnl_usd < 0.0).map(|t| t.pnl_usd.abs()).sum();
        
        let win_rate = if total > 0 { winners as f64 / total as f64 } else { 0.0 };
        let profit_factor = if gross_loss > 0.0 { gross_profit / gross_loss } else { f64::INFINITY };
        
        Self {
            total_trades: total,
            winning_trades: winners,
            losing_trades: losers,
            win_rate,
            total_pnl_usd: total_pnl,
            avg_pnl_per_trade: if total > 0 { total_pnl / total as f64 } else { 0.0 },
            max_drawdown: 0.0, // Would calculate properly in production
            sharpe_ratio: 0.0, // Would calculate properly in production
            profit_factor,
            trades,
        }
    }
}
