use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock, Mutex};
use tokio::time::interval;
use tokio_tungstenite::{accept_async, WebSocketStream};
use tracing::{error, info, warn};

use crate::restable::Restable;
use crate::structs::{App, AppStatistics, Checkpoint, SessionCount, Timeline, TrackingStatus};

#[cfg(feature = "memory")]
use crate::time_tracking::TimeTrackingConfig;

type ClientId = u64;
type ClientSender = broadcast::Sender<ServerMessage>;
type ClientReceiver = broadcast::Receiver<ServerMessage>;
type StdRwLock<T> = std::sync::RwLock<T>;
type StdArc<T> = std::sync::Arc<T>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    TrackingStatusUpdate(TrackingStatus),
    AppsUpdate(Vec<crate::structs::App>),
    TimelineUpdate(Vec<crate::structs::Timeline>),
    Error { message: String, request_id: Option<String> },
    Response { data: serde_json::Value, request_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ClientMessage {
    #[serde(rename = "subscribe")]
    Subscribe { topics: Vec<String> },
    #[serde(rename = "unsubscribe")]
    Unsubscribe { topics: Vec<String> },
    #[serde(rename = "get_apps")]
    GetApps { request_id: Option<String> },
    #[serde(rename = "get_timeline")]
    GetTimeline { request_id: Option<String>, params: Option<serde_json::Value> },
    #[serde(rename = "get_app_by_name")]
    GetAppByName { request_id: Option<String>, name: String },
    #[serde(rename = "add_process")]
    AddProcess { request_id: Option<String>, process_name: String, path: String },
    #[serde(rename = "delete_process")]
    DeleteProcess { request_id: Option<String>, process_name: String },
    #[serde(rename = "get_tracking_status")]
    GetTrackingStatus { request_id: Option<String> },
    #[serde(rename = "get_checkpoints")]
    GetCheckpoints { request_id: Option<String>, app_id: Option<i32> },
    #[serde(rename = "create_checkpoint")]
    CreateCheckpoint { request_id: Option<String>, name: String, description: Option<String>, app_id: i32 },
    #[serde(rename = "set_active_checkpoint")]
    SetActiveCheckpoint { request_id: Option<String>, checkpoint_id: i32, is_active: bool },
    #[serde(rename = "delete_checkpoint")]
    DeleteCheckpoint { request_id: Option<String>, checkpoint_id: i32 },
    #[serde(rename = "get_active_checkpoints")]
    GetActiveCheckpoints { request_id: Option<String>, app_id: Option<i32> },
    #[serde(rename = "get_session_counts")]
    GetSessionCounts { request_id: Option<String> },
    #[serde(rename = "get_statistics")]
    GetStatistics { request_id: Option<String> },
    #[serde(rename = "get_checkpoint_stats")]
    GetCheckpointStats { request_id: Option<String>, checkpoint_id: i32 },
    #[cfg(feature = "memory")]
    #[serde(rename = "update_config")]
    UpdateConfig { tracking_delay_ms: u64, check_delay_ms: u64 },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug)]
pub struct ClientConnection {
    pub id: ClientId,
    pub addr: SocketAddr,
    pub subscriptions: Arc<RwLock<Vec<String>>>,
    pub last_ping: Arc<RwLock<std::time::Instant>>,
}

pub struct AsyncWebSocketServer<T: Restable> {
    listener: TcpListener,
    clients: Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
    broadcast_tx: broadcast::Sender<ServerMessage>,
    db_client: Arc<RwLock<T>>,
    client_counter: Arc<Mutex<ClientId>>,
    #[cfg(feature = "memory")]
    config: StdArc<StdRwLock<TimeTrackingConfig>>,
}

impl<T: Restable + Clone + Send + Sync + 'static> AsyncWebSocketServer<T> {
    pub async fn new(
        bind_addr: &str, 
        db_client: T,
        #[cfg(feature = "memory")] config: StdArc<StdRwLock<TimeTrackingConfig>>
    ) -> Result<Self> {
        let listener = TcpListener::bind(bind_addr)
            .await
            .with_context(|| format!("Failed to bind to {}", bind_addr))?;

        let (broadcast_tx, _) = broadcast::channel(1024);
        
        Ok(Self {
            listener,
            clients: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
            db_client: Arc::new(RwLock::new(db_client)),
            client_counter: Arc::new(Mutex::new(0)),
            #[cfg(feature = "memory")]
            config,
        })
    }

    pub async fn run(&self) -> Result<()> {
        info!("WebSocket server starting on {}", self.listener.local_addr()?);
        
        // Start background tasks
        self.start_heartbeat_checker().await;
        
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    let client_id = self.next_client_id().await;
                    info!("New connection from {} with id {}", addr, client_id);
                    
                    let connection = ClientConnection {
                        id: client_id,
                        addr,
                        subscriptions: Arc::new(RwLock::new(Vec::new())),
                        last_ping: Arc::new(RwLock::new(std::time::Instant::now())),
                    };
                    
                    self.clients.write().await.insert(client_id, connection);
                    
                    let clients = Arc::clone(&self.clients);
                    let broadcast_rx = self.broadcast_tx.subscribe();
                    let db_client = Arc::clone(&self.db_client);
                    let broadcast_tx = self.broadcast_tx.clone();
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(
                            stream, 
                            client_id, 
                            clients, 
                            broadcast_rx, 
                            broadcast_tx,
                            db_client
                        ).await {
                            error!("Client {} error: {}", client_id, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    async fn handle_client(
        stream: TcpStream,
        client_id: ClientId,
        clients: Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
        mut broadcast_rx: ClientReceiver,
        broadcast_tx: ClientSender,
        db_client: Arc<RwLock<T>>,
    ) -> Result<()> {
        let ws_stream = accept_async(stream)
            .await
            .context("Failed to accept WebSocket connection")?;

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Send initial tracking status
        let _db_guard = db_client.read().await;
        // Send initial tracking status using default for now
        // In a real implementation, you would fetch this from the database

        loop {
            tokio::select! {
                // Handle incoming messages from client
                msg = ws_receiver.next() => {
                    match msg {
                        Some(Ok(msg)) => {
                            if msg.is_close() {
                                info!("Client {} disconnected", client_id);
                                break;
                            }
                            
                            if let Ok(text) = msg.to_text() {
                                if let Err(e) = Self::handle_client_message(
                                    text, 
                                    client_id, 
                                    &clients, 
                                    &broadcast_tx, 
                                    &db_client,
                                    &mut ws_sender
                                ).await {
                                    error!("Error handling message from client {}: {}", client_id, e);
                                }
                            }
                        }
                        Some(Err(e)) => {
                            error!("WebSocket error for client {}: {}", client_id, e);
                            break;
                        }
                        None => break,
                    }
                }
                
                // Handle broadcast messages
                broadcast_msg = broadcast_rx.recv() => {
                    match broadcast_msg {
                        Ok(server_msg) => {
                            if Self::should_send_to_client(client_id, &server_msg, &clients).await {
                                let msg_json = serde_json::to_string(&server_msg)?;
                                if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await {
                                    error!("Failed to send broadcast to client {}: {}", client_id, e);
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!("Client {} lagged, skipped {} messages", client_id, skipped);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
                
                // Ping timeout
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    let ping_msg = tokio_tungstenite::tungstenite::Message::Ping(vec![]);
                    if ws_sender.send(ping_msg).await.is_err() {
                        break;
                    }
                }
            }
        }

        // Cleanup
        clients.write().await.remove(&client_id);
        info!("Client {} connection closed", client_id);
        Ok(())
    }

    async fn handle_client_message(
        message: &str,
        client_id: ClientId,
        clients: &Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
        broadcast_tx: &ClientSender,
        db_client: &Arc<RwLock<T>>,
        ws_sender: &mut futures_util::stream::SplitSink<WebSocketStream<TcpStream>, tokio_tungstenite::tungstenite::Message>,
    ) -> Result<()> {
        let client_msg: ClientMessage = serde_json::from_str(message)
            .with_context(|| format!("Failed to parse client message: {}", message))?;

        match client_msg {
            ClientMessage::Subscribe { topics } => {
                if let Some(client) = clients.read().await.get(&client_id) {
                    let mut subs = client.subscriptions.write().await;
                    for topic in topics {
                        if !subs.contains(&topic) {
                            subs.push(topic);
                        }
                    }
                }
            }

            ClientMessage::Unsubscribe { topics } => {
                if let Some(client) = clients.read().await.get(&client_id) {
                    let mut subs = client.subscriptions.write().await;
                    subs.retain(|t| !topics.contains(t));
                }
            }

            ClientMessage::GetApps { request_id } => {
                let apps = Self::get_apps_from_db(db_client).await?;
                let response = ServerMessage::Response {
                    data: serde_json::to_value(apps)?,
                    request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                };
                let msg_json = serde_json::to_string(&response)?;
                ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
            }

            ClientMessage::GetTimeline { request_id, params: _ } => {
                let timeline = Self::get_timeline_from_db(db_client).await?;
                let response = ServerMessage::Response {
                    data: serde_json::to_value(timeline)?,
                    request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                };
                let msg_json = serde_json::to_string(&response)?;
                ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
            }

            ClientMessage::GetTrackingStatus { request_id } => {
                let status = Self::get_tracking_status_from_db(db_client).await?;
                let response = ServerMessage::Response {
                    data: serde_json::to_value(status)?,
                    request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                };
                let msg_json = serde_json::to_string(&response)?;
                ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
            }

            ClientMessage::GetAppByName { request_id, name } => {
                match Self::get_app_by_name_from_db(db_client, &name).await {
                    Ok(app) => {
                        let response = ServerMessage::Response {
                            data: serde_json::to_value(app)?,
                            request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                        };
                        let msg_json = serde_json::to_string(&response)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                    Err(e) => {
                        let error_msg = ServerMessage::Error {
                            message: format!("Failed to get app by name: {}", e),
                            request_id,
                        };
                        let msg_json = serde_json::to_string(&error_msg)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                }
            }

            ClientMessage::AddProcess { request_id, process_name, path } => {
                match Self::add_process_to_db(db_client, &process_name, &path).await {
                    Ok(_) => {
                        let response = ServerMessage::Response {
                            data: serde_json::json!({"success": true, "message": "Process added"}),
                            request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                        };
                        let msg_json = serde_json::to_string(&response)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                        
                        // Broadcast apps update to subscribed clients
                        if let Ok(apps) = Self::get_apps_from_db(db_client).await {
                            let _ = broadcast_tx.send(ServerMessage::AppsUpdate(apps));
                        }
                    }
                    Err(e) => {
                        let error_msg = ServerMessage::Error {
                            message: format!("Failed to add process: {}", e),
                            request_id,
                        };
                        let msg_json = serde_json::to_string(&error_msg)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                }
            }

            ClientMessage::DeleteProcess { request_id, process_name } => {
                match Self::delete_process_from_db(db_client, &process_name).await {
                    Ok(_) => {
                        let response = ServerMessage::Response {
                            data: serde_json::json!({"success": true, "message": "Process deleted"}),
                            request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                        };
                        let msg_json = serde_json::to_string(&response)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                        
                        // Broadcast apps update to subscribed clients
                        if let Ok(apps) = Self::get_apps_from_db(db_client).await {
                            let _ = broadcast_tx.send(ServerMessage::AppsUpdate(apps));
                        }
                    }
                    Err(e) => {
                        let error_msg = ServerMessage::Error {
                            message: format!("Failed to delete process: {}", e),
                            request_id,
                        };
                        let msg_json = serde_json::to_string(&error_msg)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                }
            }

            ClientMessage::GetCheckpoints { request_id, app_id } => {
                match Self::get_checkpoints_from_db(db_client, app_id).await {
                    Ok(checkpoints) => {
                        let response = ServerMessage::Response {
                            data: serde_json::to_value(checkpoints)?,
                            request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                        };
                        let msg_json = serde_json::to_string(&response)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                    Err(e) => {
                        let error_msg = ServerMessage::Error {
                            message: format!("Failed to get checkpoints: {}", e),
                            request_id,
                        };
                        let msg_json = serde_json::to_string(&error_msg)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                }
            }

            ClientMessage::CreateCheckpoint { request_id, name, description, app_id } => {
                match Self::create_checkpoint_in_db(db_client, &name, description.as_deref(), app_id).await {
                    Ok(_) => {
                        let response = ServerMessage::Response {
                            data: serde_json::json!({"success": true, "message": "Checkpoint created"}),
                            request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                        };
                        let msg_json = serde_json::to_string(&response)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                    Err(e) => {
                        let error_msg = ServerMessage::Error {
                            message: format!("Failed to create checkpoint: {}", e),
                            request_id,
                        };
                        let msg_json = serde_json::to_string(&error_msg)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                }
            }

            ClientMessage::SetActiveCheckpoint { request_id, checkpoint_id, is_active } => {
                match Self::set_checkpoint_active_in_db(db_client, checkpoint_id, is_active).await {
                    Ok(_) => {
                        let response = ServerMessage::Response {
                            data: serde_json::json!({"success": true, "message": "Checkpoint status updated"}),
                            request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                        };
                        let msg_json = serde_json::to_string(&response)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                    Err(e) => {
                        let error_msg = ServerMessage::Error {
                            message: format!("Failed to update checkpoint: {}", e),
                            request_id,
                        };
                        let msg_json = serde_json::to_string(&error_msg)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                }
            }

            ClientMessage::DeleteCheckpoint { request_id, checkpoint_id } => {
                match Self::delete_checkpoint_from_db(db_client, checkpoint_id).await {
                    Ok(_) => {
                        let response = ServerMessage::Response {
                            data: serde_json::json!({"success": true, "message": "Checkpoint deleted"}),
                            request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                        };
                        let msg_json = serde_json::to_string(&response)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                    Err(e) => {
                        let error_msg = ServerMessage::Error {
                            message: format!("Failed to delete checkpoint: {}", e),
                            request_id,
                        };
                        let msg_json = serde_json::to_string(&error_msg)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                }
            }

            ClientMessage::GetActiveCheckpoints { request_id, app_id } => {
                match Self::get_active_checkpoints_from_db(db_client, app_id).await {
                    Ok(checkpoints) => {
                        let response = ServerMessage::Response {
                            data: serde_json::to_value(checkpoints)?,
                            request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                        };
                        let msg_json = serde_json::to_string(&response)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                    Err(e) => {
                        let error_msg = ServerMessage::Error {
                            message: format!("Failed to get active checkpoints: {}", e),
                            request_id,
                        };
                        let msg_json = serde_json::to_string(&error_msg)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                }
            }

            ClientMessage::GetSessionCounts { request_id } => {
                match Self::get_session_counts_from_db(db_client).await {
                    Ok(session_counts) => {
                        let response = ServerMessage::Response {
                            data: serde_json::to_value(session_counts)?,
                            request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                        };
                        let msg_json = serde_json::to_string(&response)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                    Err(e) => {
                        let error_msg = ServerMessage::Error {
                            message: format!("Failed to get session counts: {}", e),
                            request_id,
                        };
                        let msg_json = serde_json::to_string(&error_msg)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                }
            }

            ClientMessage::GetStatistics { request_id } => {
                match Self::get_statistics_from_db(db_client).await {
                    Ok(statistics) => {
                        let response = ServerMessage::Response {
                            data: serde_json::to_value(statistics)?,
                            request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                        };
                        let msg_json = serde_json::to_string(&response)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                    Err(e) => {
                        let error_msg = ServerMessage::Error {
                            message: format!("Failed to get statistics: {}", e),
                            request_id,
                        };
                        let msg_json = serde_json::to_string(&error_msg)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                }
            }

            ClientMessage::GetCheckpointStats { request_id, checkpoint_id } => {
                match Self::get_checkpoint_stats_from_db(db_client, checkpoint_id).await {
                    Ok(stats) => {
                        let response = ServerMessage::Response {
                            data: serde_json::to_value(stats)?,
                            request_id: request_id.unwrap_or_else(|| "unknown".to_string()),
                        };
                        let msg_json = serde_json::to_string(&response)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                    Err(e) => {
                        let error_msg = ServerMessage::Error {
                            message: format!("Failed to get checkpoint stats: {}", e),
                            request_id,
                        };
                        let msg_json = serde_json::to_string(&error_msg)?;
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
                    }
                }
            }

            #[cfg(feature = "memory")]
            ClientMessage::UpdateConfig { tracking_delay_ms: _, check_delay_ms: _ } => {
                // Update configuration (TODO: Implement actual configuration update)
                // Note: This doesn't have a request_id in the new system
                // We'll send a success response without request_id for backward compatibility
                let response = ServerMessage::Response {
                    data: serde_json::json!({"success": true, "message": "Configuration updated"}),
                    request_id: "config_update".to_string(), // Default request ID
                };
                let msg_json = serde_json::to_string(&response)?;
                ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(msg_json)).await?;
            }

            ClientMessage::Ping => {
                if let Some(client) = clients.read().await.get(&client_id) {
                    *client.last_ping.write().await = std::time::Instant::now();
                }
                let pong_msg = tokio_tungstenite::tungstenite::Message::Pong(vec![]);
                ws_sender.send(pong_msg).await?;
            }
        }

        Ok(())
    }

    async fn should_send_to_client(
        client_id: ClientId,
        message: &ServerMessage,
        clients: &Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
    ) -> bool {
        let clients_guard = clients.read().await;
        let Some(client) = clients_guard.get(&client_id) else {
            return false;
        };

        let subscriptions = client.subscriptions.read().await;
        
        match message {
            ServerMessage::TrackingStatusUpdate(_) => {
                subscriptions.contains(&"tracking_status".to_string())
            }
            ServerMessage::AppsUpdate(_) => {
                subscriptions.contains(&"apps".to_string())
            }
            ServerMessage::TimelineUpdate(_) => {
                subscriptions.contains(&"timeline".to_string())
            }
            ServerMessage::Error { .. } | ServerMessage::Response { .. } => true,
        }
    }

    async fn start_heartbeat_checker(&self) {
        let clients = Arc::clone(&self.clients);
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                
                let mut to_remove = Vec::new();
                {
                    let clients_guard = clients.read().await;
                    let now = std::time::Instant::now();
                    
                    for (client_id, client) in clients_guard.iter() {
                        let last_ping = *client.last_ping.read().await;
                        if now.duration_since(last_ping) > Duration::from_secs(120) {
                            to_remove.push(*client_id);
                        }
                    }
                }
                
                if !to_remove.is_empty() {
                    let mut clients_guard = clients.write().await;
                    for client_id in to_remove {
                        clients_guard.remove(&client_id);
                        info!("Removed stale client: {}", client_id);
                    }
                }
            }
        });
    }

    async fn next_client_id(&self) -> ClientId {
        let mut counter = self.client_counter.lock().await;
        *counter += 1;
        *counter
    }

    // Utility functions for data conversion (adapted from handlers.rs)
    fn json_value_to_string(value: &JsonValue) -> String {
        match value {
            JsonValue::String(s) => s.clone(),
            JsonValue::Number(n) => n.to_string(),
            JsonValue::Bool(b) => b.to_string(),
            JsonValue::Null => String::new(),
            _ => serde_json::to_string(value).unwrap_or_default(),
        }
    }

    fn convert_json_rows_to_apps(rows: &[JsonValue]) -> Vec<App> {
        rows.iter()
            .filter_map(|row| row.as_object())
            .filter_map(|obj| {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::json_value_to_string(v)))
                    .collect();
                App::from_pg_row(&row_map)
            })
            .collect()
    }

    fn convert_json_rows_to_checkpoints(rows: &[JsonValue]) -> Vec<Checkpoint> {
        rows.iter()
            .filter_map(|row| row.as_object())
            .filter_map(|obj| {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::json_value_to_string(v)))
                    .collect();
                Checkpoint::from_pg_row(&row_map)
            })
            .collect()
    }

    fn convert_json_rows_to_timeline(rows: &[JsonValue]) -> Vec<Timeline> {
        rows.iter()
            .filter_map(|row| row.as_object())
            .filter_map(|obj| {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::json_value_to_string(v)))
                    .collect();
                Timeline::from_pg_row(&row_map)
            })
            .collect()
    }

    fn calculate_duration_statistics(timeline_entries: &[Timeline]) -> (i32, i32, i32, i32, i32) {
        let today = chrono::Utc::now().naive_utc().date();
        let week_ago = today - chrono::Duration::days(7);
        let month_ago = today - chrono::Duration::days(30);

        timeline_entries.iter().fold(
            (0, 0, 0, 0, 0),
            |(total, count, today_sum, week_sum, month_sum), entry| {
                let duration = entry.duration.unwrap_or(0);
                let today_increment = entry.date
                    .filter(|&date| date == today)
                    .map(|_| duration)
                    .unwrap_or(0);
                let week_increment = entry.date
                    .filter(|&date| date >= week_ago)
                    .map(|_| duration)
                    .unwrap_or(0);
                let month_increment = entry.date
                    .filter(|&date| date >= month_ago)
                    .map(|_| duration)
                    .unwrap_or(0);

                (
                    total + duration,
                    count + 1,
                    today_sum + today_increment,
                    week_sum + week_increment,
                    month_sum + month_increment,
                )
            },
        )
    }

    async fn calculate_app_statistics(
        db_client: &Arc<RwLock<T>>,
        app: &App,
    ) -> Result<AppStatistics> {
        let db_guard = db_client.read().await;
        let timeline_data = db_guard
            .get_timeline_data(app.name.as_ref().map(|s| s.as_str()), 30)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get timeline data: {}", e))?;

        let timeline_entries = timeline_data
            .as_array()
            .map(|rows| Self::convert_json_rows_to_timeline(rows))
            .unwrap_or_default();

        let recent_sessions = timeline_entries.iter().take(10).cloned().collect();
        let (total_duration, session_count, today_duration, week_duration, month_duration) =
            Self::calculate_duration_statistics(&timeline_entries);

        let average_session_length = if session_count > 0 {
            total_duration as f64 / session_count as f64
        } else {
            0.0
        };

        Ok(AppStatistics {
            app: app.clone(),
            total_duration: app.duration.unwrap_or(0),
            today_duration,
            week_duration,
            month_duration,
            average_session_length,
            recent_sessions,
        })
    }

    // Database operations using existing Restable methods (adapted from handlers.rs)
    async fn get_apps_from_db(db_client: &Arc<RwLock<T>>) -> Result<Vec<crate::structs::App>> {
        let db_guard = db_client.read().await;
        let apps_data = db_guard.get_all_apps().await
            .map_err(|e| anyhow::anyhow!("Failed to get apps from database: {}", e))?;
        
        let apps = apps_data.as_array()
            .ok_or_else(|| anyhow::anyhow!("Apps data is not an array"))?
            .iter()
            .filter_map(|row| row.as_object())
            .filter_map(|obj| {
                let row_map: std::collections::HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::json_value_to_string(v)))
                    .collect();
                crate::structs::App::from_pg_row(&row_map)
            })
            .collect();
        
        Ok(apps)
    }

    async fn get_timeline_from_db(db_client: &Arc<RwLock<T>>) -> Result<Vec<crate::structs::Timeline>> {
        let db_guard = db_client.read().await;
        let timeline_data = db_guard.get_timeline_with_checkpoints().await
            .map_err(|e| anyhow::anyhow!("Failed to get timeline from database: {}", e))?;
        
        let timeline = timeline_data.as_array()
            .ok_or_else(|| anyhow::anyhow!("Timeline data is not an array"))?
            .iter()
            .filter_map(|row| row.as_object())
            .filter_map(|obj| {
                let row_map: std::collections::HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::json_value_to_string(v)))
                    .collect();
                crate::structs::Timeline::from_pg_row(&row_map)
            })
            .collect();
        
        Ok(timeline)
    }

    async fn get_tracking_status_from_db(_db_client: &Arc<RwLock<T>>) -> Result<TrackingStatus> {
        // Get tracking status from the tracking service if available
        if let Some(service) = super::tracking_service::get_global_tracking_service() {
            Ok(service.get_legacy_status())
        } else {
            // Fallback to default status if service not initialized
            Ok(TrackingStatus {
                is_tracking: false,
                is_paused: false,
                current_app: None,
                current_session_duration: 0,
                session_start_time: None,
                active_checkpoint_ids: vec![],
            })
        }
    }

    async fn add_process_to_db(db_client: &Arc<RwLock<T>>, process_name: &str, path: &str) -> Result<()> {
        // Use the db_client directly for adding processes
        let db_guard = db_client.read().await;
        
        // Check if process already exists
        let processes = db_guard.get_processes().await
            .map_err(|e| anyhow::anyhow!("Failed to get processes: {}", e))?;
        if processes.contains(&process_name.to_string()) {
            return Err(anyhow::anyhow!("Process '{}' has already been added before.", process_name));
        }
        
        // Get product name (simplified - using process name as fallback)
        let product_name = crate::native::ver_query_value(path).unwrap_or_else(|| process_name.to_string());
        
        // Add the process
        db_guard.put_data(process_name, &product_name).await
            .map_err(|e| anyhow::anyhow!("Failed to put data: {}", e))?;
        
        // Update the process map (from time_tracking.rs)
        {
            use crate::time_tracking::PROCESS_MAP;
            PROCESS_MAP.lock().unwrap().insert(process_name.to_owned(), (false, false));
        }
        
        info!("Process '{}' has been added.", process_name);
        Ok(())
    }

    async fn delete_process_from_db(db_client: &Arc<RwLock<T>>, process_name: &str) -> Result<()> {
        // Use the db_client directly for deleting processes
        let db_guard = db_client.read().await;
        
        // Delete the process from database
        db_guard.delete_data(process_name).await
            .map_err(|e| anyhow::anyhow!("Failed to delete data: {}", e))?;
        
        // Update the process map (from time_tracking.rs)
        {
            use crate::time_tracking::PROCESS_MAP;
            PROCESS_MAP.lock().unwrap().remove(process_name);
        }
        
        info!("Process '{}' has been deleted.", process_name);
        Ok(())
    }

    async fn get_app_by_name_from_db(db_client: &Arc<RwLock<T>>, name: &str) -> Result<App> {
        let db_guard = db_client.read().await;
        let apps_data = db_guard.get_all_apps().await
            .map_err(|e| anyhow::anyhow!("Failed to get apps from database: {}", e))?;
        
        let apps = Self::convert_json_rows_to_apps(
            apps_data.as_array()
                .ok_or_else(|| anyhow::anyhow!("Apps data is not an array"))?
        );
        
        apps.into_iter()
            .find(|app| app.name.as_deref() == Some(name))
            .ok_or_else(|| anyhow::anyhow!("App '{}' not found", name))
    }

    async fn get_checkpoints_from_db(db_client: &Arc<RwLock<T>>, app_id: Option<i32>) -> Result<Vec<Checkpoint>> {
        let db_guard = db_client.read().await;
        let checkpoints_data = match app_id {
            Some(id) => db_guard.get_checkpoints_for_app(id).await,
            None => db_guard.get_all_checkpoints().await,
        }.map_err(|e| anyhow::anyhow!("Failed to get checkpoints: {}", e))?;
        
        let checkpoints = Self::convert_json_rows_to_checkpoints(
            checkpoints_data.as_array()
                .ok_or_else(|| anyhow::anyhow!("Checkpoints data is not an array"))?
        );
        
        Ok(checkpoints)
    }

    async fn create_checkpoint_in_db(db_client: &Arc<RwLock<T>>, name: &str, description: Option<&str>, app_id: i32) -> Result<()> {
        let db_guard = db_client.read().await;
        db_guard.create_checkpoint(name, description, app_id).await
            .map_err(|e| anyhow::anyhow!("Failed to create checkpoint: {}", e))?;
        Ok(())
    }

    async fn set_checkpoint_active_in_db(db_client: &Arc<RwLock<T>>, checkpoint_id: i32, is_active: bool) -> Result<()> {
        let db_guard = db_client.read().await;
        db_guard.set_checkpoint_active(checkpoint_id, is_active).await
            .map_err(|e| anyhow::anyhow!("Failed to update checkpoint status: {}", e))?;
        Ok(())
    }

    async fn delete_checkpoint_from_db(db_client: &Arc<RwLock<T>>, checkpoint_id: i32) -> Result<()> {
        let db_guard = db_client.read().await;
        db_guard.delete_checkpoint(checkpoint_id).await
            .map_err(|e| anyhow::anyhow!("Failed to delete checkpoint: {}", e))?;
        Ok(())
    }

    async fn get_active_checkpoints_from_db(db_client: &Arc<RwLock<T>>, app_id: Option<i32>) -> Result<Vec<Checkpoint>> {
        let db_guard = db_client.read().await;
        let checkpoints_data = match app_id {
            Some(id) => db_guard.get_active_checkpoints_for_app(id).await,
            None => db_guard.get_active_checkpoints().await,
        }.map_err(|e| anyhow::anyhow!("Failed to get active checkpoints: {}", e))?;
        
        let checkpoints = Self::convert_json_rows_to_checkpoints(
            checkpoints_data.as_array()
                .ok_or_else(|| anyhow::anyhow!("Active checkpoints data is not an array"))?
        );
        
        Ok(checkpoints)
    }

    async fn get_session_counts_from_db(db_client: &Arc<RwLock<T>>) -> Result<Vec<SessionCount>> {
        let db_guard = db_client.read().await;
        let app_ids = db_guard.get_all_app_ids().await
            .map_err(|e| anyhow::anyhow!("Failed to get app IDs: {}", e))?;
        
        let mut session_counts = Vec::new();
        for app_id in app_ids {
            let count = db_guard.get_session_count_for_app(app_id).await
                .map_err(|e| anyhow::anyhow!("Failed to get session count for app {}: {}", app_id, e))?;
            session_counts.push(SessionCount { app_id, session_count: count });
        }
        
        Ok(session_counts)
    }

    async fn get_statistics_from_db(db_client: &Arc<RwLock<T>>) -> Result<Vec<AppStatistics>> {
        let apps = Self::get_apps_from_db(db_client).await?;
        let mut statistics = Vec::new();
        
        for app in apps {
            match Self::calculate_app_statistics(db_client, &app).await {
                Ok(stats) => statistics.push(stats),
                Err(e) => {
                    error!("Failed to calculate statistics for app '{}': {}", 
                           app.name.as_deref().unwrap_or("unknown"), e);
                }
            }
        }
        
        Ok(statistics)
    }

    async fn get_checkpoint_stats_from_db(db_client: &Arc<RwLock<T>>, checkpoint_id: i32) -> Result<serde_json::Value> {
        let db_guard = db_client.read().await;
        let durations_data = db_guard.get_checkpoint_durations_by_ids(&[checkpoint_id]).await
            .map_err(|e| anyhow::anyhow!("Failed to get checkpoint durations: {}", e))?;
        
        let durations = Self::convert_json_rows_to_checkpoints(
            durations_data.as_array()
                .ok_or_else(|| anyhow::anyhow!("Checkpoint durations data is not an array"))?
        );
        
        Ok(serde_json::json!({
            "checkpoint_id": checkpoint_id,
            "durations": durations
        }))
    }

    pub fn broadcast_tracking_status(&self, status: TrackingStatus) -> Result<()> {
        self.broadcast_tx.send(ServerMessage::TrackingStatusUpdate(status))
            .map_err(|_| anyhow::anyhow!("No active receivers"))?;
        Ok(())
    }

    pub fn broadcast_apps_update(&self, apps: Vec<crate::structs::App>) -> Result<()> {
        self.broadcast_tx.send(ServerMessage::AppsUpdate(apps))
            .map_err(|_| anyhow::anyhow!("No active receivers"))?;
        Ok(())
    }
}
