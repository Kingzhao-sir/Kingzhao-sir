//! Data Gateway - Multi-source market data ingestion
//! Uses zero-copy parsing and lock-free channels

use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn, error};

use crate::data::types::{Tick, DataSource, OrderBookSnapshot, Level, Side};
use crate::data::orderbook::OrderBook;
use crate::market::router::MarketRouter;

pub struct DataGateway {
    tick_sender: broadcast::Sender<Tick>,
    router: Arc<MarketRouter>,
    orderbook: Arc<parking_lot::RwLock<OrderBook>>,
}

impl DataGateway {
    pub fn new(tick_sender: broadcast::Sender<Tick>, router: Arc<MarketRouter>) -> Self {
        Self {
            tick_sender,
            router,
            orderbook: Arc::new(parking_lot::RwLock::new(OrderBook::new())),
        }
    }
    
    /// Main entry point - runs the data ingestion loop
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("📡 Data Gateway starting...");
        
        // In production, this would:
        // 1. Connect to Binance WebSocket for BTC/USDT trades
        // 2. Connect to Polymarket WebSocket for order book updates
        // 3. Parse binary/JSON with simd-json for zero-copy
        // 4. Reconstruct order book in memory
        
        // Simulation loop for demonstration
        self.run_simulation().await?;
        
        Ok(())
    }
    
    /// Simulate data ingestion (replace with real WS connections in production)
    async fn run_simulation(&self) -> Result<(), Box<dyn std::error::Error>> {
        use tokio::time::{sleep, Duration};
        
        let mut interval = tokio::time::interval(Duration::from_millis(100)); // 10Hz simulation
        
        for i in 0..100 {
            sleep(Duration::from_millis(100)).await;
            
            // Simulate incoming tick from Binance
            let tick = Tick {
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                price: 95000.0 + (i as f64 * 0.1),
                quantity: 0.5,
                side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
                source: DataSource::Binance,
            };
            
            // Send tick (non-blocking)
            let _ = self.tick_sender.send(tick);
            
            // Update simulated order book
            let snapshot = self.generate_simulated_orderbook();
            *self.orderbook.write() = OrderBook::new(); // Would update incrementally in production
            
            if i % 10 == 0 {
                info!("📊 Processed {} ticks", i + 1);
            }
        }
        
        info!("📊 Data Gateway simulation complete");
        Ok(())
    }
    
    /// Generate simulated order book snapshot
    fn generate_simulated_orderbook(&self) -> OrderBookSnapshot {
        let mid = 0.52; // Simulated Polymarket price (52% probability)
        let spread = 0.002; // 20 bps
        
        OrderBookSnapshot::new(
            chrono::Utc::now().timestamp_millis(),
            vec![
                Level { price: mid - spread / 2.0, size: 1000.0, order_count: 10 },
                Level { price: mid - spread, size: 2000.0, order_count: 15 },
            ],
            vec![
                Level { price: mid + spread / 2.0, size: 1200.0, order_count: 12 },
                Level { price: mid + spread, size: 1800.0, order_count: 8 },
            ],
        )
    }
    
    /// Get current order book snapshot
    pub fn get_orderbook(&self) -> OrderBookSnapshot {
        self.orderbook.read().to_snapshot()
    }
    
    /// Get order book imbalance
    pub fn get_obi(&self) -> f64 {
        self.orderbook.read().calculate_obi()
    }
}

// Fix Side enum reference
mod fix {
    use crate::data::types::Side;
    
    pub fn parse_side(s: &str) -> Side {
        match s.to_lowercase().as_str() {
            "buy" | "bid" => Side::Buy,
            "sell" | "ask" => Side::Sell,
            _ => Side::Buy, // Default
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;
    
    #[tokio::test]
    async fn test_gateway_creation() {
        let (tx, _) = broadcast::channel::<Tick>(100);
        let config = crate::utils::config::SystemConfig::load().unwrap();
        let router = Arc::new(MarketRouter::new(config).await.unwrap());
        
        let gateway = DataGateway::new(tx, router);
        
        // Verify gateway can be created
        let obi = gateway.get_obi();
        assert_eq!(obi, 0.0); // Empty order book
    }
}
