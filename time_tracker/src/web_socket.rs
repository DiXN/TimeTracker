use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use crate::restable::Restable;
use crate::structs::{
    App, AppStatistics, Checkpoint, SessionCount, Timeline, TrackingStatus, WebSocketCommand,
    WebSocketMessage,
};
#[cfg(feature = "memory")]
use crate::time_tracking::TimeTrackingConfig;

use crate::time_tracking::{add_process, delete_process};
use log::{error, info};
use serde_json::Value as JsonValue;
use tungstenite::{Message, accept};

// Server state that holds both the client and tracking status
pub struct ServerState<T: Restable> {
    pub client: Arc<RwLock<T>>,
    tracking_status: TrackingStatus,
    #[cfg(feature = "memory")]
    config: Arc<RwLock<TimeTrackingConfig>>,
}

impl<T: Restable> ServerState<T> {
    fn new(client: T, #[cfg(feature = "memory")] config: Arc<RwLock<TimeTrackingConfig>>) -> Self {
        Self {
            client: Arc::new(RwLock::new(client)),
            tracking_status: TrackingStatus {
                is_tracking: false,
                is_paused: false,
                current_app: None,
                current_session_duration: 0,
                session_start_time: None,
                active_checkpoint_ids: vec![],
            },
            #[cfg(feature = "memory")]
            config,
        }
    }

}

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

pub fn init_web_socket<T>(client: T, #[cfg(feature = "memory")] config: Arc<RwLock<TimeTrackingConfig>>)
where
    T: Restable + Sync + Send + 'static,
{
    let server_state = ServerState::new(client, #[cfg(feature = "memory")] config);
    let state_arc = Arc::new(RwLock::new(server_state));

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
                    let state_clone = Arc::clone(&state_arc);
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
                            let response = rt_clone
                                .block_on(async { handle_message(&msg, &state_clone).await });

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
    state: &Arc<RwLock<ServerState<T>>>,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let text = msg.to_text()?;
    let command = WebSocketCommand::from_json(text)?;

    match command {
        WebSocketCommand::GetApps(_) => handle_get_apps(state).await,
        WebSocketCommand::GetTimeline(payload) => handle_get_timeline(state, &payload).await,
        WebSocketCommand::GetAppByName(payload) => handle_get_app_by_name(state, &payload).await,
        WebSocketCommand::AddProcess(payload) => handle_add_process(state, &payload).await,
        WebSocketCommand::DeleteProcess(payload) => handle_delete_process(state, &payload).await,
        WebSocketCommand::GetCheckpoints(payload) => handle_get_checkpoints(state, &payload).await,
        WebSocketCommand::CreateCheckpoint(payload) => {
            handle_create_checkpoint(state, &payload).await
        }
        WebSocketCommand::SetActiveCheckpoint(payload) => {
            handle_set_active_checkpoint(state, &payload).await
        }
        WebSocketCommand::DeleteCheckpoint(payload) => {
            handle_delete_checkpoint(state, &payload).await
        }
        WebSocketCommand::GetActiveCheckpoints(payload) => {
            handle_get_active_checkpoints(state, &payload).await
        }
        WebSocketCommand::GetTrackingStatus(payload) => handle_get_tracking_status(state, &payload),
        WebSocketCommand::GetSessionCounts(payload) => {
            handle_get_session_counts(state, &payload).await
        }
        WebSocketCommand::GetStatistics(payload) => handle_get_statistics(state, &payload).await,
        WebSocketCommand::GetCheckpointStats(payload) => {
            handle_get_checkpoint_stats(state, &payload).await
        }
        #[cfg(feature = "memory")]
        WebSocketCommand::UpdateConfig(payload) => {
            handle_update_config(state, &payload).await
        }
    }
}

async fn handle_get_apps<T>(
    state: &Arc<RwLock<ServerState<T>>>,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let client_guard = state_guard.client.read()
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
    state: &Arc<RwLock<ServerState<T>>>,
    _payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let client_guard = state_guard.client.read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    // Get all timeline data without filtering (matching Dart implementation)
    let timeline_data = client_guard.get_timeline_with_checkpoints().await?;

    let timeline_json = serde_json::to_string(&timeline_data)?;
    let response = WebSocketMessage::timeline_data(&timeline_json);

    Ok(response.to_json()?)
}

async fn handle_get_app_by_name<T>(
    state: &Arc<RwLock<ServerState<T>>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let client_guard = state_guard.client.read()
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

async fn handle_add_process<T>(
    state: &Arc<RwLock<ServerState<T>>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let process_name = params
        .get("process_name")
        .and_then(|v| v.as_str())
        .ok_or("process_name parameter is required")?;
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("path parameter is required")?;

    // Clone the Arc<RwLock<T>> as required by add_process
    let client_arc = Arc::clone(&state_guard.client);

    match add_process(process_name, path, &client_arc).await {
        Ok(_) => {
            let response =
                WebSocketMessage::success(&format!("Successfully added process: {}", process_name));
            Ok(response.to_json()?)
        }
        Err(e) => {
            let error_msg = WebSocketMessage::error(&format!(
                "Failed to add process '{}': {}",
                process_name, e
            ));
            Ok(error_msg.to_json()?)
        }
    }
}

async fn handle_delete_process<T>(
    state: &Arc<RwLock<ServerState<T>>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let process_name = params
        .get("process_name")
        .and_then(|v| v.as_str())
        .ok_or("process_name parameter is required")?;

    // Clone the Arc<RwLock<T>> as required by delete_process
    let client_arc = Arc::clone(&state_guard.client);

    match delete_process(process_name, &client_arc).await {
        Ok(_) => {
            let response = WebSocketMessage::success(&format!(
                "Successfully deleted process: {}",
                process_name
            ));
            Ok(response.to_json()?)
        }
        Err(e) => {
            let error_msg = WebSocketMessage::error(&format!(
                "Failed to delete process '{}': {}",
                process_name, e
            ));
            Ok(error_msg.to_json()?)
        }
    }
}

async fn handle_get_checkpoints<T>(
    state: &Arc<RwLock<ServerState<T>>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let client_guard = state_guard.client.read()
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
    state: &Arc<RwLock<ServerState<T>>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let client_guard = state_guard.client.read()
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

    match client_guard
        .create_checkpoint(name, description, app_id)
        .await
    {
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
    state: &Arc<RwLock<ServerState<T>>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let client_guard = state_guard.client.read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let checkpoint_id = params
        .get("checkpoint_id")
        .and_then(|v| v.as_i64())
        .map(|i| i as i32)
        .ok_or("checkpoint_id parameter is required")?;
    let is_active = params
        .get("is_active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let result = if is_active {
        client_guard
            .set_checkpoint_active(checkpoint_id, true)
            .await
    } else {
        client_guard
            .set_checkpoint_active(checkpoint_id, false)
            .await
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
    state: &Arc<RwLock<ServerState<T>>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let client_guard = state_guard.client.read()
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
    state: &Arc<RwLock<ServerState<T>>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let client_guard = state_guard.client.read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let active_checkpoints_data = if payload.is_empty() {
        client_guard.get_active_checkpoints().await?
    } else {
        let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
        if let Some(app_id) = params
            .get("app_id")
            .and_then(|v| v.as_i64())
            .map(|i| i as i32)
        {
            client_guard.get_active_checkpoints_for_app(app_id).await?
        } else {
            client_guard.get_active_checkpoints().await?
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

                if let Some(checkpoint) = Checkpoint::from_pg_row(&row_map) {
                    active_checkpoints.push(checkpoint);
                }
            }
        }
    }

    let checkpoints_json = serde_json::to_string(&active_checkpoints)?;
    let response = WebSocketMessage::active_checkpoints(&checkpoints_json);

    Ok(response.to_json()?)
}

fn handle_get_tracking_status<T>(
    state: &Arc<RwLock<ServerState<T>>>,
    _payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let status = &state_guard.tracking_status;

    let status_json = serde_json::to_string(&status)?;
    let response = WebSocketMessage::tracking_status(&status_json);

    Ok(response.to_json()?)
}

async fn handle_get_session_counts<T>(
    state: &Arc<RwLock<ServerState<T>>>,
    _payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let client_guard = state_guard.client.read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    // Get all app IDs and create session counts
    let app_ids = client_guard.get_all_app_ids().await?;
    // Use try_join_all to properly handle async operations in a loop
    let session_counts =
        futures_util::future::try_join_all(app_ids.into_iter().map(|app_id| async move {
            let state_guard = state
                .read()
                .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
            let client_guard = state_guard.client.read()
                .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
            let session_count_value = client_guard.get_session_count_for_app(app_id).await?;
            Ok::<SessionCount, Box<dyn std::error::Error>>(SessionCount {
                app_id,
                session_count: session_count_value,
            })
        }))
        .await?;

    let counts_json = serde_json::to_string(&session_counts)?;
    let response = WebSocketMessage::session_counts_data(&counts_json);

    Ok(response.to_json()?)
}

async fn handle_get_statistics<T>(
    state: &Arc<RwLock<ServerState<T>>>,
    _payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let client_guard = state_guard.client.read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    // Get all apps and create statistics
    let apps_data = client_guard.get_all_apps().await?;
    let mut app_statistics = Vec::new();

    if let Some(rows) = apps_data.as_array() {
        // Get today's date for comparison (outside the loop for efficiency)
        let today = chrono::Utc::now().naive_utc().date();
        let week_ago = today - chrono::Duration::days(7);
        let month_ago = today - chrono::Duration::days(30);

        for row in rows {
            if let Some(obj) = row.as_object() {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), json_value_to_string(v)))
                    .collect();

                if let Some(app) = App::from_pg_row(&row_map) {
                    // Get timeline data for this app for the last 30 days
                    let timeline_data = client_guard
                        .get_timeline_data(
                            app.name.as_ref().map(|s| s.as_str()),
                            30, // Get data for the last 30 days
                        )
                        .await?;

                    // Process timeline data using functional patterns
                    let (
                        recent_sessions,
                        today_duration,
                        week_duration,
                        month_duration,
                        total_duration,
                        session_count,
                    ) = if let Some(timeline_rows) = timeline_data.as_array() {
                        // Take the first 10 entries as recent sessions
                        let recent_sessions: Vec<Timeline> = timeline_rows
                            .iter()
                            .take(10)
                            .filter_map(|timeline_row| {
                                timeline_row.as_object().and_then(|timeline_obj| {
                                    let timeline_row_map: HashMap<String, String> = timeline_obj
                                        .iter()
                                        .map(|(k, v)| (k.clone(), json_value_to_string(v)))
                                        .collect();
                                    Timeline::from_pg_row(&timeline_row_map)
                                })
                            })
                            .collect();

                        // Calculate aggregations using functional approach
                        let (total_duration, session_count, today_dur, week_dur, month_dur) =
                            timeline_rows.iter().fold(
                                (0, 0, 0, 0, 0),
                                |(total, count, today_sum, week_sum, month_sum), timeline_row| {
                                    if let Some(timeline_obj) = timeline_row.as_object() {
                                        let timeline_row_map: HashMap<String, String> =
                                            timeline_obj
                                                .iter()
                                                .map(|(k, v)| (k.clone(), json_value_to_string(v)))
                                                .collect();

                                        if let Some(timeline_entry) =
                                            Timeline::from_pg_row(&timeline_row_map)
                                        {
                                            let duration = timeline_entry.duration.unwrap_or(0);
                                            let today_increment = timeline_entry
                                                .date
                                                .filter(|&date| date == today)
                                                .map(|_| duration)
                                                .unwrap_or(0);

                                            let week_increment = timeline_entry
                                                .date
                                                .filter(|&date| date >= week_ago)
                                                .map(|_| duration)
                                                .unwrap_or(0);

                                            let month_increment = timeline_entry
                                                .date
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
                                        } else {
                                            (total, count, today_sum, week_sum, month_sum)
                                        }
                                    } else {
                                        (total, count, today_sum, week_sum, month_sum)
                                    }
                                },
                            );

                        (
                            recent_sessions,
                            today_dur,
                            week_dur,
                            month_dur,
                            total_duration,
                            session_count,
                        )
                    } else {
                        (vec![], 0, 0, 0, 0, 0)
                    };

                    // Calculate average session length
                    let average_session_length = if session_count > 0 {
                        total_duration as f64 / session_count as f64
                    } else {
                        0.0
                    };

                    let statistics = AppStatistics {
                        app: app.clone(),
                        total_duration: app.duration.unwrap_or(0),
                        today_duration,
                        week_duration,
                        month_duration,
                        average_session_length,
                        recent_sessions,
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

async fn handle_get_checkpoint_stats<T>(
    state: &Arc<RwLock<ServerState<T>>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let state_guard = state
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let client_guard = state_guard.client.read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let checkpoint_id = params
        .get("checkpoint_id")
        .and_then(|v| v.as_i64())
        .map(|i| i as i32)
        .ok_or("checkpoint_id parameter is required")?;

    // Get checkpoint durations for the specified checkpoint ID
    let durations_data = client_guard
        .get_checkpoint_durations_by_ids(&[checkpoint_id])
        .await?;

    let mut durations = Vec::new();

    if let Some(rows) = durations_data.as_array() {
        for row in rows {
            if let Some(obj) = row.as_object() {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), json_value_to_string(v)))
                    .collect();

                if let Some(checkpoint) = Checkpoint::from_pg_row(&row_map) {
                    durations.push(checkpoint);
                }
            }
        }
    }

    let durations_json = serde_json::to_string(&durations)?;
    let response_payload = format!(
        r#"{{"checkpoint_id": {}, "durations": {}}}"#,
        checkpoint_id, durations_json
    );
    let response = WebSocketMessage::checkpoint_stats(&response_payload);

    Ok(response.to_json()?)
}

#[cfg(feature = "memory")]
async fn handle_update_config<T>(
    state: &Arc<RwLock<ServerState<T>>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;

    let tracking_delay_ms = params
        .get("tracking_delay_ms")
        .and_then(|v| v.as_u64())
        .ok_or("tracking_delay_ms parameter is required")?;

    let check_delay_ms = params
        .get("check_delay_ms")
        .and_then(|v| v.as_u64())
        .ok_or("check_delay_ms parameter is required")?;

    let _new_config = TimeTrackingConfig {
        tracking_delay_ms,
        check_delay_ms,
    };

    // Update the config in the server state
    {
        let state_guard = state
            .write()
            .map_err(|e| format!("Failed to acquire write lock: {}", e))?;
        let mut config = state_guard.config.write()
            .map_err(|e| format!("Failed to acquire config write lock: {}", e))?;
        config.tracking_delay_ms = tracking_delay_ms;
        config.check_delay_ms = check_delay_ms;
    }

    let response = WebSocketMessage::success("Configuration updated successfully");
    Ok(response.to_json()?)
}
