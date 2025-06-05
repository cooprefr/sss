// src/connection/websocket.rs

use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use solana_program::pubkey::Pubkey;
use tokio::sync::mpsc;
use std::collections::HashMap;
use crate::dex::whirlpool::state::Whirlpool;
use crate::data::price_data::{PricePoint, whirlpool_math};

pub struct SolanaWebSocketClient {
    sender: mpsc::UnboundedSender<WebSocketCommand>,
    receiver: mpsc::UnboundedReceiver<WhirlpoolUpdate>,
}

#[derive(Debug, Clone)]
pub enum WebSocketCommand {
    Subscribe(Pubkey),
    Unsubscribe(Pubkey),
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct WhirlpoolUpdate {
    pub pubkey: Pubkey,
    pub whirlpool: Whirlpool,
    pub timestamp: u64,
    pub slot: u64,
}

#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub decimals: u8,
    pub symbol: String,
    pub name: String,
}

impl SolanaWebSocketClient {
    pub async fn new(rpc_url: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (cmd_sender, mut cmd_receiver) = mpsc::unbounded_channel();
        let (update_sender, update_receiver) = mpsc::unbounded_channel();
        
        let rpc_url = rpc_url.to_string();
        tokio::spawn(async move {
            if let Err(e) = Self::websocket_task(rpc_url, cmd_receiver, update_sender).await {
                eprintln!("WebSocket task error: {}", e);
            }
        });

        Ok(Self {
            sender: cmd_sender,
            receiver: update_receiver,
        })
    }

    async fn websocket_task(
        rpc_url: String,
        mut cmd_receiver: mpsc::UnboundedReceiver<WebSocketCommand>,
        update_sender: mpsc::UnboundedSender<WhirlpoolUpdate>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (ws_stream, _) = connect_async(&rpc_url).await?;
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        
        let mut subscriptions: HashMap<Pubkey, u64> = HashMap::new();
        let mut subscription_id_counter = 1u64;

        loop {
            tokio::select! {
                // Handle commands from the main thread
                cmd = cmd_receiver.recv() => {
                    match cmd {
                        Some(WebSocketCommand::Subscribe(pubkey)) => {
                            let subscription_request = json!({
                                "jsonrpc": "2.0",
                                "id": subscription_id_counter,
                                "method": "accountSubscribe",
                                "params": [
                                    pubkey.to_string(),
                                    {
                                        "encoding": "base64",
                                        "commitment": "confirmed"
                                    }
                                ]
                            });

                            if let Ok(msg) = serde_json::to_string(&subscription_request) {
                                let _ = ws_sender.send(Message::Text(msg)).await;
                                subscriptions.insert(pubkey, subscription_id_counter);
                                subscription_id_counter += 1;
                            }
                        },
                        Some(WebSocketCommand::Unsubscribe(pubkey)) => {
                            if let Some(&sub_id) = subscriptions.get(&pubkey) {
                                let unsubscribe_request = json!({
                                    "jsonrpc": "2.0",
                                    "id": subscription_id_counter,
                                    "method": "accountUnsubscribe",
                                    "params": [sub_id]
                                });

                                if let Ok(msg) = serde_json::to_string(&unsubscribe_request) {
                                    let _ = ws_sender.send(Message::Text(msg)).await;
                                    subscriptions.remove(&pubkey);
                                    subscription_id_counter += 1;
                                }
                            }
                        },
                        Some(WebSocketCommand::Shutdown) | None => break,
                    }
                },
                
                // Handle incoming WebSocket messages
                msg = ws_receiver.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = Self::handle_websocket_message(
                                &text,
                                &subscriptions,
                                &update_sender
                            ).await {
                                eprintln!("Error handling WebSocket message: {}", e);
                            }
                        },
                        Some(Ok(Message::Close(_))) => {
                            println!("WebSocket connection closed");
                            break;
                        },
                        Some(Err(e)) => {
                            eprintln!("WebSocket error: {}", e);
                            break;
                        },
                        None => break,
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_websocket_message(
        text: &str,
        subscriptions: &HashMap<Pubkey, u64>,
        update_sender: &mpsc::UnboundedSender<WhirlpoolUpdate>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let msg: Value = serde_json::from_str(text)?;

        // Check if this is a subscription notification
        if let Some(params) = msg.get("params") {
            if let Some(result) = params.get("result") {
                if let Some(value) = result.get("value") {
                    if let (Some(data_str), Some(account_str)) = (
                        value.get("data").and_then(|d| d.get(0)).and_then(|s| s.as_str()),
                        value.get("pubkey").and_then(|s| s.as_str())
                    ) {
                        // Decode base64 data
                        let data = base64::decode(data_str)?;
                        let pubkey = account_str.parse::<Pubkey>()?;
                        
                        // Try to deserialize as Whirlpool
                        if let Ok(whirlpool) = Whirlpool::try_deserialize(&data) {
                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            
                            let slot = value.get("context")
                                .and_then(|c| c.get("slot"))
                                .and_then(|s| s.as_u64())
                                .unwrap_or(0);

                            let update = WhirlpoolUpdate {
                                pubkey,
                                whirlpool,
                                timestamp,
                                slot,
                            };

                            let _ = update_sender.send(update);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn subscribe(&self, pubkey: Pubkey) -> Result<(), mpsc::error::SendError<WebSocketCommand>> {
        self.sender.send(WebSocketCommand::Subscribe(pubkey))
    }

    pub fn unsubscribe(&self, pubkey: Pubkey) -> Result<(), mpsc::error::SendError<WebSocketCommand>> {
        self.sender.send(WebSocketCommand::Unsubscribe(pubkey))
    }

    pub async fn recv(&mut self) -> Option<WhirlpoolUpdate> {
        self.receiver.recv().await
    }

    pub fn try_recv(&mut self) -> Result<WhirlpoolUpdate, mpsc::error::TryRecvError> {
        self.receiver.try_recv()
    }
}

// HTTP client for initial data fetching and token metadata
pub struct SolanaHttpClient {
    client: reqwest::Client,
    rpc_url: String,
}

impl SolanaHttpClient {
    pub fn new(rpc_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            rpc_url,
        }
    }

    pub async fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                pubkey.to_string(),
                {
                    "encoding": "base64",
                    "commitment": "confirmed"
                }
            ]
        });

        let response = self.client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await?;

        let json: Value = response.json().await?;
        
        if let Some(result) = json.get("result") {
            if let Some(value) = result.get("value") {
                if let Some(data_array) = value.get("data").and_then(|d| d.as_array()) {
                    if let Some(data_str) = data_array.get(0).and_then(|s| s.as_str()) {
                        return Ok(base64::decode(data_str)?);
                    }
                }
            }
        }

        Err("Failed to get account data".into())
    }

    pub async fn get_whirlpool(&self, pubkey: &Pubkey) -> Result<Whirlpool, Box<dyn std::error::Error + Send + Sync>> {
        let data = self.get_account_data(pubkey).await?;
        Ok(Whirlpool::try_deserialize(&data)?)
    }

    pub async fn get_token_metadata(&self, mint: &Pubkey) -> Result<TokenInfo, Box<dyn std::error::Error + Send + Sync>> {
        // This would typically fetch from a token registry or mint account
        // For now, return defaults with common token info
        let token_info = match mint.to_string().as_str() {
            "So11111111111111111111111111111111111111112" => TokenInfo {
                decimals: 9,
                symbol: "SOL".to_string(),
                name: "Solana".to_string(),
            },
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" => TokenInfo {
                decimals: 6,
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
            },
            _ => {
                // Try to fetch from mint account
                // This is simplified - in reality you'd parse the mint account data
                TokenInfo {
                    decimals: 6, // Default
                    symbol: "UNK".to_string(),
                    name: "Unknown Token".to_string(),
                }
            }
        };

        Ok(token_info)
    }
}

// Add to Cargo.toml dependencies:
/*
[dependencies]
tokio-tungstenite = "0.20"
futures-util = "0.3"
serde_json = "1.0"
reqwest = { version = "0.11", features = ["json"] }
base64 = "0.21"
*/