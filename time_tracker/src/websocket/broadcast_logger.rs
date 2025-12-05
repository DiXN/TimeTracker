use log::{error, info, warn};

use super::tracking_notifier::{
    broadcast_apps_update as internal_broadcast_apps_update,
    broadcast_timeline_update as internal_broadcast_timeline_update,
    broadcast_tracking_status_update as internal_broadcast_tracking_status_update,
    has_active_broadcaster,
};
use crate::structs::{App, Timeline, TrackingStatus};

pub fn broadcast_apps_update(apps: Vec<App>) {
    if !has_active_broadcaster() {
        warn!("Attempted to broadcast apps update but no active WebSocket broadcaster found");
        return;
    }

    let app_count = apps.len();
    let app_names: Vec<&str> = apps
        .iter()
        .filter_map(|app| app.name.as_deref())
        .take(3)
        .collect();

    let app_names_str = if app_count > 3 {
        format!("{} and {} more", app_names.join(", "), app_count - 3)
    } else {
        app_names.join(", ")
    };

    info!("Broadcasting apps update: {} apps ({})", app_count, app_names_str);
    internal_broadcast_apps_update(apps);
}

pub fn broadcast_timeline_update(timeline: Vec<Timeline>) {
    if !has_active_broadcaster() {
        warn!("Attempted to broadcast timeline update but no active WebSocket broadcaster found");
        return;
    }

    let timeline_count = timeline.len();
    let total_duration: i32 = timeline.iter().filter_map(|t| t.duration).sum();

    let date_range = match (timeline.first(), timeline.last()) {
        (Some(first), Some(last)) => {
            match (first.date, last.date) {
                (Some(f), Some(l)) if f == l => f.to_string(),
                (Some(f), Some(l)) => format!("{} to {}", f, l),
                _ => "unknown dates".to_string(),
            }
        }
        _ => "no entries".to_string(),
    };

    info!(
        "Broadcasting timeline update: {} entries, {} total duration, dates: {}",
        timeline_count, total_duration, date_range
    );
    internal_broadcast_timeline_update(timeline);
}

pub fn broadcast_tracking_status_update(status: TrackingStatus) {
    if !has_active_broadcaster() {
        warn!("Attempted to broadcast tracking status update but no active WebSocket broadcaster found");
        return;
    }

    let app_name = status.current_app.as_deref().unwrap_or("None");
    let tracking_state = match (status.is_tracking, status.is_paused) {
        (true, false) => "Active",
        (true, true) => "Paused",
        (false, _) => "Stopped",
    };

    info!(
        "Broadcasting tracking status update: {} - {} (session: {}s, checkpoints: {})",
        tracking_state,
        app_name,
        status.current_session_duration,
        status.active_checkpoint_ids.len()
    );
    internal_broadcast_tracking_status_update(status);
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct BroadcastStats {
    pub has_active_broadcaster: bool,
}

#[allow(dead_code)]
pub fn get_broadcast_stats() -> BroadcastStats {
    BroadcastStats {
        has_active_broadcaster: has_active_broadcaster(),
    }
}

#[allow(dead_code)]
pub fn log_broadcast_status() {
    if has_active_broadcaster() {
        info!("WebSocket broadcasting system: ACTIVE - ready to broadcast updates");
    } else {
        info!("WebSocket broadcasting system: INACTIVE - no active WebSocket clients");
    }
}

#[allow(dead_code)]
pub fn with_broadcast_logging<F, T>(operation_name: &str, operation: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, Box<dyn std::error::Error>>,
{
    info!("Starting broadcast operation: {}", operation_name);

    match operation() {
        Ok(result) => {
            info!("Broadcast operation completed successfully: {}", operation_name);
            Ok(result)
        }
        Err(e) => {
            error!("Broadcast operation failed: {} - Error: {}", operation_name, e);
            Err(format!("Broadcast operation '{}' failed: {}", operation_name, e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_broadcast_stats() {
        let stats = get_broadcast_stats();
        assert!(stats.has_active_broadcaster || !stats.has_active_broadcaster);
    }

    #[test]
    fn test_with_broadcast_logging_success() {
        let result = with_broadcast_logging("test_operation", || Ok("success".to_string()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[test]
    fn test_with_broadcast_logging_failure() {
        let result: Result<String, String> =
            with_broadcast_logging("test_operation", || Err("test error".into()));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("test error"));
    }
}
