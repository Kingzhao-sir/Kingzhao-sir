//! High-Performance Order Book Builder
//! Uses object pooling and zero-allocation hot path

use crate::data::types::{Level, OrderBookSnapshot, Side};

/// In-memory order book with fixed capacity to avoid allocations
pub struct OrderBook {
    bids: [Level; 20], // Top 20 levels
    asks: [Level; 20],
    bid_count: usize,
    ask_count: usize,
    last_update_ms: i64,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: [Level { price: 0.0, size: 0.0, order_count: 0 }; 20],
            asks: [Level { price: 0.0, size: 0.0, order_count: 0 }; 20],
            bid_count: 0,
            ask_count: 0,
            last_update_ms: 0,
        }
    }
    
    /// Update order book from snapshot (zero-copy where possible)
    #[inline]
    pub fn update(&mut self, snapshot: &OrderBookSnapshot) {
        self.last_update_ms = snapshot.timestamp_ms;
        
        // Update bids
        self.bid_count = snapshot.bids.len().min(20);
        for i in 0..self.bid_count {
            self.bids[i] = snapshot.bids[i];
        }
        
        // Update asks
        self.ask_count = snapshot.asks.len().min(20);
        for i in 0..self.ask_count {
            self.asks[i] = snapshot.asks[i];
        }
    }
    
    /// Get best bid price
    #[inline]
    pub fn best_bid(&self) -> Option<f64> {
        if self.bid_count > 0 {
            Some(self.bids[0].price)
        } else {
            None
        }
    }
    
    /// Get best ask price
    #[inline]
    pub fn best_ask(&self) -> Option<f64> {
        if self.ask_count > 0 {
            Some(self.asks[0].price)
        } else {
            None
        }
    }
    
    /// Get mid price
    #[inline]
    pub fn mid_price(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / 2.0),
            (Some(bid), None) => Some(bid),
            (None, Some(ask)) => Some(ask),
            (None, None) => None,
        }
    }
    
    /// Calculate Order Book Imbalance (OBI)
    #[inline]
    pub fn calculate_obi(&self) -> f64 {
        let bid_volume: f64 = self.bids[..self.bid_count].iter().map(|l| l.size).sum();
        let ask_volume: f64 = self.asks[..self.ask_count].iter().map(|l| l.size).sum();
        let total = bid_volume + ask_volume;
        
        if total == 0.0 {
            0.0
        } else {
            (bid_volume - ask_volume) / total
        }
    }
    
    /// Get spread in basis points
    #[inline]
    pub fn spread_bps(&self) -> u32 {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => {
                let mid = (bid + ask) / 2.0;
                if mid > 0.0 {
                    ((ask - bid) / mid * 10000.0) as u32
                } else {
                    0
                }
            }
            _ => 0,
        }
    }
    
    /// Create snapshot for serialization
    pub fn to_snapshot(&self) -> OrderBookSnapshot {
        OrderBookSnapshot::new(
            self.last_update_ms,
            self.bids[..self.bid_count].to_vec(),
            self.asks[..self.ask_count].to_vec(),
        )
    }
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_orderbook_basic() {
        let mut ob = OrderBook::new();
        
        let snapshot = OrderBookSnapshot::new(
            1000,
            vec![
                Level { price: 99.0, size: 100.0, order_count: 5 },
                Level { price: 98.0, size: 200.0, order_count: 3 },
            ],
            vec![
                Level { price: 101.0, size: 150.0, order_count: 4 },
                Level { price: 102.0, size: 250.0, order_count: 2 },
            ],
        );
        
        ob.update(&snapshot);
        
        assert_eq!(ob.best_bid(), Some(99.0));
        assert_eq!(ob.best_ask(), Some(101.0));
        assert_eq!(ob.mid_price(), Some(100.0));
        assert!(ob.spread_bps() > 0);
    }
    
    #[test]
    fn test_obi_calculation() {
        let mut ob = OrderBook::new();
        
        // More buy pressure
        let snapshot = OrderBookSnapshot::new(
            1000,
            vec![Level { price: 99.0, size: 300.0, order_count: 5 }],
            vec![Level { price: 101.0, size: 100.0, order_count: 4 }],
        );
        
        ob.update(&snapshot);
        let obi = ob.calculate_obi();
        assert!(obi > 0.0); // Positive = buy pressure
        
        // More sell pressure
        let snapshot2 = OrderBookSnapshot::new(
            2000,
            vec![Level { price: 99.0, size: 100.0, order_count: 5 }],
            vec![Level { price: 101.0, size: 300.0, order_count: 4 }],
        );
        
        ob.update(&snapshot2);
        let obi2 = ob.calculate_obi();
        assert!(obi2 < 0.0); // Negative = sell pressure
    }
}
