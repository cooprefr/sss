// Updated main.rs with full trading terminal integration

use eframe::{egui, App, Frame};
use egui::{CentralPanel, Context, SidePanel, TopBottomPanel, Color32};
use std::collections::HashMap;
use solana_program::pubkey::Pubkey;
use std::str::FromStr;

mod data;
mod connection;
mod ui;
mod dex;

use data::price_data::{PriceHistory, PricePoint, whirlpool_math};
use connection::websocket::{SolanaWebSocketClient, SolanaHttpClient, WhirlpoolUpdate};
use ui::chart::{TradingChart, PriceTicker, MarketDepth};
use dex::whirlpool::state::Whirlpool;

#[derive(PartialEq)]
enum ViewTab {
    Chart,
    Orders,
    GraphArb,
}

pub struct MyApp {
    active_tab: ViewTab,
    show_file_menu: bool,
    
    // Trading data
    trading_chart: TradingChart,
    price_tickers: HashMap<String, PriceTicker>,
    market_depth: MarketDepth,
    
    // Connection state
    ws_client: Option<SolanaWebSocketClient>,
    http_client: SolanaHttpClient,
    connected: bool,
    connection_status: String,
    
    // Selected pools and tokens
    selected_pools: Vec<PoolInfo>,
    token_metadata: HashMap<Pubkey, TokenMetadata>,
    
    // UI state
    show_settings: bool,
    rpc_endpoint: String,
    ws_endpoint: String,
    auto_reconnect: bool,
    
    // Real-time updates
    last_update_time: std::time::Instant,
    update_counter: u64,
}

#[derive(Clone, Debug)]
pub struct PoolInfo {
    pub pubkey: Pubkey,
    pub name: String,
    pub token_a: Pubkey,
    pub token_b: Pubkey,
    pub dex_name: String,
    pub tick_spacing: u16,
    pub fee_rate: u16,
}

#[derive(Clone, Debug)]
pub struct TokenMetadata {
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub logo_uri: Option<String>,
}

impl Default for MyApp {
    fn default() -> Self {
        let mut app = Self {
            active_tab: ViewTab::Chart,
            show_file_menu: false,
            trading_chart: TradingChart::new(),
            price_tickers: HashMap::new(),
            market_depth: MarketDepth {
                bids: vec![],
                asks: vec![],
                spread: 0.0,
            },
            ws_client: None,
            http_client: SolanaHttpClient::new("https://api.mainnet-beta.solana.com".to_string()),
            connected: false,
            connection_status: "Disconnected".to_string(),
            selected_pools: vec![],
            token_metadata: HashMap::new(),
            show_settings: false,
            rpc_endpoint: "https://api.mainnet-beta.solana.com".to_string(),
            ws_endpoint: "wss://api.mainnet-beta.solana.com".to_string(),
            auto_reconnect: true,
            last_update_time: std::time::Instant::now(),
            update_counter: 0,
        };

        // Add some default pools (popular Whirlpool pools)
        app.add_default_pools();
        
        // Initialize price histories for each DEX
        app.trading_chart.add_price_history("Whirlpool".to_string(), PriceHistory::new(10000, 300));
        
        app
    }
}

impl MyApp {
    fn add_default_pools(&mut self) {
        // SOL/USDC pool (most liquid)
        if let (Ok(pool_pubkey), Ok(sol_mint), Ok(usdc_mint)) = (
            Pubkey::from_str("HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ"),
            Pubkey::from_str("So11111111111111111111111111111111111111112"),
            Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
        ) {
            self.selected_pools.push(PoolInfo {
                pubkey: pool_pubkey,
                name: "SOL/USDC".to_string(),
                token_a: sol_mint,
                token_b: usdc_mint,
                dex_name: "Whirlpool".to_string(),
                tick_spacing: 64,
                fee_rate: 300, // 0.3%
            });

            // Add token metadata
            self.token_metadata.insert(sol_mint, TokenMetadata {
                symbol: "SOL".to_string(),
                name: "Solana".to_string(),
                decimals: 9,
                logo_uri: None,
            });

            self.token_metadata.insert(usdc_mint, TokenMetadata {
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
                decimals: 6,
                logo_uri: None,
            });
        }
    }

    async fn connect_websocket(&mut self) {
        match SolanaWebSocketClient::new(&self.ws_endpoint).await {
            Ok(mut client) => {
                // Subscribe to all selected pools
                for pool in &self.selected_pools {
                    if let Err(e) = client.subscribe(pool.pubkey) {
                        eprintln!("Failed to subscribe to {}: {}", pool.name, e);
                    }
                }
                
                self.ws_client = Some(client);
                self.connected = true;
                self.connection_status = "Connected".to_string();
            }
            Err(e) => {
                eprintln!("Failed to connect WebSocket: {}", e);
                self.connection_status = format!("Connection failed: {}", e);
            }
        }
    }

    fn process_whirlpool_update(&mut self, update: WhirlpoolUpdate) {
        // Find the pool info for this update
        let pool_info = self.selected_pools
            .iter()
            .find(|p| p.pubkey == update.pubkey);

        if let Some(pool) = pool_info {
            // Get token metadata for price calculation
            let token_a_meta = self.token_metadata.get(&pool.token_a);
            let token_b_meta = self.token_metadata.get(&pool.token_b);

            if let (Some(meta_a), Some(meta_b)) = (token_a_meta, token_b_meta) {
                // Calculate price
                let price = whirlpool_math::calculate_price_from_whirlpool(
                    &update.whirlpool,
                    meta_a.decimals,
                    meta_b.decimals,
                );

                // Create price point
                let price_point = PricePoint {
                    timestamp: update.timestamp,
                    price,
                    volume: 0.0, // Would need to calculate from recent trades
                    liquidity: update.whirlpool.liquidity as f64,
                    tick: update.whirlpool.tick_current_index,
                };

                // Update chart
                self.trading_chart.update_price_point(&pool.dex_name, price_point);

                // Update price ticker
                let price_change_24h = self.trading_chart
                    .get_price_change_24h(&pool.dex_name)
                    .unwrap_or(0.0);

                self.price_tickers.insert(pool.name.clone(), PriceTicker {
                    symbol: pool.name.clone(),
                    current_price: price,
                    price_change_24h,
                    volume_24h: 0.0, // Would need historical data
                    high_24h: price, // Would need to track
                    low_24h: price,  // Would need to track
                });

                self.update_counter += 1;
                self.last_update_time = std::time::Instant::now();
            }
        }
    }
}

impl App for MyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        // Set dark theme
        ctx.set_visuals(egui::Visuals::dark());

        // Process WebSocket updates
        if let Some(ref mut client) = self.ws_client {
            while let Ok(update) = client.try_recv() {
                self.process_whirlpool_update(update);
            }
        }

        // Request repaint for real-time updates
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        // Top menu bar
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // File menu
                if ui.button("File").clicked() {
                    self.show_file_menu = !self.show_file_menu;
                }

                ui.separator();

                // Tab selection
                if ui
                    .selectable_label(self.active_tab == ViewTab::Chart, "Chart")
                    .clicked()
                {
                    self.active_tab = ViewTab::Chart;
                }

                if ui
                    .selectable_label(self.active_tab == ViewTab::Orders, "Orders/Trades")
                    .clicked()
                {
                    self.active_tab = ViewTab::Orders;
                }

                if ui
                    .selectable_label(self.active_tab == ViewTab::GraphArb, "Graph Arb")
                    .clicked()
                {
                    self.active_tab = ViewTab::GraphArb;
                }

                ui.separator();

                // Connection status
                let status_color = if self.connected {
                    Color32::GREEN
                } else {
                    Color32::RED
                };
                ui.colored_label(status_color, &self.connection_status);

                // Connect/Disconnect button
                if self.connected {
                    if ui.button("Disconnect").clicked() {
                        self.ws_client = None;
                        self.connected = false;
                        self.connection_status = "Disconnected".to_string();
                    }
                } else {
                    if ui.button("Connect").clicked() {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async {
                            self.connect_websocket().await;
                        });
                    }
                }

                ui.separator();

                // Settings
                if ui.button("Settings").clicked() {
                    self.show_settings = !self.show_settings;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("Updates: {} | Last: {:.1}s ago", 
                        self.update_counter,
                        self.last_update_time.elapsed().as_secs_f32()
                    ));
                });
            });
        });

        // Left sidebar for token list and controls
        if self.show_file_menu {
            SidePanel::left("left_panel")
                .resizable(true)
                .default_width(250.0)
                .show(ctx, |ui| {
                    ui.heading("Solana Pools");
                    ui.separator();

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for pool in &self.selected_pools.clone() {
                            ui.horizontal(|ui| {
                                let is_selected = self.trading_chart.selected_dex == pool.dex_name;
                                if ui
                                    .selectable_label(is_selected, &pool.name)
                                    .clicked()
                                {
                                    self.trading_chart.selected_dex = pool.dex_name.clone();
                                }

                                // Show current price if available
                                if let Some(price) = self.trading_chart.get_current_price(&pool.dex_name) {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(format!("${:.4}", price));
                                    });
                                }
                            });
                            ui.separator();
                        }
                    });

                    ui.separator();
                    if ui.button("Add Pool").clicked() {
                        // TODO: Open pool selection dialog
                    }
                });
        }

        // Settings panel
        if self.show_settings {
            SidePanel::right("settings_panel")
                .resizable(false)
                .default_width(300.0)
                .show(ctx, |ui| {
                    ui.heading("Settings");
                    ui.separator();

                    ui.label("RPC Endpoint:");
                    ui.text_edit_singleline(&mut self.rpc_endpoint);

                    ui.label("WebSocket Endpoint:");
                    ui.text_edit_singleline(&mut self.ws_endpoint);

                    ui.checkbox(&mut self.auto_reconnect, "Auto Reconnect");

                    ui.separator();

                    ui.heading("Chart Settings");
                    // Chart settings would go here
                });
        }

        // Main content area
        CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                ViewTab::Chart => {
                    // Price tickers at the top
                    ui.horizontal_wrapped(|ui| {
                        for ticker in self.price_tickers.values() {
                            ui.group(|ui| {
                                ticker.show(ui);
                            });
                        }
                    });

                    ui.separator();

                    // Main chart
                    self.trading_chart.show(ui);
                }
                ViewTab::Orders => {
                    ui.vertical_centered(|ui| {
                        ui.heading("Orders & Trades");
                        ui.separator();
                        
                        // Market depth chart
                        self.market_depth.show(ui);
                        
                        ui.separator();
                        
                        // Orders table would go here
                        ui.label("Order book and trade history will be implemented here");
                    });
                }
                ViewTab::GraphArb => {
                    ui.vertical_centered(|ui| {
                        ui.heading("Arbitrage Graph");
                        ui.label("Multi-DEX arbitrage visualization will be implemented here");
                        
                        // This would show a graph of price differences across DEXes
                        // and potential arbitrage opportunities
                    });
                }
            }
        });
    }
}

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    env_logger::init(); // Initialize logging

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("Solana HFT Trading Terminal"),
        ..Default::default()
    };

    eframe::run_native(
        "Solana HFT Trading Terminal",
        options,
        Box::new(|_cc| {
            Ok(Box::new(MyApp::default()))
        }),
    )
}

