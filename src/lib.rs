// src/lib.rs - Main library module

pub mod data {
    pub mod price_data;
}

pub mod connection {
    pub mod websocket;
}

pub mod ui {
    pub mod chart;
}

pub mod dex {
    pub mod whirlpool {
        pub mod constants;
        pub mod state;
        pub mod mod;
    }
}

pub mod config;
pub mod utils;

// src/config.rs - Configuration management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use solana_program::pubkey::Pubkey;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub connection: ConnectionConfig,
    pub ui: UiConfig,
    pub trading: TradingConfig,
    pub pools: Vec<PoolConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub rpc_endpoint: String,
    pub ws_endpoint: String,
    pub timeout_seconds: u64,
    pub retry_attempts: u32,
    pub auto_reconnect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,
    pub chart_update_interval_ms: u64,
    pub max_chart_points: usize,
    pub default_timeframe: String,
    pub show_volume: bool,
    pub show_grid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    pub default_slippage: f64,
    pub max_price_impact: f64,
    pub price_alert_threshold: f64,
    pub enable_arbitrage_detection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub name: String,
    pub pubkey: String,
    pub dex: String,
    pub token_a: String,
    pub token_b: String,
    pub enabled: bool,
    pub priority: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig {
                rpc_endpoint: "https://api.mainnet-beta.solana.com".to_string(),
                ws_endpoint: "wss://api.mainnet-beta.solana.com".to_string(),
                timeout_seconds: 30,
                retry_attempts: 3,
                auto_reconnect: true,
            },
            ui: UiConfig {
                theme: "dark".to_string(),
                chart_update_interval_ms: 100,
                max_chart_points: 10000,
                default_timeframe: "5m".to_string(),
                show_volume: true,
                show_grid: true,
            },
            trading: TradingConfig {
                default_slippage: 1.0, // 1%
                max_price_impact: 5.0, // 5%
                price_alert_threshold: 2.0, // 2%
                enable_arbitrage_detection: true,
            },
            pools: vec![
                PoolConfig {
                    name: "SOL/USDC".to_string(),
                    pubkey: "HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ".to_string(),
                    dex: "Whirlpool".to_string(),
                    token_a: "So11111111111111111111111111111111111111112".to_string(),
                    token_b: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                    enabled: true,
                    priority: 1,
                },
            ],
        }
    }
}

impl AppConfig {
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    pub fn load_or_default(path: &str) -> Self {
        match Self::load_from_file(path) {
            Ok(config) => config,
            Err(_) => {
                let default_config = Self::default();
                if let Err(e) = default_config.save_to_file(path) {
                    eprintln!("Failed to save default config: {}", e);
                }
                default_config
            }
        }
    }
}

// src/utils.rs - Utility functions

use std::time::{SystemTime, UNIX_EPOCH};
use solana_program::pubkey::Pubkey;

pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn format_timestamp(timestamp: u64) -> String {
    let datetime = chrono::DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap_or_default();
    datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

pub fn format_price(price: f64, decimals: u8) -> String {
    format!("${:.precision$}", price, precision = decimals as usize)
}

pub fn format_volume(volume: f64) -> String {
    if volume >= 1_000_000.0 {
        format!("${:.1}M", volume / 1_000_000.0)
    } else if volume >= 1_000.0 {
        format!("${:.1}K", volume / 1_000.0)
    } else {
        format!("${:.2}", volume)
    }
}

pub fn calculate_percentage_change(old_value: f64, new_value: f64) -> f64 {
    if old_value == 0.0 {
        0.0
    } else {
        ((new_value - old_value) / old_value) * 100.0
    }
}

pub fn is_valid_pubkey(pubkey_str: &str) -> bool {
    pubkey_str.parse::<Pubkey>().is_ok()
}

// Price calculation utilities
pub mod price_utils {
    use crate::dex::whirlpool::state::Whirlpool;

    pub fn calculate_pool_tvl(
        whirlpool: &Whirlpool,
        price_a_usd: f64,
        price_b_usd: f64,
        decimals_a: u8,
        decimals_b: u8,
    ) -> f64 {
        // Simplified TVL calculation
        // In reality, this would require complex math to determine exact token amounts
        let liquidity = whirlpool.liquidity as f64;
        let sqrt_price = whirlpool.sqrt_price as f64 / (1u128 << 64) as f64;
        
        // Approximate calculation - would need more sophisticated math for accuracy
        let estimated_value_a = liquidity / sqrt_price / 10f64.powi(decimals_a as i32) * price_a_usd;
        let estimated_value_b = liquidity * sqrt_price / 10f64.powi(decimals_b as i32) * price_b_usd;
        
        estimated_value_a + estimated_value_b
    }

    pub fn calculate_fee_tier_display(fee_rate: u16) -> String {
        let percentage = fee_rate as f64 / 10000.0; // Fee rate is in basis points
        format!("{:.2}%", percentage)
    }

    pub fn estimate_swap_output(
        input_amount: f64,
        current_price: f64,
        liquidity: f64,
        fee_rate: u16,
    ) -> f64 {
        // Simplified swap calculation - real implementation would use AMM math
        let fee_multiplier = 1.0 - (fee_rate as f64 / 10000.0);
        let effective_input = input_amount * fee_multiplier;
        
        // This is a very simplified calculation
        // Real AMM math involves sqrt price calculations and liquidity distribution
        effective_input * current_price
    }
}

// Error handling
#[derive(Debug, thiserror::Error)]
pub enum TradingTerminalError {
    #[error("WebSocket connection error: {0}")]
    WebSocketError(String),
    
    #[error("Price calculation error: {0}")]
    PriceCalculationError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Data parsing error: {0}")]
    DataParsingError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
}

// Performance monitoring
pub struct PerformanceMonitor {
    pub websocket_latency_ms: f64,
    pub chart_update_time_ms: f64,
    pub price_updates_per_second: f64,
    pub memory_usage_mb: f64,
    pub last_update_time: std::time::Instant,
    update_count: u64,
    start_time: std::time::Instant,
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self {
            websocket_latency_ms: 0.0,
            chart_update_time_ms: 0.0,
            price_updates_per_second: 0.0,
            memory_usage_mb: 0.0,
            last_update_time: std::time::Instant::now(),
            update_count: 0,
            start_time: std::time::Instant::now(),
        }
    }
}

impl PerformanceMonitor {
    pub fn record_price_update(&mut self, latency_ms: f64) {
        self.websocket_latency_ms = latency_ms;
        self.update_count += 1;
        self.last_update_time = std::time::Instant::now();
        
        let elapsed_seconds = self.start_time.elapsed().as_secs_f64();
        if elapsed_seconds > 0.0 {
            self.price_updates_per_second = self.update_count as f64 / elapsed_seconds;
        }
    }

    pub fn record_chart_update(&mut self, update_time_ms: f64) {
        self.chart_update_time_ms = update_time_ms;
    }

    pub fn show_stats(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label("Performance Monitor");
            ui.separator();
            ui.label(format!("WebSocket Latency: {:.1}ms", self.websocket_latency_ms));
            ui.label(format!("Chart Update Time: {:.1}ms", self.chart_update_time_ms));
            ui.label(format!("Updates/sec: {:.1}", self.price_updates_per_second));
            ui.label(format!("Memory Usage: {:.1}MB", self.memory_usage_mb));
            ui.label(format!("Last Update: {:.1}s ago", self.last_update_time.elapsed().as_secs_f32()));
        });
    }
}

// Arbitrage detection system
pub struct ArbitrageDetector {
    pub opportunities: Vec<ArbitrageOpportunity>,
    pub min_profit_threshold: f64,
    pub max_price_age_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    pub token_pair: String,
    pub buy_dex: String,
    pub sell_dex: String,
    pub buy_price: f64,
    pub sell_price: f64,
    pub profit_percentage: f64,
    pub profit_usd: f64,
    pub timestamp: u64,
    pub confidence_score: f64,
}

impl ArbitrageDetector {
    pub fn new(min_profit_threshold: f64) -> Self {
        Self {
            opportunities: Vec::new(),
            min_profit_threshold,
            max_price_age_seconds: 10, // Only consider prices from last 10 seconds
        }
    }

    pub fn detect_opportunities(
        &mut self,
        prices: &std::collections::HashMap<String, std::collections::HashMap<String, f64>>,
    ) {
        self.opportunities.clear();
        let current_time = current_timestamp();

        // Compare prices across different DEXes for the same token pair
        for (token_pair, dex_prices) in prices {
            let mut dex_price_vec: Vec<(String, f64)> = dex_prices.iter()
                .map(|(dex, price)| (dex.clone(), *price))
                .collect();
            
            dex_price_vec.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

            if dex_price_vec.len() >= 2 {
                let (buy_dex, buy_price) = &dex_price_vec[0];
                let (sell_dex, sell_price) = &dex_price_vec[dex_price_vec.len() - 1];

                let profit_percentage = (sell_price - buy_price) / buy_price * 100.0;
                
                if profit_percentage >= self.min_profit_threshold {
                    self.opportunities.push(ArbitrageOpportunity {
                        token_pair: token_pair.clone(),
                        buy_dex: buy_dex.clone(),
                        sell_dex: sell_dex.clone(),
                        buy_price: *buy_price,
                        sell_price: *sell_price,
                        profit_percentage,
                        profit_usd: 0.0, // Would calculate based on trade size
                        timestamp: current_time,
                        confidence_score: self.calculate_confidence_score(profit_percentage),
                    });
                }
            }
        }

        // Sort by profit percentage
        self.opportunities.sort_by(|a, b| 
            b.profit_percentage.partial_cmp(&a.profit_percentage).unwrap()
        );
    }

    fn calculate_confidence_score(&self, profit_percentage: f64) -> f64 {
        // Simple confidence scoring based on profit size
        // Higher profits get higher confidence, but capped at reasonable levels
        (profit_percentage / 10.0).min(1.0).max(0.0)
    }

    pub fn show_opportunities(&self, ui: &mut egui::Ui) {
        ui.heading("Arbitrage Opportunities");
        
        if self.opportunities.is_empty() {
            ui.label("No arbitrage opportunities detected");
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for opp in &self.opportunities {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(&opp.token_pair);
                        ui.separator();
                        ui.colored_label(
                            egui::Color32::GREEN,
                            format!("+{:.2}%", opp.profit_percentage)
                        );
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label(format!("Buy: {} @ ${:.4}", opp.buy_dex, opp.buy_price));
                        ui.label(format!("Sell: {} @ ${:.4}", opp.sell_dex, opp.sell_price));
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label(format!("Confidence: {:.0}%", opp.confidence_score * 100.0));
                        ui.label(format!("Age: {}s", current_timestamp() - opp.timestamp));
                    });
                });
                ui.separator();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentage_change_calculation() {
        assert_eq!(calculate_percentage_change(100.0, 110.0), 10.0);
        assert_eq!(calculate_percentage_change(100.0, 90.0), -10.0);
        assert_eq!(calculate_percentage_change(0.0, 10.0), 0.0);
    }

    #[test]
    fn test_pubkey_validation() {
        assert!(is_valid_pubkey("So11111111111111111111111111111111111111112"));
        assert!(!is_valid_pubkey("invalid_pubkey"));
    }

    #[test]
    fn test_volume_formatting() {
        assert_eq!(format_volume(1500000.0), "$1.5M");
        assert_eq!(format_volume(1500.0), "$1.5K");
        assert_eq!(format_volume(15.5), "$15.50");
    }

    #[test]
    fn test_arbitrage_detection() {
        let mut detector = ArbitrageDetector::new(1.0);
        let mut prices = std::collections::HashMap::new();
        
        let mut sol_usdc_prices = std::collections::HashMap::new();
        sol_usdc_prices.insert("Whirlpool".to_string(), 100.0);
        sol_usdc_prices.insert("Orca".to_string(), 102.0);
        
        prices.insert("SOL/USDC".to_string(), sol_usdc_prices);
        
        detector.detect_opportunities(&prices);
        
        assert_eq!(detector.opportunities.len(), 1);
        assert_eq!(detector.opportunities[0].profit_percentage, 2.0);
    }
}