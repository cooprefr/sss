// src/data/price_data.rs

use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PricePoint {
    pub timestamp: u64,
    pub price: f64,
    pub volume: f64,
    pub liquidity: f64,
    pub tick: i32,
}

#[derive(Clone, Debug)]
pub struct CandlestickData {
    pub timestamp: u64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Clone, Debug)]
pub struct PriceHistory {
    pub points: VecDeque<PricePoint>,
    pub candlesticks: VecDeque<CandlestickData>,
    pub max_size: usize,
    pub timeframe_seconds: u64, // For candlestick aggregation
}

impl PriceHistory {
    pub fn new(max_size: usize, timeframe_seconds: u64) -> Self {
        Self {
            points: VecDeque::with_capacity(max_size),
            candlesticks: VecDeque::with_capacity(max_size / 10), // Fewer candles than points
            max_size,
            timeframe_seconds,
        }
    }

    pub fn add_price_point(&mut self, point: PricePoint) {
        // Add to points
        if self.points.len() >= self.max_size {
            self.points.pop_front();
        }
        self.points.push_back(point.clone());

        // Update or create candlestick
        self.update_candlestick(point);
    }

    fn update_candlestick(&mut self, point: PricePoint) {
        let candle_timestamp = (point.timestamp / self.timeframe_seconds) * self.timeframe_seconds;
        
        if let Some(last_candle) = self.candlesticks.back_mut() {
            if last_candle.timestamp == candle_timestamp {
                // Update existing candle
                last_candle.high = last_candle.high.max(point.price);
                last_candle.low = last_candle.low.min(point.price);
                last_candle.close = point.price;
                last_candle.volume += point.volume;
                return;
            }
        }

        // Create new candle
        let new_candle = CandlestickData {
            timestamp: candle_timestamp,
            open: point.price,
            high: point.price,
            low: point.price,
            close: point.price,
            volume: point.volume,
        };

        if self.candlesticks.len() >= self.max_size / 10 {
            self.candlesticks.pop_front();
        }
        self.candlesticks.push_back(new_candle);
    }

    pub fn get_price_range(&self, from_timestamp: u64, to_timestamp: u64) -> Vec<&PricePoint> {
        self.points
            .iter()
            .filter(|p| p.timestamp >= from_timestamp && p.timestamp <= to_timestamp)
            .collect()
    }

    pub fn get_latest_price(&self) -> Option<f64> {
        self.points.back().map(|p| p.price)
    }

    pub fn get_price_change_24h(&self) -> Option<f64> {
        if self.points.len() < 2 {
            return None;
        }

        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let day_ago = current_time - 86400; // 24 hours ago
        
        let old_price = self.points
            .iter()
            .find(|p| p.timestamp >= day_ago)
            .map(|p| p.price)?;
        
        let current_price = self.points.back()?.price;
        
        Some(((current_price - old_price) / old_price) * 100.0)
    }
}

// Price calculation utilities for Whirlpool
pub mod whirlpool_math {
    use crate::dex::whirlpool::state::Whirlpool;

    const Q64: u128 = 1u128 << 64;

    pub fn sqrt_price_x64_to_price(sqrt_price_x64: u128, decimals_a: u8, decimals_b: u8) -> f64 {
        // Convert sqrt_price from Q64.64 fixed point to f64
        let sqrt_price = sqrt_price_x64 as f64 / Q64 as f64;
        
        // Square to get the actual price ratio
        let price_ratio = sqrt_price * sqrt_price;
        
        // Adjust for token decimals
        let decimal_adjustment = 10f64.powi(decimals_a as i32 - decimals_b as i32);
        
        price_ratio * decimal_adjustment
    }

    pub fn calculate_price_from_whirlpool(
        whirlpool: &Whirlpool,
        decimals_a: u8,
        decimals_b: u8,
    ) -> f64 {
        sqrt_price_x64_to_price(whirlpool.sqrt_price, decimals_a, decimals_b)
    }

    pub fn calculate_liquidity_in_usd(
        whirlpool: &Whirlpool,
        price_a_usd: f64,
        price_b_usd: f64,
        decimals_a: u8,
        decimals_b: u8,
    ) -> f64 {
        // This is a simplified calculation
        // In reality, you'd need to calculate the amounts of each token based on liquidity and current tick
        let liquidity = whirlpool.liquidity as f64;
        let sqrt_price = whirlpool.sqrt_price as f64 / Q64 as f64;
        
        // Approximate liquidity value (this would need more complex math for accuracy)
        let token_a_amount = liquidity / sqrt_price / 10f64.powi(decimals_a as i32);
        let token_b_amount = liquidity * sqrt_price / 10f64.powi(decimals_b as i32);
        
        token_a_amount * price_a_usd + token_b_amount * price_b_usd
    }

    pub fn tick_to_price(tick: i32, decimals_a: u8, decimals_b: u8) -> f64 {
        // Price = 1.0001^tick adjusted for decimals
        let base_price = 1.0001f64.powi(tick);
        let decimal_adjustment = 10f64.powi(decimals_a as i32 - decimals_b as i32);
        base_price * decimal_adjustment
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::whirlpool_math::*;

    #[test]
    fn test_price_history() {
        let mut history = PriceHistory::new(100, 60); // 1-minute candles
        
        let point1 = PricePoint {
            timestamp: 1000,
            price: 100.0,
            volume: 1000.0,
            liquidity: 50000.0,
            tick: 0,
        };
        
        history.add_price_point(point1);
        assert_eq!(history.points.len(), 1);
        assert_eq!(history.candlesticks.len(), 1);
    }

    #[test]
    fn test_sqrt_price_conversion() {
        // Test with known values
        let sqrt_price_x64 = 79228162514264337593543950336u128; // sqrt(1) in Q64.64
        let price = sqrt_price_x64_to_price(sqrt_price_x64, 6, 6);
        assert!((price - 1.0).abs() < 0.0001);
    }
}