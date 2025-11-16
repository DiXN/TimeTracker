use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{anyhow, Context};
use serde_json::Value as JsonValue;
use tungstenite::Message;

use crate::restable::Restable;
use crate::structs::{
    App, AppStatistics, Checkpoint, SessionCount, Timeline, WebSocketCommand,
    WebSocketMessage,
};
use crate::time_tracking::{add_process, delete_process};
use super::server::ServerState;
use super::broadcast::SubscriptionTopic;

type HandlerResult = Result<String, Box<dyn std::error::Error>>;

#[derive(Debug)]
pub enum HandlerError {
    LockError(String),
    MissingParameter(String),
    InvalidParameter(String, String),
    DatabaseError(String),
    SerializationError(String),
}

impl std::fmt::Display for HandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandlerError::LockError(msg) => write!(f, "Lock acquisition failed: {}", msg),
            HandlerError::MissingParameter(param) => write!(f, "Parameter '{}' is required", param),
            HandlerError::InvalidParameter(param, msg) => write!(f, "Invalid parameter '{}': {}", param, msg),
            HandlerError::DatabaseError(msg) => write!(f, "Database operation failed: {}", msg),
            HandlerError::SerializationError(msg) => write!(f, "Serialization failed: {}", msg),
        }
    }
}

impl std::error::Error for HandlerError {}

pub struct MessageHandler;

impl MessageHandler {
    pub async fn handle_message<T>(
        msg: &Message,
        state: &Arc<RwLock<ServerState<T>>>,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        Self::handle_message_with_client_id(msg, state, None).await
    }

    pub async fn handle_message_with_client_id<T>(
        msg: &Message,
        state: &Arc<RwLock<ServerState<T>>>,
        client_id: Option<u64>,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let text = msg.to_text()?;
        let command = WebSocketCommand::from_json(text)?;

        match command {
            WebSocketCommand::GetApps(_) => Self::handle_get_apps(state).await,
            WebSocketCommand::GetTimeline(payload) => Self::handle_get_timeline(state, &payload).await,
            WebSocketCommand::GetAppByName(payload) => Self::handle_get_app_by_name(state, &payload).await,
            WebSocketCommand::AddProcess(payload) => Self::handle_add_process(state, &payload).await,
            WebSocketCommand::DeleteProcess(payload) => Self::handle_delete_process(state, &payload).await,
            WebSocketCommand::GetCheckpoints(payload) => Self::handle_get_checkpoints(state, &payload).await,
            WebSocketCommand::CreateCheckpoint(payload) => Self::handle_create_checkpoint(state, &payload).await,
            WebSocketCommand::SetActiveCheckpoint(payload) => Self::handle_set_active_checkpoint(state, &payload).await,
            WebSocketCommand::DeleteCheckpoint(payload) => Self::handle_delete_checkpoint(state, &payload).await,
            WebSocketCommand::GetActiveCheckpoints(payload) => Self::handle_get_active_checkpoints(state, &payload).await,
            WebSocketCommand::GetTrackingStatus(payload) => Self::handle_get_tracking_status(state, &payload),
            WebSocketCommand::GetSessionCounts(payload) => Self::handle_get_session_counts(state, &payload).await,
            WebSocketCommand::GetStatistics(payload) => Self::handle_get_statistics(state, &payload).await,
            WebSocketCommand::GetCheckpointStats(payload) => Self::handle_get_checkpoint_stats(state, &payload).await,
            #[cfg(feature = "memory")]
            WebSocketCommand::UpdateConfig(payload) => Self::handle_update_config(state, &payload).await,
            WebSocketCommand::Subscribe(payload) => Self::handle_subscribe(state, &payload, client_id),
            WebSocketCommand::Unsubscribe(payload) => Self::handle_unsubscribe(state, &payload, client_id),
        }
    }

    fn acquire_client_lock<T>(state: &Arc<RwLock<ServerState<T>>>) -> Result<Arc<RwLock<T>>, HandlerError>
    where
        T: Restable + Sync + Send,
    {
        let state_guard = state.read()
            .map_err(|e| HandlerError::LockError(format!("Server state: {}", e)))?;
        Ok(Arc::clone(&state_guard.client))
    }

    async fn acquire_locks<T>(state: &Arc<RwLock<ServerState<T>>>) -> Result<(Arc<RwLock<T>>,), HandlerError>
    where
        T: Restable + Sync + Send,
    {
        let client_arc = Self::acquire_client_lock(state)?;
        Ok((client_arc,))
    }

    fn parse_payload(payload: &str) -> Result<HashMap<String, JsonValue>, HandlerError> {
        if payload.is_empty() {
            return Ok(HashMap::new());
        }
        serde_json::from_str(payload)
            .map_err(|e| HandlerError::SerializationError(format!("Failed to parse payload JSON: {}", e)))
    }

    fn extract_string_param(params: &HashMap<String, JsonValue>, key: &str) -> Result<String, HandlerError> {
        params.get(key)
            .and_then(JsonValue::as_str)
            .map(String::from)
            .ok_or_else(|| HandlerError::MissingParameter(key.to_string()))
    }

    fn extract_i32_param(params: &HashMap<String, JsonValue>, key: &str) -> Result<i32, HandlerError> {
        params.get(key)
            .and_then(JsonValue::as_i64)
            .map(|i| i as i32)
            .ok_or_else(|| HandlerError::MissingParameter(key.to_string()))
    }

    fn extract_bool_param(params: &HashMap<String, JsonValue>, key: &str) -> Result<bool, HandlerError> {
        params.get(key)
            .and_then(JsonValue::as_bool)
            .ok_or_else(|| HandlerError::MissingParameter(key.to_string()))
    }

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
        let today = chrono::Local::now().naive_local().date();
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

    async fn call_db_method<T, F, Fut, R>(client: Arc<RwLock<T>>, operation: F, context_msg: &str) -> Result<R, anyhow::Error>
    where
        T: Restable + Sync + Send,
        F: FnOnce(Arc<RwLock<T>>) -> Fut,
        Fut: std::future::Future<Output = Result<R, Box<dyn std::error::Error>>>,
    {
        operation(client).await
            .map_err(|e| anyhow!("{}: {}", context_msg, e))
    }

    async fn handle_get_apps<T>(state: &Arc<RwLock<ServerState<T>>>) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let client_arc = Self::acquire_client_lock(state)?;

        let apps_data = Self::call_db_method(client_arc, |c| async move {
            let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
            guard.get_all_apps().await
        }, "Failed to retrieve apps").await?;

        let apps = apps_data.as_array()
            .map(|rows| Self::convert_json_rows_to_apps(rows))
            .unwrap_or_default();

        let apps_json = serde_json::to_string(&apps)
            .context("Failed to serialize apps")?;
        let response = WebSocketMessage::apps_list(&apps_json);
        Ok(response.to_json().context("Failed to serialize response")?)
    }

    async fn handle_get_timeline<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        _payload: &str,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let client_arc = Self::acquire_client_lock(state)?;

        let timeline_data = Self::call_db_method(client_arc, |c| async move {
            let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
            guard.get_timeline_with_checkpoints().await
        }, "Failed to retrieve timeline data").await?;

        let timeline_json = serde_json::to_string(&timeline_data)
            .context("Failed to serialize timeline data")?;
        let response = WebSocketMessage::timeline_data(&timeline_json);
        Ok(response.to_json().context("Failed to serialize response")?)
    }

    async fn handle_get_app_by_name<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        payload: &str,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let client_arc = Self::acquire_client_lock(state)?;
        let params = Self::parse_payload(payload)?;
        let app_name = Self::extract_string_param(&params, "name")?;

        let apps_data = Self::call_db_method(client_arc, |c| async move {
            let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
            guard.get_all_apps().await
        }, "Failed to retrieve apps").await?;

        let apps = apps_data.as_array()
            .map(|rows| Self::convert_json_rows_to_apps(rows))
            .unwrap_or_default();

        let found_app = apps.into_iter()
            .find(|app| app.name.as_deref() == Some(&app_name))
            .ok_or_else(|| anyhow!("App '{}' not found", app_name))?;

        let app_json = serde_json::to_string(&found_app)
            .context("Failed to serialize app")?;
        let response = WebSocketMessage::app(&app_json);
        Ok(response.to_json().context("Failed to serialize response")?)
    }

    async fn handle_add_process<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        payload: &str,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let client_arc = Self::acquire_client_lock(state)?;
        let params = Self::parse_payload(payload)?;
        let process_name = Self::extract_string_param(&params, "process_name")?;
        let path = Self::extract_string_param(&params, "path")?;

        add_process(&process_name, &path, &client_arc).await
            .map_err(|e| anyhow!("Failed to add process '{}': {}", process_name, e))?;

        let response = WebSocketMessage::success(&format!("Successfully added process: {}", process_name));
        Ok(response.to_json().context("Failed to serialize response")?)
    }

    async fn handle_delete_process<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        payload: &str,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let client_arc = Self::acquire_client_lock(state)?;
        let params = Self::parse_payload(payload)?;
        let process_name = Self::extract_string_param(&params, "process_name")?;

        delete_process(&process_name, &client_arc).await
            .map_err(|e| anyhow!("Failed to delete process '{}': {}", process_name, e))?;

        let response = WebSocketMessage::success(&format!("Successfully deleted process: {}", process_name));
        Ok(response.to_json().context("Failed to serialize response")?)
    }

    fn handle_get_tracking_status<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        _payload: &str,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let state_guard = state.read()
            .map_err(|e| HandlerError::LockError(format!("Server state: {}", e)))?;
        let status = state_guard.get_current_tracking_status();
        let status_json = serde_json::to_string(&status)
            .context("Failed to serialize tracking status")?;
        let response = WebSocketMessage::tracking_status(&status_json);
        Ok(response.to_json().context("Failed to serialize response")?)
    }

    async fn handle_get_checkpoints<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        payload: &str,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let (client_arc,) = Self::acquire_locks(state).await?;
        let params = Self::parse_payload(payload)?;

        let checkpoints_data = Self::call_db_method(client_arc, |c| async move {
            let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
            match params.get("app_id").and_then(JsonValue::as_i64).map(|i| i as i32) {
                Some(app_id) => guard.get_checkpoints_for_app(app_id).await,
                None => guard.get_all_checkpoints().await,
            }
        }, "Failed to retrieve checkpoints").await?;

        let checkpoints = checkpoints_data.as_array()
            .map(|rows| Self::convert_json_rows_to_checkpoints(rows))
            .unwrap_or_default();

        let checkpoints_json = serde_json::to_string(&checkpoints)
            .context("Failed to serialize checkpoints")?;
        let response = WebSocketMessage::checkpoints_list(&checkpoints_json);
        Ok(response.to_json().context("Failed to serialize response")?)
    }

    async fn handle_create_checkpoint<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        payload: &str,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let (client_arc,) = Self::acquire_locks(state).await?;
        let params = Self::parse_payload(payload)?;
        let name = Self::extract_string_param(&params, "name")?;
        let description = params.get("description").and_then(JsonValue::as_str);
        let app_id = Self::extract_i32_param(&params, "app_id")?;

        let name_clone = name.clone();
        Self::call_db_method(Arc::clone(&client_arc), |c| async move {
            let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
            guard.create_checkpoint(&name_clone, description, app_id).await
        }, &format!("Failed to create checkpoint '{}'", name)).await?;

        // Try to return updated checkpoints list, fallback to success message
        let response = match Self::call_db_method(client_arc, |c| async move {
            let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
            guard.get_checkpoints_for_app(app_id).await
        }, "Failed to get updated checkpoints").await {
            Ok(checkpoints_data) => {
                let checkpoints = checkpoints_data.as_array()
                    .map(|rows| Self::convert_json_rows_to_checkpoints(rows))
                    .unwrap_or_default();
                let checkpoints_json = serde_json::to_string(&checkpoints)
                    .context("Failed to serialize checkpoints")?;
                WebSocketMessage::checkpoints_list(&checkpoints_json)
            }
            Err(_) => WebSocketMessage::success(&format!("Successfully created checkpoint: {}", name)),
        };

        Ok(response.to_json().context("Failed to serialize response")?)
    }

    async fn handle_set_active_checkpoint<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        payload: &str,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let (client_arc,) = Self::acquire_locks(state).await?;
        let params = Self::parse_payload(payload)?;
        let checkpoint_id = Self::extract_i32_param(&params, "checkpoint_id")?;
        let is_active = params.get("is_active")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);

        Self::call_db_method(client_arc, |c| async move {
            let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
            guard.set_checkpoint_active(checkpoint_id, is_active).await
        }, &format!("Failed to update checkpoint {}", checkpoint_id)).await?;

        let status = if is_active { "activated" } else { "deactivated" };
        let response = WebSocketMessage::success(&format!(
            "Successfully {} checkpoint with ID: {}",
            status, checkpoint_id
        ));
        Ok(response.to_json().context("Failed to serialize response")?)
    }

    async fn handle_delete_checkpoint<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        payload: &str,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let (client_arc,) = Self::acquire_locks(state).await?;
        let params = Self::parse_payload(payload)?;
        let checkpoint_id = Self::extract_i32_param(&params, "checkpoint_id")?;

        Self::call_db_method(client_arc, |c| async move {
            let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
            guard.delete_checkpoint(checkpoint_id).await
        }, &format!("Failed to delete checkpoint {}", checkpoint_id)).await?;

        let response = WebSocketMessage::success(&format!(
            "Successfully deleted checkpoint with ID: {}",
            checkpoint_id
        ));
        Ok(response.to_json().context("Failed to serialize response")?)
    }

    async fn handle_get_active_checkpoints<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        payload: &str,
    ) -> Result<String, Box<dyn std::error::Error>>
    where
        T: Restable + Sync + Send,
    {
        let client_arc = Self::acquire_client_lock(state)
            .map_err(|e| format!("Failed to acquire client lock: {}", e))?;

        let active_checkpoints_data = if payload.is_empty() {
            Self::call_db_method(client_arc, |c| async move {
                let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
                guard.get_active_checkpoints().await
            }, "Failed to get active checkpoints").await?
        } else {
            let params = Self::parse_payload(payload)
                .map_err(|e| format!("Failed to parse payload: {}", e))?;

            if let Some(app_id) = params.get("app_id").and_then(|v| v.as_i64()).map(|i| i as i32) {
                Self::call_db_method(client_arc, |c| async move {
                    let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
                    guard.get_active_checkpoints_for_app(app_id).await
                }, &format!("Failed to get active checkpoints for app {}", app_id)).await?
            } else {
                Self::call_db_method(client_arc, |c| async move {
                    let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
                    guard.get_active_checkpoints().await
                }, "Failed to get active checkpoints").await?
            }
        };

        let active_checkpoints = active_checkpoints_data
            .as_array()
            .map(|rows| Self::convert_json_rows_to_checkpoints(rows))
            .unwrap_or_default();

        let checkpoints_json = serde_json::to_string(&active_checkpoints)
            .map_err(|e| format!("Failed to serialize active checkpoints: {}", e))?;
        let response = WebSocketMessage::active_checkpoints(&checkpoints_json);

        response.to_json()
            .map_err(|e| format!("Failed to serialize response: {}", e).into())
    }

    async fn handle_get_session_counts<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        _payload: &str,
    ) -> Result<String, Box<dyn std::error::Error>>
    where
        T: Restable + Sync + Send,
    {
        let client_arc = Self::acquire_client_lock(state)
            .map_err(|e| format!("Failed to acquire client lock: {}", e))?;

        let app_ids = Self::call_db_method(Arc::clone(&client_arc), |c| async move {
            let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
            guard.get_all_app_ids().await
        }, "Failed to retrieve app IDs").await?;

        let session_counts = futures_util::future::try_join_all(
            app_ids.into_iter().map(|app_id| {
                let client_arc = Arc::clone(&client_arc);
                async move {
                    let session_count_value = Self::call_db_method(client_arc, |c| async move {
                        let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
                        guard.get_session_count_for_app(app_id).await
                    }, &format!("Failed to get session count for app {}", app_id)).await?;

                    Ok::<SessionCount, Box<dyn std::error::Error>>(SessionCount {
                        app_id,
                        session_count: session_count_value,
                    })
                }
            })
        ).await?;

        let counts_json = serde_json::to_string(&session_counts)
            .map_err(|e| format!("Failed to serialize session counts: {}", e))?;
        let response = WebSocketMessage::session_counts_data(&counts_json);

        response.to_json()
            .map_err(|e| format!("Failed to serialize response: {}", e).into())
    }

    async fn calculate_app_statistics<T>(
        client_guard: &T,
        app: &App,
    ) -> Result<AppStatistics, Box<dyn std::error::Error>>
    where
        T: Restable + Sync + Send,
    {
        let timeline_data = client_guard
            .get_timeline_data(app.name.as_ref().map(|s| s.as_str()), 30)
            .await?;

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

    async fn handle_get_statistics<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        _payload: &str,
    ) -> Result<String, Box<dyn std::error::Error>>
    where
        T: Restable + Sync + Send,
    {
        let state_guard = state.read()
            .map_err(|e| format!("Failed to acquire state lock: {}", e))?;
        let client_guard = state_guard.client.read()
            .map_err(|e| format!("Failed to acquire client lock: {}", e))?;

        let apps_data = client_guard.get_all_apps().await
            .map_err(|e| format!("Failed to retrieve apps: {}", e))?;

        let apps = apps_data
            .as_array()
            .map(|rows| Self::convert_json_rows_to_apps(rows))
            .unwrap_or_default();

        let mut app_statistics = Vec::new();
        for app in apps {
            match Self::calculate_app_statistics(&*client_guard, &app).await {
                Ok(stats) => app_statistics.push(stats),
                Err(e) => eprintln!("Failed to calculate statistics for app '{}': {}",
                    app.name.as_deref().unwrap_or("unknown"), e),
            }
        }

        let statistics_json = serde_json::to_string(&app_statistics)
            .map_err(|e| format!("Failed to serialize statistics: {}", e))?;
        let response = WebSocketMessage::statistics_data(&statistics_json);

        response.to_json()
            .map_err(|e| format!("Failed to serialize response: {}", e).into())
    }

    async fn handle_get_checkpoint_stats<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        payload: &str,
    ) -> Result<String, Box<dyn std::error::Error>>
    where
        T: Restable + Sync + Send,
    {
        let client_arc = Self::acquire_client_lock(state)
            .map_err(|e| format!("Failed to acquire client lock: {}", e))?;

        let params = Self::parse_payload(payload)
            .map_err(|e| format!("Failed to parse payload: {}", e))?;
        let checkpoint_id = Self::extract_i32_param(&params, "checkpoint_id")
            .map_err(|e| format!("Invalid checkpoint_id parameter: {}", e))?;

        let durations_data = Self::call_db_method(client_arc, |c| async move {
            let guard = c.read().map_err(|e| format!("Lock error: {}", e))?;
            guard.get_checkpoint_durations_by_ids(&[checkpoint_id]).await
        }, &format!("Failed to get durations for checkpoint {}", checkpoint_id)).await?;

        let durations = durations_data
            .as_array()
            .map(|rows| Self::convert_json_rows_to_checkpoints(rows))
            .unwrap_or_default();



        let response_payload = serde_json::json!({
            "checkpoint_id": checkpoint_id,
            "durations": durations
        });

        let response = WebSocketMessage::checkpoint_stats(&response_payload.to_string());
        response.to_json()
            .map_err(|e| format!("Failed to serialize response: {}", e).into())
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

        let tracking_delay_ms = params.get("tracking_delay_ms").and_then(|v| v.as_u64())
            .ok_or("tracking_delay_ms parameter is required")?;
        let check_delay_ms = params.get("check_delay_ms").and_then(|v| v.as_u64())
            .ok_or("check_delay_ms parameter is required")?;

        {
            let state_guard = state.write().map_err(|e| format!("Failed to acquire write lock: {}", e))?;
            let mut config = state_guard.config().write().map_err(|e| format!("Failed to acquire config write lock: {}", e))?;
            config.tracking_delay_ms = tracking_delay_ms;
            config.check_delay_ms = check_delay_ms;
        }

        let response = WebSocketMessage::success("Configuration updated successfully");
        Ok(response.to_json()?)
    }

    fn handle_subscribe<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        payload: &str,
        client_id: Option<u64>,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let params = Self::parse_payload(payload)?;
        let topics_value = params.get("topics")
            .ok_or_else(|| anyhow!("Missing 'topics' parameter"))?;

        let topic_strings: Vec<String> = serde_json::from_value(topics_value.clone())
            .context("Failed to parse topics array")?;

        let mut topics = Vec::new();
        for topic_str in topic_strings {
            if let Some(topic) = SubscriptionTopic::from_str(&topic_str) {
                topics.push(topic);
            } else {
                return Err(anyhow!("Invalid subscription topic: {}", topic_str).into());
            }
        }

        let client_id = if let Some(id) = client_id {
            id
        } else {
            let client_id_value = params.get("client_id")
                .ok_or_else(|| anyhow!("Missing 'client_id' parameter and no client context"))?;
            serde_json::from_value(client_id_value.clone())
                .context("Failed to parse client_id")?
        };

        let state_guard = state.read()
            .map_err(|e| anyhow!("Failed to acquire server state lock: {}", e))?;
        state_guard.subscribe_client(client_id, topics.clone());

        let topic_names: Vec<&str> = topics.iter().map(|t| t.as_str()).collect();
        let response = WebSocketMessage::success(&format!(
            "Successfully subscribed to topics: {}",
            topic_names.join(", ")
        ));
        Ok(response.to_json()?)
    }

    fn handle_unsubscribe<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        payload: &str,
        client_id: Option<u64>,
    ) -> HandlerResult
    where
        T: Restable + Sync + Send,
    {
        let params = Self::parse_payload(payload)?;
        let topics_value = params.get("topics")
            .ok_or_else(|| anyhow!("Missing 'topics' parameter"))?;

        let topic_strings: Vec<String> = serde_json::from_value(topics_value.clone())
            .context("Failed to parse topics array")?;

        let mut topics = Vec::new();
        for topic_str in topic_strings {
            if let Some(topic) = SubscriptionTopic::from_str(&topic_str) {
                topics.push(topic);
            } else {
                return Err(anyhow!("Invalid subscription topic: {}", topic_str).into());
            }
        }

        let client_id = if let Some(id) = client_id {
            id
        } else {
            let client_id_value = params.get("client_id")
                .ok_or_else(|| anyhow!("Missing 'client_id' parameter and no client context"))?;
            serde_json::from_value(client_id_value.clone())
                .context("Failed to parse client_id")?
        };

        let state_guard = state.read()
            .map_err(|e| anyhow!("Failed to acquire server state lock: {}", e))?;
        state_guard.unsubscribe_client(client_id, topics.clone());

        let topic_names: Vec<&str> = topics.iter().map(|t| t.as_str()).collect();
        let response = WebSocketMessage::success(&format!(
            "Successfully unsubscribed from topics: {}",
            topic_names.join(", ")
        ));
        Ok(response.to_json()?)
    }
}
