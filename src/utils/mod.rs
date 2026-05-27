//! Utility functions and helpers

/// Calculate Sharpe ratio
pub fn calculate_sharpe_ratio(returns: &[f64], risk_free_rate: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let avg_return = returns.iter().sum::<f64>() / returns.len() as f64;
    
    let variance = returns.iter()
        .map(|r| (r - avg_return).powi(2))
        .sum::<f64>() / returns.len() as f64;
    
    let std_dev = variance.sqrt();
    
    if std_dev < 1e-10 {
        return 0.0;
    }

    // Annualize (assuming daily returns)
    let annualized_return = avg_return * 252.0;
    let annualized_std = std_dev * 252.0_f64.sqrt();
    
    (annualized_return - risk_free_rate) / annualized_std
}

/// Calculate maximum drawdown from equity curve
pub fn calculate_max_drawdown(equity_curve: &[f64]) -> (f64, f64) {
    if equity_curve.is_empty() {
        return (0.0, 0.0);
    }

    let mut peak = equity_curve[0];
    let mut max_dd = 0.0;
    let mut max_dd_pct = 0.0;

    for &equity in equity_curve {
        if equity > peak {
            peak = equity;
        }
        
        let dd = peak - equity;
        let dd_pct = if peak > 0.0 { dd / peak } else { 0.0 };
        
        if dd > max_dd {
            max_dd = dd;
            max_dd_pct = dd_pct;
        }
    }

    (max_dd, max_dd_pct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sharpe_ratio() {
        let returns = vec![0.01, 0.02, -0.01, 0.03, 0.01];
        let sharpe = calculate_sharpe_ratio(&returns, 0.0);
        
        assert!(sharpe.is_finite());
        assert!(sharpe > 0.0);
    }

    #[test]
    fn test_max_drawdown() {
        let equity_curve = vec![100.0, 110.0, 120.0, 110.0, 115.0, 105.0, 125.0];
        let (dd, dd_pct) = calculate_max_drawdown(&equity_curve);
        
        assert!((dd - 15.0).abs() < 0.01); // From 120 to 105
        assert!((dd_pct - 0.125).abs() < 0.01); // 15/120
    }

    #[test]
    fn test_empty_inputs() {
        assert_eq!(calculate_sharpe_ratio(&[], 0.0), 0.0);
        assert_eq!(calculate_max_drawdown(&[]), (0.0, 0.0));
    }
}
