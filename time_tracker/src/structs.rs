use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct App {
    pub id: i32,
    pub duration: Option<i32>,
    pub launches: Option<i32>,
    pub longest_session: Option<i32>,
    pub name: Option<String>,
    pub product_name: Option<String>,
    pub longest_session_on: Option<NaiveDate>,
}

/// Serializes an Option<NaiveDateTime> as an empty string if None, or the datetime string if Some
fn serialize_optional_datetime<S>(value: &Option<chrono::NaiveDateTime>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(dt) => serializer.serialize_str(&dt.format("%Y-%m-%dT%H:%M:%S%.f").to_string()),
        None => serializer.serialize_str(""),
    }
}

impl App {
    pub fn new(id: i32, name: Option<String>, product_name: Option<String>) -> Self {
        App {
            id,
            name,
            product_name,
            duration: Some(0),
            launches: Some(0),
            longest_session: Some(0),
            longest_session_on: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub id: i32,
    pub date: Option<NaiveDate>,
    pub duration: Option<i32>,
    pub app_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: i32,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(serialize_with = "serialize_optional_datetime")]
    pub created_at: Option<NaiveDateTime>,
    #[serde(serialize_with = "serialize_optional_datetime")]
    pub valid_from: Option<NaiveDateTime>,
    pub color: Option<String>,
    pub app_id: i32,
    pub is_active: Option<bool>,
    // Consolidated fields from other tables
    pub timeline_id: Option<i32>,
    pub duration: Option<i32>,
    pub sessions_count: Option<i32>,
    #[serde(serialize_with = "serialize_optional_datetime")]
    pub last_updated: Option<NaiveDateTime>,
    #[serde(serialize_with = "serialize_optional_datetime")]
    pub activated_at: Option<NaiveDateTime>,
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingStatus {
    pub is_tracking: bool,
    pub is_paused: bool,
    pub current_app: Option<String>,
    pub current_session_duration: i32,
    pub session_start_time: Option<String>,
    pub active_checkpoint_ids: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStatistics {
    pub app: App,
    pub total_duration: i32,
    pub today_duration: i32,
    pub week_duration: i32,
    pub month_duration: i32,
    pub average_session_length: f64,
    pub recent_sessions: Vec<Timeline>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCount {
    pub app_id: i32,
    pub session_count: i32,
}

impl Timeline {
    pub fn new(
        id: i32,
        date: Option<NaiveDate>,
        duration: Option<i32>,
        app_id: Option<i32>,
    ) -> Self {
        Timeline {
            id,
            date,
            duration,
            app_id,
        }
    }
}

impl Checkpoint {
    pub fn new(id: i32, name: Option<String>, description: Option<String>, app_id: i32) -> Self {
        Checkpoint {
            id,
            name,
            description,
            created_at: None,
            valid_from: None,
            color: None,
            app_id,
            is_active: None,
            timeline_id: None,
            duration: None,
            sessions_count: None,
            last_updated: None,
            activated_at: None,
        }
    }
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppsResponse {
    pub apps: Vec<App>,
    pub total_apps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineResponse {
    pub timeline: Vec<Timeline>,
    pub total_entries: usize,
    pub date_range: Option<DateRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum WebSocketMessage {
    #[serde(rename = "apps_list")]
    AppsList(String),
    #[serde(rename = "timeline_data")]
    TimelineData(String),
    #[serde(rename = "app")]
    App(String),
    #[serde(rename = "error")]
    Error(String),
    #[serde(rename = "success")]
    Success(String),
    #[serde(rename = "tracking_status_update")]
    TrackingStatusUpdate(String),
    #[serde(rename = "statistics_data")]
    StatisticsData(String),
    #[serde(rename = "session_counts_legacy")]
    SessionCounts(String),
    #[serde(rename = "checkpoints_list")]
    CheckpointsList(String),
    #[serde(rename = "checkpoint")]
    Checkpoint(String),
    #[serde(rename = "active_checkpoints")]
    ActiveCheckpoints(String),
    #[serde(rename = "tracking_status")]
    TrackingStatus(String),
    #[serde(rename = "session_counts")]
    SessionCountsData(String),
    #[serde(rename = "statistics")]
    Statistics(String),
    #[serde(rename = "checkpoint_stats")]
    CheckpointStats(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum WebSocketCommand {
    #[serde(rename = "get_apps")]
    GetApps(String),
    #[serde(rename = "get_timeline")]
    GetTimeline(String),
    #[serde(rename = "get_app_by_name")]
    GetAppByName(String),
    #[serde(rename = "add_process")]
    AddProcess(String),
    #[serde(rename = "delete_process")]
    DeleteProcess(String),
    #[serde(rename = "get_tracking_status")]
    GetTrackingStatus(String),
    #[serde(rename = "get_statistics")]
    GetStatistics(String),
    #[serde(rename = "get_session_counts")]
    GetSessionCounts(String),
    #[serde(rename = "get_checkpoints")]
    GetCheckpoints(String),
    #[serde(rename = "create_checkpoint")]
    CreateCheckpoint(String),
    #[serde(rename = "set_active_checkpoint")]
    SetActiveCheckpoint(String),
    #[serde(rename = "delete_checkpoint")]
    DeleteCheckpoint(String),
    #[serde(rename = "get_active_checkpoints")]
    GetActiveCheckpoints(String),
    #[serde(rename = "get_checkpoint_stats")]
    GetCheckpointStats(String),
    #[cfg(feature = "sqlite")]
    #[serde(rename = "update_config")]
    UpdateConfig(String),
}

impl WebSocketMessage {
    pub fn error(message: &str) -> Self {
        let payload = format!(r#"{{"message": "{}"}}"#, message);
        WebSocketMessage::Error(payload)
    }

    pub fn success(message: &str) -> Self {
        let payload = format!(r#"{{"message": "{}"}}"#, message);
        WebSocketMessage::Success(payload)
    }

    pub fn apps_list(apps_json: &str) -> Self {
        let payload = format!(r#"{{"apps": {}}}"#, apps_json);
        WebSocketMessage::AppsList(payload)
    }

    pub fn timeline_data(timeline_json: &str) -> Self {
        let payload = format!(r#"{{"timeline": {}}}"#, timeline_json);
        WebSocketMessage::TimelineData(payload)
    }

    pub fn app(app_json: &str) -> Self {
        let payload = format!(r#"{{"app": {}}}"#, app_json);
        WebSocketMessage::App(payload)
    }

    pub fn checkpoints_list(checkpoints_json: &str) -> Self {
        let payload = format!(r#"{{"checkpoints": {}}}"#, checkpoints_json);
        WebSocketMessage::CheckpointsList(payload)
    }

    pub fn checkpoint(checkpoint_json: &str) -> Self {
        let payload = format!(r#"{{"checkpoint": {}}}"#, checkpoint_json);
        WebSocketMessage::Checkpoint(payload)
    }

    pub fn active_checkpoints(checkpoints_json: &str) -> Self {
        let payload = format!(r#"{{"active_checkpoints": {}}}"#, checkpoints_json);
        WebSocketMessage::ActiveCheckpoints(payload)
    }

    pub fn tracking_status(status_json: &str) -> Self {
        let payload = format!(r#"{{"status": {}}}"#, status_json);
        WebSocketMessage::TrackingStatus(payload)
    }

    pub fn session_counts_data(counts_json: &str) -> Self {
        let payload = format!(r#"{{"counts": {}}}"#, counts_json);
        WebSocketMessage::SessionCountsData(payload)
    }

    pub fn statistics_data(statistics_json: &str) -> Self {
        let payload = format!(r#"{{"statistics": {}}}"#, statistics_json);
        WebSocketMessage::StatisticsData(payload)
    }

    pub fn checkpoint_stats(stats_json: &str) -> Self {
        // The stats_json should already be the properly formatted JSON object
        WebSocketMessage::CheckpointStats(stats_json.to_string())
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

impl WebSocketCommand {
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }
}

impl App {
    pub fn from_pg_row(row_data: &HashMap<String, String>) -> Option<Self> {
        let id = row_data.get("id")?.parse().ok()?;

        let parse_optional_int = |key: &str| -> Option<i32> {
            row_data
                .get(key)
                .filter(|s| !s.is_empty())
                .and_then(|s| s.parse().ok())
        };

        let parse_optional_string = |key: &str| -> Option<String> {
            row_data
                .get(key)
                .filter(|s| !s.is_empty())
                .map(String::from)
        };

        let longest_session_on = row_data
            .get("longest_session_on")
            .filter(|s| !s.is_empty())
            .and_then(|date_str| NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok());

        Some(App {
            id,
            duration: parse_optional_int("duration"),
            launches: parse_optional_int("launches"),
            longest_session: parse_optional_int("longest_session"),
            name: parse_optional_string("name"),
            product_name: parse_optional_string("product_name"),
            longest_session_on,
        })
    }
}

impl Timeline {
    pub fn from_pg_row(row_data: &HashMap<String, String>) -> Option<Self> {
        let id = row_data.get("id")?.parse().ok()?;

        let date = row_data
            .get("date")
            .filter(|s| !s.is_empty())
            .and_then(|date_str| {
                date_str
                    .parse::<NaiveDate>()
                    .or_else(|_| NaiveDate::parse_from_str(date_str, "%Y-%m-%d"))
                    .ok()
            });

        let duration = row_data
            .get("duration")
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse().ok());

        let app_id = row_data
            .get("app_id")
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse().ok());

        Some(Timeline {
            id,
            date,
            duration,
            app_id,
        })
    }
}

impl Checkpoint {
    pub fn from_pg_row(row_data: &HashMap<String, String>) -> Option<Self> {
        let id = row_data.get("id")?.parse().ok()?;

        let parse_optional_string = |key: &str| -> Option<String> {
            row_data
                .get(key)
                .filter(|s| !s.is_empty())
                .map(String::from)
        };

        let parse_optional_datetime = |key: &str| -> Option<NaiveDateTime> {
            row_data
                .get(key)
                .filter(|s| !s.is_empty())
                .and_then(|datetime_str| {
                    // Try multiple formats to handle different datetime representations
                    NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S%.f")
                        .or_else(|_| NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S"))
                        .or_else(|_| NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%dT%H:%M:%S%.f"))
                        .or_else(|_| NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%dT%H:%M:%S"))
                        .ok()
                })
        };

        let app_id = row_data
            .get("app_id")
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse().ok());

        let is_active = row_data
            .get("is_active")
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse().ok());

        // Parse additional fields for consolidated table
        let timeline_id = row_data
            .get("timeline_id")
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse().ok());

        let duration = row_data
            .get("duration")
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse().ok());

        let sessions_count = row_data
            .get("sessions_count")
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse().ok());

        Some(Checkpoint {
            id,
            name: parse_optional_string("name"),
            description: parse_optional_string("description"),
            created_at: parse_optional_datetime("created_at"),
            valid_from: parse_optional_datetime("valid_from"),
            color: parse_optional_string("color"),
            app_id: app_id.unwrap_or_default(),
            is_active,
            timeline_id,
            duration,
            sessions_count,
            last_updated: parse_optional_datetime("last_updated"),
            activated_at: parse_optional_datetime("activated_at"),
        })
    }
}


