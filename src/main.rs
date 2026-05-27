//! Polymarket 5-Minute BTC Prediction HFT System
//! 
//! Architecture: CQRS + Event-Driven
//! Core Principles: Zero-Allocation Hot Path, Lock-Free Concurrency

mod market;
mod data;
mod strategy;
mod execution;
mod utils;

use tokio::sync::broadcast;
use tracing::{info, error};
use std::sync::Arc;

use market::router::MarketRouter;
use data::gateway::DataGateway;
use strategy::engine::SignalEngine;
use execution::oms::OrderManagementSystem;
use utils::config::SystemConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("polymarket_hft=info".parse()?)
        )
        .init();

    info!("🚀 Starting Polymarket HFT Engine...");

    // Load configuration
    let config = SystemConfig::load()?;
    
    // Create shared state channels (Lock-free MPMC)
    let (tick_tx, _) = broadcast::channel::<data::types::Tick>(1024);
    let (signal_tx, _) = broadcast::channel::<strategy::types::Signal>(128);
    let (order_tx, _) = broadcast::channel::<execution::types::OrderCommand>(256);

    // Initialize components
    let router = Arc::new(MarketRouter::new(config.clone()).await?);
    let gateway = DataGateway::new(tick_tx.clone(), router.clone());
    let signal_engine = SignalEngine::new(signal_tx.clone(), config.clone());
    let oms = OrderManagementSystem::new(order_tx.clone(), config.clone());

    info!("✅ All components initialized");
    info!("📊 Current Market: {}", router.get_current_market_id());
    info!("⏱️  Next Rollover in: {}s", router.get_seconds_to_rollover());

    // Start components
    let gateway_handle = tokio::spawn(async move {
        if let Err(e) = gateway.run().await {
            error!("Data Gateway failed: {}", e);
        }
    });

    let signal_handle = tokio::spawn(async move {
        if let Err(e) = signal_engine.run().await {
            error!("Signal Engine failed: {}", e);
        }
    });

    let oms_handle = tokio::spawn(async move {
        if let Err(e) = oms.run().await {
            error!("OMS failed: {}", e);
        }
    });

    // Monitor for shutdown signals
    tokio::select! {
        _ = gateway_handle => info!("Gateway stopped"),
        _ = signal_handle => info!("Signal Engine stopped"),
        _ = oms_handle => info!("OMS stopped"),
        _ = tokio::signal::ctrl_c() => info!("Received shutdown signal"),
    }

    info!("🛑 System shutting down gracefully...");
    Ok(())
}
