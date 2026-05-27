//! Backtest configuration and data structures

use serde::{Deserialize, Serialize};
use rust_decimal::Decimal;
use crate::core::TimestampUs;

/// Backtest configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    /// Initial capital in USDC
    pub initial_capital: Decimal,
    /// Commission rate (e.g., 0.02 for 2%)
    pub commission_rate: f64,
    /// Slippage model: fixed bps or percentage
    pub slippage_bps: u32,
    /// Start timestamp for backtest
    pub start_time_us: TimestampUs,
    /// End timestamp for backtest
    pub end_time_us: TimestampUs,
    /// Minimum confidence threshold to execute
    pub min_confidence: f64,
    /// Maximum position size per trade
    pub max_position_size: Decimal,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            initial_capital: Decimal::from(10000), // $10,000
            commission_rate: 0.02, // 2%
            slippage_bps: 10, // 0.1% slippage
            start_time_us: 0,
            end_time_us: 0,
            min_confidence: 0.65,
            max_position_size: Decimal::from(500),
        }
    }
}

/// Single tick data point for backtesting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestTick {
    pub timestamp_us: TimestampUs,
    pub market_id: String,
    pub price: Decimal,
    pub quantity: Decimal,
    pub side: crate::core::Side,
    pub implied_probability: f64,
}

/// OHLCV candle for backtesting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestCandle {
    pub timestamp_us: TimestampUs,
    pub market_id: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub period_seconds: u32,
}

/// Trade record from backtest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestTrade {
    pub timestamp_us: TimestampUs,
    pub market_id: String,
    pub side: crate::core::Side,
    pub entry_price: Decimal,
    pub exit_price: Option<Decimal>,
    pub quantity: Decimal,
    pub pnl: Option<Decimal>,
    pub commission: Decimal,
    pub slippage: Decimal,
    pub exit_timestamp_us: Option<TimestampUs>,
}

/// Backtest report summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestReport {
    /// Total number of trades
    pub total_trades: u32,
    /// Number of winning trades
    pub winning_trades: u32,
    /// Number of losing trades
    pub losing_trades: u32,
    /// Win rate (winning_trades / total_trades)
    pub win_rate: f64,
    /// Total profit/loss in USDC
    pub total_pnl: Decimal,
    /// Total commissions paid
    pub total_commission: Decimal,
    /// Total slippage cost
    pub total_slippage: Decimal,
    /// Net PnL after costs
    pub net_pnl: Decimal,
    /// Return on initial capital
    pub return_pct: f64,
    /// Maximum drawdown (peak to trough)
    pub max_drawdown: Decimal,
    /// Maximum drawdown percentage
    pub max_drawdown_pct: f64,
    /// Sharpe ratio (annualized)
    pub sharpe_ratio: f64,
    /// Average trade duration in seconds
    pub avg_trade_duration_sec: f64,
    /// Largest winning trade
    pub largest_win: Decimal,
    /// Largest losing trade
    pub largest_loss: Decimal,
    /// Average winning trade
    pub avg_win: Decimal,
    /// Average losing trade
    pub avg_loss: Decimal,
    /// Profit factor (gross profit / gross loss)
    pub profit_factor: f64,
    /// Starting capital
    pub starting_capital: Decimal,
    /// Ending capital
    pub ending_capital: Decimal,
}

impl BacktestReport {
    pub fn new(starting_capital: Decimal) -> Self {
        Self {
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
            win_rate: 0.0,
            total_pnl: Decimal::ZERO,
            total_commission: Decimal::ZERO,
            total_slippage: Decimal::ZERO,
            net_pnl: Decimal::ZERO,
            return_pct: 0.0,
            max_drawdown: Decimal::ZERO,
            max_drawdown_pct: 0.0,
            sharpe_ratio: 0.0,
            avg_trade_duration_sec: 0.0,
            largest_win: Decimal::ZERO,
            largest_loss: Decimal::ZERO,
            avg_win: Decimal::ZERO,
            avg_loss: Decimal::ZERO,
            profit_factor: 0.0,
            starting_capital,
            ending_capital: starting_capital,
        }
    }

    /// Calculate final metrics after all trades are processed
    pub fn finalize(&mut self, trades: &[BacktestTrade]) {
        self.total_trades = trades.len() as u32;
        
        if self.total_trades == 0 {
            return;
        }

        let mut gross_profit = Decimal::ZERO;
        let mut gross_loss = Decimal::ZERO;
        let mut total_duration = 0.0;
        let mut completed_trades = 0;

        for trade in trades {
            if let Some(pnl) = trade.pnl {
                if pnl > Decimal::ZERO {
                    gross_profit += pnl;
                    self.winning_trades += 1;
                    if pnl > self.largest_win {
                        self.largest_win = pnl;
                    }
                } else {
                    gross_loss += pnl.abs();
                    self.losing_trades += 1;
                    if pnl.abs() > self.largest_loss.abs() {
                        self.largest_loss = pnl;
                    }
                }

                self.total_pnl += pnl;
                completed_trades += 1;

                if let (Some(exit_ts), _) = (trade.exit_timestamp_us, trade.timestamp_us) {
                    total_duration += (exit_ts - trade.timestamp_us) as f64 / 1_000_000.0;
                }
            }

            self.total_commission += trade.commission;
            self.total_slippage += trade.slippage;
        }

        self.net_pnl = self.total_pnl - self.total_commission - self.total_slippage;
        self.ending_capital = self.starting_capital + self.net_pnl;
        
        if self.starting_capital != Decimal::ZERO {
            self.return_pct = (self.net_pnl / self.starting_capital).to_f64().unwrap_or(0.0);
        }

        if self.winning_trades > 0 {
            self.avg_win = gross_profit / Decimal::from(self.winning_trades);
        }

        if self.losing_trades > 0 {
            self.avg_loss = gross_loss / Decimal::from(self.losing_trades);
        }

        if gross_loss != Decimal::ZERO {
            self.profit_factor = (gross_profit / gross_loss).to_f64().unwrap_or(0.0);
        }

        if completed_trades > 0 {
            self.avg_trade_duration_sec = total_duration / completed_trades as f64;
        }

        self.win_rate = if self.total_trades > 0 {
            self.winning_trades as f64 / self.total_trades as f64
        } else {
            0.0
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backtest_config_default() {
        let config = BacktestConfig::default();
        
        assert_eq!(config.initial_capital, Decimal::from(10000));
        assert_eq!(config.commission_rate, 0.02);
        assert_eq!(config.slippage_bps, 10);
        assert!((config.min_confidence - 0.65).abs() < 0.01);
    }

    #[test]
    fn test_backtest_report_initialization() {
        let report = BacktestReport::new(Decimal::from(10000));
        
        assert_eq!(report.starting_capital, Decimal::from(10000));
        assert_eq!(report.ending_capital, Decimal::from(10000));
        assert_eq!(report.total_trades, 0);
        assert_eq!(report.total_pnl, Decimal::ZERO);
    }

    #[test]
    fn test_backtest_report_with_trades() {
        let mut report = BacktestReport::new(Decimal::from(10000));
        
        let trades = vec![
            BacktestTrade {
                timestamp_us: 1000000,
                market_id: "test".to_string(),
                side: crate::core::Side::Yes,
                entry_price: Decimal::from(50),
                exit_price: Some(Decimal::from(55)),
                quantity: Decimal::from(100),
                pnl: Some(Decimal::from(500)),
                commission: Decimal::from(10),
                slippage: Decimal::from(5),
                exit_timestamp_us: Some(2000000),
            },
            BacktestTrade {
                timestamp_us: 3000000,
                market_id: "test".to_string(),
                side: crate::core::Side::No,
                entry_price: Decimal::from(50),
                exit_price: Some(Decimal::from(48)),
                quantity: Decimal::from(100),
                pnl: Some(Decimal::from(200)),
                commission: Decimal::from(10),
                slippage: Decimal::from(5),
                exit_timestamp_us: Some(4000000),
            },
        ];
        
        report.finalize(&trades);
        
        assert_eq!(report.total_trades, 2);
        assert_eq!(report.winning_trades, 2);
        assert_eq!(report.losing_trades, 0);
        assert!((report.win_rate - 1.0).abs() < 0.01);
        assert_eq!(report.total_pnl, Decimal::from(700));
        assert_eq!(report.total_commission, Decimal::from(20));
        assert_eq!(report.total_slippage, Decimal::from(10));
    }
}
