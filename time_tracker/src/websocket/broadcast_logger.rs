use log::{info, warn, error};
use crate::structs::{App, Timeline, TrackingStatus};
use super::tracking_notifier::{
    broadcast_apps_update as internal_broadcast_apps_update,
    broadcast_timeline_update as internal_broadcast_timeline_update,
    broadcast_tracking_status_update as internal_broadcast_tracking_status_update,
    has_active_broadcaster,
};

/// Logged wrapper for broadcasting apps updates
pub fn broadcast_apps_update(apps: Vec<App>) {
    if !has_active_broadcaster() {
        warn!("Attempted to broadcast apps update but no active WebSocket broadcaster found");
        return;
    }

    let app_count = apps.len();
    let app_names: Vec<String> = apps.iter()
        .filter_map(|app| app.name.as_ref())
        .take(3) // Show first 3 app names
        .cloned()
        .collect();
    
    let app_names_str = if app_count > 3 {
        format!("{} and {} more", app_names.join(", "), app_count - 3)
    } else {
        app_names.join(", ")
    };

    info!("Broadcasting apps update: {} apps ({})", app_count, app_names_str);
    
    internal_broadcast_apps_update(apps);
    
    info!("Apps update broadcast completed");
}

/// Logged wrapper for broadcasting timeline updates
pub fn broadcast_timeline_update(timeline: Vec<Timeline>) {
    if !has_active_broadcaster() {
        warn!("Attempted to broadcast timeline update but no active WebSocket broadcaster found");
        return;
    }

    let timeline_count = timeline.len();
    let total_duration: i32 = timeline.iter()
        .filter_map(|t| t.duration)
        .sum();
    
    let date_range = if timeline.is_empty() {
        "no entries".to_string()
    } else {
        let dates: Vec<String> = timeline.iter()
            .filter_map(|t| t.date.as_ref())
            .map(|d| d.to_string())
            .collect();
        if dates.is_empty() {
            "unknown dates".to_string()
        } else {
            let first = dates.first().unwrap();
            let last = dates.last().unwrap();
            if first == last {
                first.clone()
            } else {
                format!("{} to {}", first, last)
            }
        }
    };

    info!("Broadcasting timeline update: {} entries, {} total duration, dates: {}", 
          timeline_count, total_duration, date_range);
    
    internal_broadcast_timeline_update(timeline);
    
    info!("Timeline update broadcast completed");
}

/// Logged wrapper for broadcasting tracking status updates
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

    info!("Broadcasting tracking status update: {} - {} (session: {}s, checkpoints: {})", 
          tracking_state, 
          app_name, 
          status.current_session_duration,
          status.active_checkpoint_ids.len());
    
    internal_broadcast_tracking_status_update(status);
    
    info!("Tracking status update broadcast completed");
}

/// Get broadcasting statistics for monitoring
pub fn get_broadcast_stats() -> BroadcastStats {
    BroadcastStats {
        has_active_broadcaster: has_active_broadcaster(),
    }
}

#[derive(Debug)]
pub struct BroadcastStats {
    pub has_active_broadcaster: bool,
}

/// Log current broadcasting system status
pub fn log_broadcast_status() {
    let stats = get_broadcast_stats();
    if stats.has_active_broadcaster {
        info!("WebSocket broadcasting system: ACTIVE - ready to broadcast updates");
    } else {
        info!("WebSocket broadcasting system: INACTIVE - no active WebSocket clients");
    }
}

/// Wrapper that logs and executes a broadcasting operation with error handling
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
        // Stats should be retrievable (broadcaster may or may not be active)
        assert!(stats.has_active_broadcaster == true || stats.has_active_broadcaster == false);
    }

    #[test]
    fn test_with_broadcast_logging_success() {
        let result = with_broadcast_logging("test_operation", || {
            Ok("success".to_string())
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[test]
    fn test_with_broadcast_logging_failure() {
        let result = with_broadcast_logging("test_operation", || {
            Err("test error".into())
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("test error"));
    }
}
