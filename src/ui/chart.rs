// src/ui/chart.rs

use eframe::egui::{self, plot::*, *};
use std::collections::VecDeque;
use crate::data::price_data::{PricePoint, CandlestickData, PriceHistory};

#[derive(Debug, Clone, PartialEq)]
pub enum ChartType {
    Line,
    Candlestick,
    Volume,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimeRange {
    Minutes1,
    Minutes5,
    Minutes15,
    Hours1,
    Hours4,
    Days1,
}

impl TimeRange {
    pub fn to_seconds(&self) -> u64 {
        match self {
            TimeRange::Minutes1 => 60,
            TimeRange::Minutes5 => 300,
            TimeRange::Minutes15 => 900,
            TimeRange::Hours1 => 3600,
            TimeRange::Hours4 => 14400,
            TimeRange::Days1 => 86400,
        }
    }

    pub fn to_string(&self) -> &'static str {
        match self {
            TimeRange::Minutes1 => "1m",
            TimeRange::Minutes5 => "5m",
            TimeRange::Minutes15 => "15m",
            TimeRange::Hours1 => "1h",
            TimeRange::Hours4 => "4h",
            TimeRange::Days1 => "1d",
        }
    }
}

pub struct TradingChart {
    pub chart_type: ChartType,
    pub time_range: TimeRange,
    pub show_volume: bool,
    pub show_grid: bool,
    pub auto_bounds: bool,
    pub selected_dex: String,
    pub price_histories: std::collections::HashMap<String, PriceHistory>,
    pub colors: ChartColors,
    
    // Interactive state
    pub zoom_level: f32,
    pub pan_offset: f64,
    pub crosshair_enabled: bool,
    pub last_bounds: Option<PlotBounds>,
}

#[derive(Debug, Clone)]
pub struct ChartColors {
    pub bull_candle: Color32,
    pub bear_candle: Color32,
    pub bull_wick: Color32,
    pub bear_wick: Color32,
    pub line_color: Color32,
    pub volume_color: Color32,
    pub grid_color: Color32,
    pub text_color: Color32,
    pub background: Color32,
}

impl Default for ChartColors {
    fn default() -> Self {
        Self {
            bull_candle: Color32::from_rgb(0, 150, 0),
            bear_candle: Color32::from_rgb(200, 0, 0),
            bull_wick: Color32::from_rgb(0, 100, 0),
            bear_wick: Color32::from_rgb(150, 0, 0),
            line_color: Color32::from_rgb(100, 150, 255),
            volume_color: Color32::from_rgba_premultiplied(100, 100, 100, 100),
            grid_color: Color32::from_rgba_premultiplied(100, 100, 100, 50),
            text_color: Color32::WHITE,
            background: Color32::from_rgb(20, 20, 25),
        }
    }
}

impl Default for TradingChart {
    fn default() -> Self {
        Self {
            chart_type: ChartType::Candlestick,
            time_range: TimeRange::Minutes5,
            show_volume: true,
            show_grid: true,
            auto_bounds: true,
            selected_dex: "Whirlpool".to_string(),
            price_histories: std::collections::HashMap::new(),
            colors: ChartColors::default(),
            zoom_level: 1.0,
            pan_offset: 0.0,
            crosshair_enabled: true,
            last_bounds: None,
        }
    }
}

impl TradingChart {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_price_history(&mut self, dex_name: String, history: PriceHistory) {
        self.price_histories.insert(dex_name, history);
    }

    pub fn update_price_point(&mut self, dex_name: &str, point: PricePoint) {
        if let Some(history) = self.price_histories.get_mut(dex_name) {
            history.add_price_point(point);
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // Chart controls
        self.show_controls(ui);
        
        ui.separator();

        // Main chart area
        let chart_height = ui.available_height() * if self.show_volume { 0.7 } else { 1.0 };
        
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), chart_height),
            Layout::top_down(Align::LEFT),
            |ui| {
                self.show_price_chart(ui);
            },
        );

        if self.show_volume {
            ui.separator();
            self.show_volume_chart(ui);
        }
    }

    fn show_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Chart Type:");
            egui::ComboBox::from_label("")
                .selected_text(format!("{:?}", self.chart_type))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.chart_type, ChartType::Line, "Line");
                    ui.selectable_value(&mut self.chart_type, ChartType::Candlestick, "Candlestick");
                });

            ui.separator();

            ui.label("Timeframe:");
            ui.horizontal(|ui| {
                for &time_range in &[
                    TimeRange::Minutes1,
                    TimeRange::Minutes5,
                    TimeRange::Minutes15,
                    TimeRange::Hours1,
                    TimeRange::Hours4,
                    TimeRange::Days1,
                ] {
                    if ui
                        .selectable_label(
                            self.time_range == time_range,
                            time_range.to_string(),
                        )
                        .clicked()
                    {
                        self.time_range = time_range;
                        // Regenerate candlesticks for new timeframe
                        self.regenerate_candlesticks();
                    }
                }
            });

            ui.separator();

            ui.checkbox(&mut self.show_volume, "Volume");
            ui.checkbox(&mut self.show_grid, "Grid");
            ui.checkbox(&mut self.auto_bounds, "Auto Bounds");

            ui.separator();

            // DEX selector
            egui::ComboBox::from_label("DEX")
                .selected_text(&self.selected_dex)
                .show_ui(ui, |ui| {
                    for dex_name in self.price_histories.keys() {
                        ui.selectable_value(&mut self.selected_dex, dex_name.clone(), dex_name);
                    }
                });
        });
    }

    fn show_price_chart(&mut self, ui: &mut egui::Ui) {
        let history = match self.price_histories.get(&self.selected_dex) {
            Some(h) => h,
            None => return,
        };

        Plot::new("price_chart")
            .height(ui.available_height())
            .show_grid(self.show_grid)
            .show_background(false)
            .show_axes([true, true])
            .allow_zoom(true)
            .allow_drag(true)
            .allow_scroll(true)
            .auto_bounds_x()
            .auto_bounds_y()
            .show(ui, |plot_ui| {
                // Store bounds for crosshair calculations
                self.last_bounds = Some(plot_ui.plot_bounds());

                match self.chart_type {
                    ChartType::Line => self.draw_line_chart(plot_ui, history),
                    ChartType::Candlestick => self.draw_candlestick_chart(plot_ui, history),
                    ChartType::Volume => {} // Volume only, handled separately
                }

                // Draw crosshair if enabled
                if self.crosshair_enabled {
                    self.draw_crosshair(plot_ui);
                }
            });
    }

    fn draw_line_chart(&self, plot_ui: &mut PlotUi, history: &PriceHistory) {
        let points: PlotPoints = history
            .points
            .iter()
            .map(|p| [p.timestamp as f64, p.price])
            .collect();

        let line = Line::new(points)
            .color(self.colors.line_color)
            .width(2.0)
            .name(&self.selected_dex);

        plot_ui.line(line);
    }

    fn draw_candlestick_chart(&self, plot_ui: &mut PlotUi, history: &PriceHistory) {
        for candle in &history.candlesticks {
            self.draw_single_candlestick(plot_ui, candle);
        }
    }

    fn draw_single_candlestick(&self, plot_ui: &mut PlotUi, candle: &CandlestickData) {
        let x = candle.timestamp as f64;
        let is_bullish = candle.close >= candle.open;
        
        let body_color = if is_bullish {
            self.colors.bull_candle
        } else {
            self.colors.bear_candle
        };
        
        let wick_color = if is_bullish {
            self.colors.bull_wick
        } else {
            self.colors.bear_wick
        };

        // Draw wick (high-low line)
        let wick_points = PlotPoints::from(vec![[x, candle.low], [x, candle.high]]);
        let wick = Line::new(wick_points).color(wick_color).width(1.0);
        plot_ui.line(wick);

        // Draw body (open-close rectangle)
        let body_top = candle.open.max(candle.close);
        let body_bottom = candle.open.min(candle.close);
        let candle_width = self.time_range.to_seconds() as f64 * 0.8; // 80% of timeframe width
        
        // Create rectangle for candle body
        let rect_points = vec![
            [x - candle_width / 2.0, body_bottom],
            [x + candle_width / 2.0, body_bottom],
            [x + candle_width / 2.0, body_top],
            [x - candle_width / 2.0, body_top],
        ];
        
        let polygon = Polygon::new(PlotPoints::from(rect_points))
            .color(body_color)
            .stroke(Stroke::new(1.0, body_color));
        plot_ui.polygon(polygon);
    }

    fn draw_crosshair(&self, plot_ui: &mut PlotUi) {
        if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
            // Vertical line
            let v_line = Line::new(PlotPoints::from(vec![
                [pointer_pos.x, plot_ui.plot_bounds().min()[1]],
                [pointer_pos.x, plot_ui.plot_bounds().max()[1]],
            ]))
            .color(Color32::from_rgba_premultiplied(255, 255, 255, 100))
            .width(1.0)
            .style(LineStyle::Dashed { length: 5.0 });
            
            // Horizontal line
            let h_line = Line::new(PlotPoints::from(vec![
                [plot_ui.plot_bounds().min()[0], pointer_pos.y],
                [plot_ui.plot_bounds().max()[0], pointer_pos.y],
            ]))
            .color(Color32::from_rgba_premultiplied(255, 255, 255, 100))
            .width(1.0)
            .style(LineStyle::Dashed { length: 5.0 });

            plot_ui.line(v_line);
            plot_ui.line(h_line);
        }
    }

    fn show_volume_chart(&mut self, ui: &mut egui::Ui) {
        let history = match self.price_histories.get(&self.selected_dex) {
            Some(h) => h,
            None => return,
        };

        ui.label("Volume");
        
        Plot::new("volume_chart")
            .height(ui.available_height())
            .show_grid(self.show_grid)
            .show_background(false)
            .show_axes([true, true])
            .allow_zoom(true)
            .allow_drag(true)
            .auto_bounds_x()
            .auto_bounds_y()
            .show(ui, |plot_ui| {
                // Draw volume bars
                for candle in &history.candlesticks {
                    let x = candle.timestamp as f64;
                    let bar_width = self.time_range.to_seconds() as f64 * 0.8;
                    
                    let bar_points = vec![
                        [x - bar_width / 2.0, 0.0],
                        [x + bar_width / 2.0, 0.0],
                        [x + bar_width / 2.0, candle.volume],
                        [x - bar_width / 2.0, candle.volume],
                    ];
                    
                    let polygon = Polygon::new(PlotPoints::from(bar_points))
                        .color(self.colors.volume_color)
                        .stroke(Stroke::new(0.5, self.colors.volume_color));
                    plot_ui.polygon(polygon);
                }
            });
    }

    fn regenerate_candlesticks(&mut self) {
        let timeframe = self.time_range.to_seconds();
        
        for history in self.price_histories.values_mut() {
            history.timeframe_seconds = timeframe;
            history.candlesticks.clear();
            
            // Rebuild candlesticks from price points
            for point in &history.points {
                history.update_candlestick(point.clone());
            }
        }
    }

    pub fn get_current_price(&self, dex_name: &str) -> Option<f64> {
        self.price_histories
            .get(dex_name)
            .and_then(|h| h.get_latest_price())
    }

    pub fn get_price_change_24h(&self, dex_name: &str) -> Option<f64> {
        self.price_histories
            .get(dex_name)
            .and_then(|h| h.get_price_change_24h())
    }
}

// Price ticker widget
pub struct PriceTicker {
    pub symbol: String,
    pub current_price: f64,
    pub price_change_24h: f64,
    pub volume_24h: f64,
    pub high_24h: f64,
    pub low_24h: f64,
}

impl PriceTicker {
    pub fn show(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(&self.symbol);
            ui.separator();
            
            // Current price
            ui.label(format!("${:.4}", self.current_price));
            
            // 24h change
            let change_color = if self.price_change_24h >= 0.0 {
                Color32::from_rgb(0, 200, 0)
            } else {
                Color32::from_rgb(200, 0, 0)
            };
            
            ui.colored_label(
                change_color,
                format!("{:+.2}%", self.price_change_24h),
            );
            
            ui.separator();
            
            // 24h high/low
            ui.label(format!("H: ${:.4}", self.high_24h));
            ui.label(format!("L: ${:.4}", self.low_24h));
            
            ui.separator();
            
            // Volume
            ui.label(format!("Vol: ${:.0}", self.volume_24h));
        });
    }
}

// Market depth widget for order book visualization
pub struct MarketDepth {
    pub bids: Vec<(f64, f64)>, // (price, quantity)
    pub asks: Vec<(f64, f64)>, // (price, quantity)
    pub spread: f64,
}

impl MarketDepth {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.label("Market Depth");
        
        Plot::new("market_depth")
            .height(200.0)
            .show_grid(true)
            .show_background(false)
            .allow_zoom(true)
            .auto_bounds_x()
            .auto_bounds_y()
            .show(ui, |plot_ui| {
                // Draw bid side (green)
                let mut cumulative_bid_volume = 0.0;
                let bid_points: PlotPoints = self.bids
                    .iter()
                    .map(|(price, qty)| {
                        cumulative_bid_volume += qty;
                        [*price, cumulative_bid_volume]
                    })
                    .collect();
                
                let bid_line = Line::new(bid_points)
                    .color(Color32::from_rgb(0, 150, 0))
                    .width(2.0)
                    .name("Bids");
                plot_ui.line(bid_line);
                
                // Draw ask side (red)
                let mut cumulative_ask_volume = 0.0;
                let ask_points: PlotPoints = self.asks
                    .iter()
                    .map(|(price, qty)| {
                        cumulative_ask_volume += qty;
                        [*price, cumulative_ask_volume]
                    })
                    .collect();
                
                let ask_line = Line::new(ask_points)
                    .color(Color32::from_rgb(200, 0, 0))
                    .width(2.0)
                    .name("Asks");
                plot_ui.line(ask_line);
            });
        
        ui.label(format!("Spread: ${:.4}", self.spread));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::price_data::PricePoint;

    #[test]
    fn test_chart_creation() {
        let mut chart = TradingChart::new();
        let mut history = PriceHistory::new(1000, 300); // 5-minute candles
        
        let point = PricePoint {
            timestamp: 1000,
            price: 100.0,
            volume: 1000.0,
            liquidity: 50000.0,
            tick: 0,
        };
        
        history.add_price_point(point);
        chart.add_price_history("Whirlpool".to_string(), history);
        
        assert!(chart.price_histories.contains_key("Whirlpool"));
    }

    #[test]
    fn test_time_range_conversion() {
        assert_eq!(TimeRange::Minutes1.to_seconds(), 60);
        assert_eq!(TimeRange::Hours1.to_seconds(), 3600);
        assert_eq!(TimeRange::Days1.to_seconds(), 86400);
    }
}