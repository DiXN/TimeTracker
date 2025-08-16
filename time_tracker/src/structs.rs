use chrono::NaiveDate;
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
    #[serde(rename = "session_counts")]
    SessionCounts(String),
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
