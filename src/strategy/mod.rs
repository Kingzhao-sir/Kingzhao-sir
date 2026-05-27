//! Strategy engine module - signal generation and alpha calculation

pub mod engine;
pub mod features;
pub mod signals;

pub use engine::StrategyEngine;
pub use features::FeatureExtractor;
pub use signals::SignalGenerator;
