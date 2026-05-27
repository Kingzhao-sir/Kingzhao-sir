//! Order Book Manager - High-performance LOB maintenance
//! 
//! Features:
//! - Pre-allocated memory pools for price levels
//! - Zero-allocation updates in hot path
//! - BTreeMap-based implementation for O(log n) operations

use crate::types::{OrderBook, PriceLevel, Decimal};
use std::collections::BTreeMap;
use parking_lot::RwLock;

/// Manages multiple order books with zero-allocation updates
pub struct OrderBookManager {
    books: RwLock<BTreeMap<String, OrderBook>>,
}

impl OrderBookManager {
    pub fn new() -> Self {
        Self {
            books: RwLock::new(BTreeMap::new()),
        }
    }
    
    /// Get or create order book for symbol
    pub fn get_or_create(&self, symbol: &str) -> OrderBook {
        let mut books = self.books.write();
        books.entry(symbol.to_string())
            .or_insert_with(|| OrderBook::new(symbol.to_string()))
            .clone()
    }
    
    /// Update bid level (zero-allocation with object pool in production)
    pub fn update_bid(&self, symbol: &str, price: Decimal, quantity: Decimal) {
        let mut books = self.books.write();
        if let Some(book) = books.get_mut(symbol) {
            book.update_bid(price, quantity);
        }
    }
    
    /// Update ask level
    pub fn update_ask(&self, symbol: &str, price: Decimal, quantity: Decimal) {
        let mut books = self.books.write();
        if let Some(book) = books.get_mut(symbol) {
            book.update_ask(price, quantity);
        }
    }
    
    /// Get mid price for symbol
    pub fn get_mid_price(&self, symbol: &str) -> Option<Decimal> {
        let books = self.books.read();
        books.get(symbol).and_then(|book| book.mid_price())
    }
    
    /// Get spread for symbol
    pub fn get_spread(&self, symbol: &str) -> Option<Decimal> {
        let books = self.books.read();
        books.get(symbol).and_then(|book| book.spread())
    }
    
    /// Calculate order book imbalance (OBI)
    /// OBI = (bid_volume - ask_volume) / (bid_volume + ask_volume)
    pub fn get_obi(&self, symbol: &str, depth: usize) -> Option<f64> {
        let books = self.books.read();
        let book = books.get(symbol)?;
        
        let bid_volume: f64 = book.bids.iter()
            .take(depth)
            .map(|level| level.quantity.to_f64().unwrap_or(0.0))
            .sum();
        
        let ask_volume: f64 = book.asks.iter()
            .take(depth)
            .map(|level| level.quantity.to_f64().unwrap_or(0.0))
            .sum();
        
        let total = bid_volume + ask_volume;
        if total > 0.0 {
            Some((bid_volume - ask_volume) / total)
        } else {
            Some(0.0)
        }
    }
}

impl Default for OrderBookManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    
    #[test]
    fn test_order_book_manager_creation() {
        let manager = OrderBookManager::new();
        assert!(manager.books.read().is_empty());
    }
    
    #[test]
    fn test_update_and_query() {
        let manager = OrderBookManager::new();
        let symbol = "BTC";
        
        // Add some bids
        manager.update_bid(symbol, Decimal::from(50000), Decimal::from(1));
        manager.update_bid(symbol, Decimal::from(49999), Decimal::from(2));
        
        // Add some asks
        manager.update_ask(symbol, Decimal::from(50001), Decimal::from(1));
        manager.update_ask(symbol, Decimal::from(50002), Decimal::from(2));
        
        // Check mid price
        let mid = manager.get_mid_price(symbol).unwrap();
        assert_eq!(mid, Decimal::from(50000.5));
        
        // Check spread
        let spread = manager.get_spread(symbol).unwrap();
        assert_eq!(spread, Decimal::from(1));
    }
    
    #[test]
    fn test_order_book_imbalance() {
        let manager = OrderBookManager::new();
        let symbol = "BTC";
        
        // Symmetric book
        manager.update_bid(symbol, Decimal::from(50000), Decimal::from(10));
        manager.update_ask(symbol, Decimal::from(50001), Decimal::from(10));
        
        let obi = manager.get_obi(symbol, 1).unwrap();
        assert!((obi - 0.0).abs() < 0.001);
        
        // Bid-heavy book
        manager.update_bid(symbol, Decimal::from(49999), Decimal::from(20));
        let obi = manager.get_obi(symbol, 2).unwrap();
        assert!(obi > 0.0); // Positive OBI means more bid pressure
    }
}
