use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use crate::restable::Restable;
use crate::structs::{
    ActiveCheckpoint, App, AppStatistics, Checkpoint, SessionCount, Timeline, TrackingStatus,
    WebSocketCommand, WebSocketMessage,
};
use tungstenite::{Message, accept};
use serde_json::Value as JsonValue;

// Helper function to convert JSON values to strings
fn json_value_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => String::new(),
        // For arrays and objects, serialize them to JSON strings
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

pub fn init_web_socket<T>(client: T)
where
    T: Restable + Sync + Send + 'static,
{
    let client_arc = Arc::new(RwLock::new(client));

    thread::spawn(move || {
        // Create a runtime for handling async operations
        let rt = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime,
            Err(e) => {
                error!("Failed to create async runtime: {}", e);
                return;
            }
        };

        let server = match TcpListener::bind("127.0.0.1:6754") {
            Ok(server) => server,
            Err(e) => {
                error!("Failed to bind WebSocket server: {}", e);
                return;
            }
        };
        info!("WebSocket server started on 127.0.0.1:6754");

        loop {
            match server.accept() {
                Ok((stream, _)) => {
                    let client_clone = Arc::clone(&client_arc);
                    let rt_clone = rt.handle().clone();
                    thread::spawn(move || {
                        let mut websocket = match accept(stream) {
                            Ok(ws) => ws,
                            Err(e) => {
                                error!("Failed to accept WebSocket connection: {}", e);
                                return;
                            }
                        };
                        info!("New WebSocket connection established");

                        loop {
                            let msg = match websocket.read() {
                                Ok(msg) => msg,
                                Err(e) => {
                                    error!("WebSocket error: {}", e);
                                    break;
                                }
                            };

                            if msg.is_close() {
                                info!("WebSocket connection closed");
                                break;
                            }

                            if !msg.is_text() && !msg.is_binary() {
                                // Ping/pong messages are handled automatically
                                continue;
                            }

                            // Handle the async message using our runtime
                            let response = rt_clone.block_on(async {
                                handle_message(&msg, &client_clone).await
                            });

                            let response_text = match response {
                                Ok(text) => text,
                                Err(e) => {
                                    let error_msg = WebSocketMessage::error(&format!(
                                        "Error processing request: {}",
                                        e
                                    ));
                                    match error_msg.to_json() {
                                        Ok(error_json) => error_json,
                                        Err(_) => continue,
                                    }
                                }
                            };

                            if let Err(e) = websocket.send(Message::text(response_text)) {
                                error!("Failed to send WebSocket response: {}", e);
                                break;
                            }
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    });
}

async fn handle_message<T>(
    msg: &Message,
    client: &Arc<RwLock<T>>,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let text = msg.to_text()?;
    let command = WebSocketCommand::from_json(text)?;

    match command {
        WebSocketCommand::GetApps(_) => handle_get_apps(client).await,
        WebSocketCommand::GetTimeline(payload) => handle_get_timeline(client, &payload).await,
        WebSocketCommand::GetAppByName(payload) => handle_get_app_by_name(client, &payload).await,
        WebSocketCommand::AddProcess(payload) => {
            // handle_add_process is sync, so we don't await it
            handle_add_process(client, &payload)
        },
        WebSocketCommand::DeleteProcess(payload) => {
            // handle_delete_process is sync, so we don't await it
            handle_delete_process(client, &payload)
        },
        WebSocketCommand::GetCheckpoints(payload) => handle_get_checkpoints(client, &payload).await,
        WebSocketCommand::CreateCheckpoint(payload) => handle_create_checkpoint(client, &payload).await,
        WebSocketCommand::SetActiveCheckpoint(payload) => {
            handle_set_active_checkpoint(client, &payload).await
        }
        WebSocketCommand::DeleteCheckpoint(payload) => handle_delete_checkpoint(client, &payload).await,
        WebSocketCommand::GetActiveCheckpoints(payload) => {
            handle_get_active_checkpoints(client, &payload).await
        }
        WebSocketCommand::GetTrackingStatus(payload) => {
            handle_get_tracking_status(client, &payload)
        }
        WebSocketCommand::GetSessionCounts(payload) => handle_get_session_counts(client, &payload).await,
        WebSocketCommand::GetStatistics(payload) => handle_get_statistics(client, &payload).await,
    }
}

async fn handle_get_apps<T>(client: &Arc<RwLock<T>>) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let apps_data = client_guard.get_all_apps().await?;
    let mut apps = Vec::new();

    if let Some(rows) = apps_data.as_array() {
        for row in rows {
            if let Some(obj) = row.as_object() {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), json_value_to_string(v)))
                    .collect();

                if let Some(app) = App::from_pg_row(&row_map) {
                    apps.push(app);
                }
            }
        }
    }

    let apps_json = serde_json::to_string(&apps)?;
    let response = WebSocketMessage::apps_list(&apps_json);

    Ok(response.to_json()?)
}

async fn handle_get_timeline<T>(
    client: &Arc<RwLock<T>>,
    _payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    // Get all timeline data without filtering (matching Dart implementation)
    let timeline_data = client_guard.get_all_timeline().await?;
    let associations_data = client_guard.get_timeline_checkpoint_associations().await?;

    // Build associations map
    let mut associations_map: HashMap<i32, Vec<i32>> = HashMap::new();
    if let Some(rows) = associations_data.as_array() {
        for row in rows {
            if let Some(obj) = row.as_object()
                && let (Some(timeline_id), Some(checkpoint_id)) = (
                    obj.get("timeline_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<i32>().ok()),
                    obj.get("checkpoint_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<i32>().ok()),
                )
            {
                associations_map
                    .entry(timeline_id)
                    .or_default()
                    .push(checkpoint_id);
            }
        }
    }

    let mut enhanced_timeline = Vec::new();

    if let Some(rows) = timeline_data.as_array() {
        for row in rows {
            if let Some(obj) = row.as_object() {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), json_value_to_string(v)))
                    .collect();

                if let Some(timeline_entry) = Timeline::from_pg_row(&row_map) {
                    // Create enhanced timeline with checkpoint associations
                    let mut timeline_json = serde_json::to_value(&timeline_entry)?;
                    if let Some(obj) = timeline_json.as_object_mut() {
                        let empty_vec = vec![];
                        let associations = associations_map
                            .get(&timeline_entry.id)
                            .unwrap_or(&empty_vec);
                        obj.insert(
                            "checkpoint_associations".to_string(),
                            serde_json::to_value(associations)?,
                        );
                    }
                    enhanced_timeline.push(timeline_json);
                }
            }
        }
    }

    let timeline_json = serde_json::to_string(&enhanced_timeline)?;
    let response = WebSocketMessage::timeline_data(&timeline_json);

    Ok(response.to_json()?)
}

async fn handle_get_app_by_name<T>(
    client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let app_name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("name parameter is required")?;

    let apps_data = client_guard.get_all_apps().await?;

    if let Some(rows) = apps_data.as_array() {
        for row in rows {
            if let Some(obj) = row.as_object() {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), json_value_to_string(v)))
                    .collect();

                if let Some(app) = App::from_pg_row(&row_map) {
                    let matches = app
                        .name
                        .as_ref()
                        .map(|name| name == app_name)
                        .unwrap_or(false);

                    if matches {
                        let app_json = serde_json::to_string(&app)?;
                        let response = WebSocketMessage::app(&app_json);
                        return Ok(response.to_json()?);
                    }
                }
            }
        }
    }

    let error_msg = WebSocketMessage::error("App not found");
    Ok(error_msg.to_json()?)
}

fn handle_add_process<T>(
    _client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let _process_name = params
        .get("process_name")
        .and_then(|v| v.as_str())
        .ok_or("process_name parameter is required")?;
    let _path = params
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("path parameter is required")?;

    // Since this is a sync function calling an async function, we need to handle it properly
    // For now, we'll return an error indicating this operation is not supported in this context
    // In a real implementation, this would need to be restructured
    let error_msg = WebSocketMessage::error("Add process operation not supported in this context");
    Ok(error_msg.to_json()?)
}

fn handle_delete_process<T>(
    _client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let _process_name = params
        .get("process_name")
        .and_then(|v| v.as_str())
        .ok_or("process_name parameter is required")?;

    // Since this is a sync function calling an async function, we need to handle it properly
    // For now, we'll return an error indicating this operation is not supported in this context
    // In a real implementation, this would need to be restructured
    let error_msg = WebSocketMessage::error("Delete process operation not supported in this context");
    Ok(error_msg.to_json()?)
}

async fn handle_get_checkpoints<T>(
    client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let checkpoints_data = if payload.is_empty() {
        client_guard.get_all_checkpoints().await?
    } else {
        let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
        if let Some(app_id) = params
            .get("app_id")
            .and_then(|v| v.as_i64())
            .map(|i| i as i32)
        {
            client_guard.get_checkpoints_for_app(app_id).await?
        } else {
            client_guard.get_all_checkpoints().await?
        }
    };

    let mut checkpoints = Vec::new();

    if let Some(rows) = checkpoints_data.as_array() {
        for row in rows {
            if let Some(obj) = row.as_object() {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), json_value_to_string(v)))
                    .collect();

                if let Some(checkpoint) = Checkpoint::from_pg_row(&row_map) {
                    checkpoints.push(checkpoint);
                }
            }
        }
    }

    let checkpoints_json = serde_json::to_string(&checkpoints)?;
    let response = WebSocketMessage::checkpoints_list(&checkpoints_json);

    Ok(response.to_json()?)
}

async fn handle_create_checkpoint<T>(
    client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("name parameter is required")?;
    let description = params.get("description").and_then(|v| v.as_str());
    let app_id = params
        .get("app_id")
        .and_then(|v| v.as_i64())
        .map(|i| i as i32)
        .ok_or("app_id is required for creating checkpoints")?;

    // Handle color parameter (default to #2196F3 if not provided)
    let _color = params
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("#2196F3");

    // Handle valid_from parameter (could be used in future)
    let _valid_from = params.get("valid_from").and_then(|v| v.as_str());

    match client_guard.create_checkpoint(name, description, app_id).await {
        Ok(_) => {
            // Get updated checkpoints for the app to return current state
            match client_guard.get_checkpoints_for_app(app_id).await {
                Ok(checkpoints_data) => {
                    let mut checkpoints = Vec::new();
                    if let Some(rows) = checkpoints_data.as_array() {
                        for row in rows {
                            if let Some(obj) = row.as_object() {
                                let row_map: HashMap<String, String> = obj
                                    .iter()
                                    .map(|(k, v)| (k.clone(), json_value_to_string(v)))
                                    .collect();

                                if let Some(checkpoint) = Checkpoint::from_pg_row(&row_map) {
                                    checkpoints.push(checkpoint);
                                }
                            }
                        }
                    }

                    let checkpoints_json = serde_json::to_string(&checkpoints)?;
                    let response = WebSocketMessage::checkpoints_list(&checkpoints_json);
                    Ok(response.to_json()?)
                }
                Err(_) => {
                    let response = WebSocketMessage::success(&format!(
                        "Successfully created checkpoint: {}",
                        name
                    ));
                    Ok(response.to_json()?)
                }
            }
        }
        Err(e) => {
            let error_msg =
                WebSocketMessage::error(&format!("Failed to create checkpoint '{}': {}", name, e));
            Ok(error_msg.to_json()?)
        }
    }
}

async fn handle_set_active_checkpoint<T>(
    client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let checkpoint_id = params
        .get("checkpoint_id")
        .and_then(|v| v.as_i64())
        .map(|i| i as i32)
        .ok_or("checkpoint_id parameter is required")?;
    let app_id = params
        .get("app_id")
        .and_then(|v| v.as_i64())
        .map(|i| i as i32)
        .ok_or("app_id parameter is required")?;
    let is_active = params
        .get("is_active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let result = if is_active {
        client_guard.activate_checkpoint(checkpoint_id, app_id).await
    } else {
        client_guard.deactivate_checkpoint(checkpoint_id, app_id).await
    };

    match result {
        Ok(_) => {
            let status = if is_active {
                "activated"
            } else {
                "deactivated"
            };
            let response = WebSocketMessage::success(&format!(
                "Successfully {} checkpoint with ID: {}",
                status, checkpoint_id
            ));
            Ok(response.to_json()?)
        }
        Err(e) => {
            let error_msg = WebSocketMessage::error(&format!(
                "Failed to update checkpoint with ID {}: {}",
                checkpoint_id, e
            ));
            Ok(error_msg.to_json()?)
        }
    }
}

async fn handle_delete_checkpoint<T>(
    client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let checkpoint_id = params
        .get("checkpoint_id")
        .and_then(|v| v.as_i64())
        .map(|i| i as i32)
        .ok_or("checkpoint_id parameter is required")?;

    match client_guard.delete_checkpoint(checkpoint_id).await {
        Ok(_) => {
            let response = WebSocketMessage::success(&format!(
                "Successfully deleted checkpoint with ID: {}",
                checkpoint_id
            ));
            Ok(response.to_json()?)
        }
        Err(e) => {
            let error_msg = WebSocketMessage::error(&format!(
                "Failed to delete checkpoint with ID {}: {}",
                checkpoint_id, e
            ));
            Ok(error_msg.to_json()?)
        }
    }
}

async fn handle_get_active_checkpoints<T>(
    client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let active_checkpoints_data = if payload.is_empty() {
        client_guard.get_all_active_checkpoints_table().await?
    } else {
        let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
        if let Some(app_id) = params
            .get("app_id")
            .and_then(|v| v.as_i64())
            .map(|i| i as i32)
        {
            client_guard.get_active_checkpoints_for_app_table(app_id).await?
        } else {
            client_guard.get_all_active_checkpoints_table().await?
        }
    };

    let mut active_checkpoints = Vec::new();

    if let Some(rows) = active_checkpoints_data.as_array() {
        for row in rows {
            if let Some(obj) = row.as_object() {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), json_value_to_string(v)))
                    .collect();

                if let Some(active_checkpoint) = ActiveCheckpoint::from_pg_row(&row_map) {
                    active_checkpoints.push(active_checkpoint);
                }
            }
        }
    }

    let checkpoints_json = serde_json::to_string(&active_checkpoints)?;
    let response = WebSocketMessage::active_checkpoints(&checkpoints_json);

    Ok(response.to_json()?)
}

fn handle_get_tracking_status<T>(
    _client: &Arc<RwLock<T>>,
    _payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    // For now, return a default tracking status
    // In a real implementation, this would track actual process states
    let status = TrackingStatus {
        is_tracking: false,
        is_paused: false,
        current_app: None,
        current_session_duration: 0,
        session_start_time: None,
        active_checkpoint_ids: vec![],
    };

    let status_json = serde_json::to_string(&status)?;
    let response = WebSocketMessage::tracking_status(&status_json);

    Ok(response.to_json()?)
}

async fn handle_get_session_counts<T>(
    client: &Arc<RwLock<T>>,
    _payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    // Get all app IDs and create session counts
    let app_ids = client_guard.get_all_app_ids().await?;
    // Use try_join_all to properly handle async operations in a loop
    let session_counts = futures_util::future::try_join_all(
        app_ids.into_iter().map(|app_id| async move {
            let client_guard = client.read()
                .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
            let session_count_value = client_guard.get_session_count_for_app(app_id).await?;
            Ok::<SessionCount, Box<dyn std::error::Error>>(SessionCount {
                app_id,
                session_count: session_count_value,
            })
        })
    ).await?;

    let counts_json = serde_json::to_string(&session_counts)?;
    let response = WebSocketMessage::session_counts_data(&counts_json);

    Ok(response.to_json()?)
}

async fn handle_get_statistics<T>(
    client: &Arc<RwLock<T>>,
    _payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    // Get all apps and create statistics
    let apps_data = client_guard.get_all_apps().await?;
    let mut app_statistics = Vec::new();

    if let Some(rows) = apps_data.as_array() {
        for row in rows {
            if let Some(obj) = row.as_object() {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), json_value_to_string(v)))
                    .collect();

                if let Some(app) = App::from_pg_row(&row_map) {
                    let statistics = AppStatistics {
                        app: app.clone(),
                        total_duration: app.duration.unwrap_or(0),
                        today_duration: 0, // Would need to calculate from timeline
                        week_duration: 0,  // Would need to calculate from timeline
                        month_duration: 0, // Would need to calculate from timeline
                        average_session_length: app.duration.unwrap_or(0) as f64
                            / app.launches.unwrap_or(1).max(1) as f64,
                        recent_sessions: vec![], // Would need to query timeline
                    };
                    app_statistics.push(statistics);
                }
            }
        }
    }

    let statistics_json = serde_json::to_string(&app_statistics)?;
    let response = WebSocketMessage::statistics_data(&statistics_json);

    Ok(response.to_json()?)
}
