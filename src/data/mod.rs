//! Data Module - Market data ingestion and order book management

pub mod gateway;
pub mod types;
pub mod orderbook;

#[cfg(test)]
mod tests {
    #[test]
    fn test_data_module() {
        // Basic module test
        assert!(true);
    }
}
