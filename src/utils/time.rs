//! Time utilities for market cycle management

use chrono::{DateTime, Utc, Duration};

/// Calculate the current 5-minute market window
#[inline]
pub fn get_current_market_window() -> (i64, i64) {
    let now = Utc::now();
    let timestamp_ms = now.timestamp_millis();
    
    // 5 minutes = 300,000 ms
    let window_start = (timestamp_ms / 300_000) * 300_000;
    let window_end = window_start + 300_000;
    
    (window_start, window_end)
}

/// Calculate seconds until market close
#[inline]
pub fn seconds_to_market_close() -> u32 {
    let (_, window_end) = get_current_market_window();
    let now = Utc::now().timestamp_millis();
    ((window_end - now) / 1000) as u32
}

/// Check if we're in the decision window (last 30 seconds)
#[inline]
pub fn is_in_decision_window(decision_seconds: u32) -> bool {
    seconds_to_market_close() <= decision_seconds
}

/// Check if we're in the blackout window (last 5 seconds)
#[inline]
pub fn is_in_blackout_window(blackout_seconds: u32) -> bool {
    seconds_to_market_close() <= blackout_seconds
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_market_window_calculation() {
        let (start, end) = get_current_market_window();
        assert_eq!(end - start, 300_000); // 5 minutes in ms
    }
    
    #[test]
    fn test_seconds_to_close_reasonable() {
        let seconds = seconds_to_market_close();
        assert!(seconds < 300); // Should be less than 5 minutes
    }
}
