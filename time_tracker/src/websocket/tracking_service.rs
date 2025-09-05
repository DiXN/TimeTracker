use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex};
use std::time::SystemTime;

use anyhow::Result;
use chrono::{NaiveDateTime, Utc};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use crate::structs::TrackingStatus;

lazy_static! {
    static ref GLOBAL_TRACKING_SERVICE: RwLock<Option<Arc<TrackingStatusService>>> = RwLock::new(None);
}

/// Enhanced tracking status with additional metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedTrackingStatus {
    pub is_tracking: bool,
    pub is_paused: bool,
    pub current_app: Option<String>,
    pub current_session_duration: i32,
    pub session_start_time: Option<NaiveDateTime>,
    pub active_checkpoint_ids: Vec<i32>,
    pub total_tracked_today: i32,
    pub last_updated: SystemTime,
    pub session_id: Option<String>,
}

impl From<TrackingStatus> for EnhancedTrackingStatus {
    fn from(status: TrackingStatus) -> Self {
        let session_start_time = status.session_start_time
            .as_deref()
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ").ok())
            .or_else(|| status.session_start_time
                .as_deref()
                .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok()));

        Self {
            is_tracking: status.is_tracking,
            is_paused: status.is_paused,
            current_app: status.current_app,
            current_session_duration: status.current_session_duration,
            session_start_time,
            active_checkpoint_ids: status.active_checkpoint_ids,
            total_tracked_today: 0, // Will be populated later
            last_updated: SystemTime::now(),
            session_id: None, // Will be generated if needed
        }
    }
}

impl Into<TrackingStatus> for EnhancedTrackingStatus {
    fn into(self) -> TrackingStatus {
        TrackingStatus {
            is_tracking: self.is_tracking,
            is_paused: self.is_paused,
            current_app: self.current_app,
            current_session_duration: self.current_session_duration,
            session_start_time: self.session_start_time
                .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.fZ").to_string()),
            active_checkpoint_ids: self.active_checkpoint_ids,
        }
    }
}

/// Tracking status change events
#[derive(Debug, Clone)]
pub enum TrackingEvent {
    SessionStarted {
        app_name: String,
        start_time: NaiveDateTime,
        checkpoint_ids: Vec<i32>,
    },
    SessionPaused {
        app_name: String,
        duration: i32,
    },
    SessionResumed {
        app_name: String,
    },
    SessionEnded {
        app_name: String,
        total_duration: i32,
        end_time: NaiveDateTime,
    },
    DurationUpdated {
        app_name: String,
        current_duration: i32,
    },
    CheckpointsChanged {
        app_name: Option<String>,
        checkpoint_ids: Vec<i32>,
    },
}

pub struct TrackingStatusService {
    current_status: Arc<RwLock<EnhancedTrackingStatus>>,
    event_broadcaster: broadcast::Sender<TrackingEvent>,
    status_broadcaster: broadcast::Sender<EnhancedTrackingStatus>,
    session_history: Arc<Mutex<Vec<TrackingSession>>>,
    active_checkpoints: Arc<RwLock<HashMap<i32, String>>>, // checkpoint_id -> app_name
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingSession {
    pub session_id: String,
    pub app_name: String,
    pub start_time: NaiveDateTime,
    pub end_time: Option<NaiveDateTime>,
    pub duration: i32,
    pub checkpoint_ids: Vec<i32>,
    pub was_paused: bool,
    pub pause_count: i32,
}

impl TrackingStatusService {
    pub fn new() -> Self {
        let (event_broadcaster, _) = broadcast::channel(1024);
        let (status_broadcaster, _) = broadcast::channel(512);
        
        let initial_status = EnhancedTrackingStatus {
            is_tracking: false,
            is_paused: false,
            current_app: None,
            current_session_duration: 0,
            session_start_time: None,
            active_checkpoint_ids: vec![],
            total_tracked_today: 0,
            last_updated: SystemTime::now(),
            session_id: None,
        };

        Self {
            current_status: Arc::new(RwLock::new(initial_status)),
            event_broadcaster,
            status_broadcaster,
            session_history: Arc::new(Mutex::new(Vec::new())),
            active_checkpoints: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize the global tracking service
    pub fn initialize() -> Arc<Self> {
        let service = Arc::new(Self::new());
        
        // Start background tasks
        service.start_background_tasks();
        
        // Register globally
        {
            let mut global_service = GLOBAL_TRACKING_SERVICE.write().unwrap();
            *global_service = Some(Arc::clone(&service));
        }
        
        info!("Tracking status service initialized");
        service
    }

    /// Get the global tracking service instance
    pub fn global() -> Option<Arc<Self>> {
        GLOBAL_TRACKING_SERVICE.read().unwrap().clone()
    }

    /// Start a new tracking session
    pub fn start_session(&self, app_name: String, checkpoint_ids: Vec<i32>) -> Result<()> {
        let start_time = Utc::now().naive_utc();
        let session_id = format!("{}_{}", app_name, start_time.and_utc().timestamp());

        // Update current status
        {
            let mut status = self.current_status.write().unwrap();
            status.is_tracking = true;
            status.is_paused = false;
            status.current_app = Some(app_name.clone());
            status.current_session_duration = 0;
            status.session_start_time = Some(start_time);
            status.active_checkpoint_ids = checkpoint_ids.clone();
            status.last_updated = SystemTime::now();
            status.session_id = Some(session_id.clone());
        }

        // Create session record
        let session = TrackingSession {
            session_id,
            app_name: app_name.clone(),
            start_time,
            end_time: None,
            duration: 0,
            checkpoint_ids: checkpoint_ids.clone(),
            was_paused: false,
            pause_count: 0,
        };

        {
            let mut history = self.session_history.lock().unwrap();
            history.push(session);
        }

        // Broadcast event
        let event = TrackingEvent::SessionStarted {
            app_name,
            start_time,
            checkpoint_ids,
        };

        if let Err(e) = self.event_broadcaster.send(event) {
            warn!("Failed to broadcast session start event: {}", e);
        }

        self.broadcast_status_update()?;
        
        info!("Started tracking session for app: {}", self.current_status.read().unwrap().current_app.as_deref().unwrap_or("Unknown"));
        Ok(())
    }

    /// End the current tracking session
    pub fn end_session(&self) -> Result<()> {
        let end_time = Utc::now().naive_utc();
        let (app_name, duration) = {
            let mut status = self.current_status.write().unwrap();
            let app_name = status.current_app.clone();
            let duration = status.current_session_duration;
            
            status.is_tracking = false;
            status.is_paused = false;
            status.current_app = None;
            status.current_session_duration = 0;
            status.session_start_time = None;
            status.active_checkpoint_ids.clear();
            status.last_updated = SystemTime::now();
            status.session_id = None;
            
            (app_name, duration)
        };

        // Update session history
        if let Some(app_name) = &app_name {
            let mut history = self.session_history.lock().unwrap();
            if let Some(session) = history.last_mut() {
                session.end_time = Some(end_time);
                session.duration = duration;
            }

            // Broadcast event
            let event = TrackingEvent::SessionEnded {
                app_name: app_name.clone(),
                total_duration: duration,
                end_time,
            };

            if let Err(e) = self.event_broadcaster.send(event) {
                warn!("Failed to broadcast session end event: {}", e);
            }
        }

        self.broadcast_status_update()?;
        
        info!("Ended tracking session for app: {}", app_name.as_deref().unwrap_or("Unknown"));
        Ok(())
    }

    /// Pause the current tracking session
    pub fn pause_session(&self) -> Result<()> {
        let (app_name, duration) = {
            let mut status = self.current_status.write().unwrap();
            if !status.is_tracking || status.is_paused {
                return Ok(()); // Already paused or not tracking
            }

            status.is_paused = true;
            status.last_updated = SystemTime::now();
            (status.current_app.clone(), status.current_session_duration)
        };

        // Update session history
        {
            let mut history = self.session_history.lock().unwrap();
            if let Some(session) = history.last_mut() {
                session.was_paused = true;
                session.pause_count += 1;
            }
        }

        if let Some(app_name) = app_name {
            let event = TrackingEvent::SessionPaused { app_name, duration };
            if let Err(e) = self.event_broadcaster.send(event) {
                warn!("Failed to broadcast session pause event: {}", e);
            }
        }

        self.broadcast_status_update()?;
        info!("Paused tracking session");
        Ok(())
    }

    /// Resume the current tracking session
    pub fn resume_session(&self) -> Result<()> {
        let app_name = {
            let mut status = self.current_status.write().unwrap();
            if !status.is_tracking || !status.is_paused {
                return Ok(()); // Not paused or not tracking
            }

            status.is_paused = false;
            status.last_updated = SystemTime::now();
            status.current_app.clone()
        };

        if let Some(app_name) = app_name {
            let event = TrackingEvent::SessionResumed { app_name };
            if let Err(e) = self.event_broadcaster.send(event) {
                warn!("Failed to broadcast session resume event: {}", e);
            }
        }

        self.broadcast_status_update()?;
        info!("Resumed tracking session");
        Ok(())
    }

    /// Update the current session duration
    pub fn update_duration(&self, new_duration: i32) -> Result<()> {
        let app_name = {
            let mut status = self.current_status.write().unwrap();
            if !status.is_tracking {
                return Ok(()); // Not tracking
            }

            status.current_session_duration = new_duration;
            status.last_updated = SystemTime::now();
            status.current_app.clone()
        };

        // Update session history
        {
            let mut history = self.session_history.lock().unwrap();
            if let Some(session) = history.last_mut() {
                session.duration = new_duration;
            }
        }

        if let Some(app_name) = app_name {
            let event = TrackingEvent::DurationUpdated {
                app_name,
                current_duration: new_duration,
            };
            if let Err(e) = self.event_broadcaster.send(event) {
                debug!("Failed to broadcast duration update: {}", e);
            }
        }

        // Only broadcast status updates every few seconds to avoid spam
        if new_duration % 5 == 0 {
            self.broadcast_status_update()?;
        }

        Ok(())
    }

    /// Update active checkpoints for current session
    pub fn update_checkpoints(&self, checkpoint_ids: Vec<i32>) -> Result<()> {
        let app_name = {
            let mut status = self.current_status.write().unwrap();
            status.active_checkpoint_ids = checkpoint_ids.clone();
            status.last_updated = SystemTime::now();
            status.current_app.clone()
        };

        // Update session history
        {
            let mut history = self.session_history.lock().unwrap();
            if let Some(session) = history.last_mut() {
                session.checkpoint_ids = checkpoint_ids.clone();
            }
        }

        let event = TrackingEvent::CheckpointsChanged {
            app_name,
            checkpoint_ids,
        };

        if let Err(e) = self.event_broadcaster.send(event) {
            warn!("Failed to broadcast checkpoint change event: {}", e);
        }

        self.broadcast_status_update()?;
        Ok(())
    }

    /// Get current tracking status
    pub fn get_status(&self) -> EnhancedTrackingStatus {
        self.current_status.read().unwrap().clone()
    }

    /// Get current status as legacy TrackingStatus
    pub fn get_legacy_status(&self) -> TrackingStatus {
        self.get_status().into()
    }

    /// Update status from external TrackingStatus (for compatibility)
    pub fn update_from_legacy(&self, status: TrackingStatus) -> Result<()> {
        let enhanced: EnhancedTrackingStatus = status.into();
        
        {
            let mut current = self.current_status.write().unwrap();
            *current = enhanced;
        }

        self.broadcast_status_update()?;
        Ok(())
    }

    /// Subscribe to tracking events
    pub fn subscribe_events(&self) -> broadcast::Receiver<TrackingEvent> {
        self.event_broadcaster.subscribe()
    }

    /// Subscribe to status updates
    pub fn subscribe_status(&self) -> broadcast::Receiver<EnhancedTrackingStatus> {
        self.status_broadcaster.subscribe()
    }

    /// Get session history
    pub fn get_session_history(&self) -> Vec<TrackingSession> {
        self.session_history.lock().unwrap().clone()
    }

    /// Get today's total tracked time
    pub fn get_today_total(&self) -> i32 {
        let today = Utc::now().naive_utc().date();
        let history = self.session_history.lock().unwrap();
        
        history.iter()
            .filter(|session| session.start_time.date() == today)
            .map(|session| session.duration)
            .sum()
    }

    /// Broadcast status update to subscribers
    fn broadcast_status_update(&self) -> Result<()> {
        let status = self.get_status();
        
        if let Err(e) = self.status_broadcaster.send(status) {
            debug!("Failed to broadcast status update: {}", e);
        }
        
        Ok(())
    }

    /// Start background tasks
    fn start_background_tasks(&self) {
        let service = self;
        
        // Task to update today's total periodically
        {
            let current_status = Arc::clone(&service.current_status);
            let session_history = Arc::clone(&service.session_history);
            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    
                    let today = Utc::now().naive_utc().date();
                    let total_today = {
                        let history = session_history.lock().unwrap();
                        history.iter()
                            .filter(|session| session.start_time.date() == today)
                            .map(|session| session.duration)
                            .sum()
                    };

                    {
                        let mut status = current_status.write().unwrap();
                        status.total_tracked_today = total_today;
                    }
                }
            });
        }

        // Task to clean old session history (keep last 1000 sessions)
        {
            let session_history = Arc::clone(&service.session_history);
            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(3600)); // Every hour
                loop {
                    interval.tick().await;
                    
                    let mut history = session_history.lock().unwrap();
                    if history.len() > 1000 {
                        let keep_from = history.len() - 1000;
                        *history = history.split_off(keep_from);
                        debug!("Cleaned old session history, keeping last 1000 sessions");
                    }
                }
            });
        }
    }
}

/// Integration functions for existing time tracking system
impl TrackingStatusService {
    /// Called when a process starts being tracked
    pub fn on_process_started(&self, process_name: &str) {
        info!("Process tracking started: {}", process_name);
        if let Err(e) = self.start_session(process_name.to_string(), vec![]) {
            error!("Failed to start tracking session: {}", e);
        }
    }

    /// Called when a process stops being tracked
    pub fn on_process_ended(&self, process_name: &str) {
        info!("Process tracking ended: {}", process_name);
        if let Err(e) = self.end_session() {
            error!("Failed to end tracking session: {}", e);
        }
    }

    /// Called when tracking duration is updated
    pub fn on_duration_update(&self, _process_name: &str, duration: i32) {
        if let Err(e) = self.update_duration(duration) {
            error!("Failed to update session duration: {}", e);
        }
    }

    /// Called when checkpoints are activated/deactivated
    pub fn on_checkpoints_changed(&self, checkpoint_ids: Vec<i32>) {
        if let Err(e) = self.update_checkpoints(checkpoint_ids) {
            error!("Failed to update checkpoints: {}", e);
        }
    }

    /// Called when tracking is paused/resumed globally
    pub fn on_tracking_paused(&self, is_paused: bool) {
        if is_paused {
            if let Err(e) = self.pause_session() {
                error!("Failed to pause session: {}", e);
            }
        } else {
            if let Err(e) = self.resume_session() {
                error!("Failed to resume session: {}", e);
            }
        }
    }
}

/// Global functions for integration with existing system
pub fn initialize_tracking_service() -> Arc<TrackingStatusService> {
    TrackingStatusService::initialize()
}

pub fn get_global_tracking_service() -> Option<Arc<TrackingStatusService>> {
    TrackingStatusService::global()
}

pub fn get_current_tracking_status() -> Option<TrackingStatus> {
    TrackingStatusService::global().map(|service| service.get_legacy_status())
}

pub fn update_tracking_status_legacy(status: TrackingStatus) {
    if let Some(service) = TrackingStatusService::global() {
        if let Err(e) = service.update_from_legacy(status) {
            error!("Failed to update tracking status: {}", e);
        }
    }
}

pub fn notify_process_started(process_name: &str) {
    if let Some(service) = TrackingStatusService::global() {
        service.on_process_started(process_name);
    }
}

pub fn notify_process_ended(process_name: &str) {
    if let Some(service) = TrackingStatusService::global() {
        service.on_process_ended(process_name);
    }
}

pub fn notify_duration_update(process_name: &str, duration: i32) {
    if let Some(service) = TrackingStatusService::global() {
        service.on_duration_update(process_name, duration);
    }
}

pub fn notify_checkpoints_changed(checkpoint_ids: Vec<i32>) {
    if let Some(service) = TrackingStatusService::global() {
        service.on_checkpoints_changed(checkpoint_ids);
    }
}

pub fn notify_tracking_paused(is_paused: bool) {
    if let Some(service) = TrackingStatusService::global() {
        service.on_tracking_paused(is_paused);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tracking_service_basic_flow() {
        let service = TrackingStatusService::new();
        
        // Initially not tracking
        let status = service.get_status();
        assert!(!status.is_tracking);
        assert!(!status.is_paused);
        assert!(status.current_app.is_none());
        
        // Start tracking
        service.start_session("test_app".to_string(), vec![1, 2]).unwrap();
        let status = service.get_status();
        assert!(status.is_tracking);
        assert!(!status.is_paused);
        assert_eq!(status.current_app, Some("test_app".to_string()));
        assert_eq!(status.active_checkpoint_ids, vec![1, 2]);
        
        // Update duration
        service.update_duration(60).unwrap();
        let status = service.get_status();
        assert_eq!(status.current_session_duration, 60);
        
        // Pause
        service.pause_session().unwrap();
        let status = service.get_status();
        assert!(status.is_tracking);
        assert!(status.is_paused);
        
        // Resume
        service.resume_session().unwrap();
        let status = service.get_status();
        assert!(status.is_tracking);
        assert!(!status.is_paused);
        
        // End session
        service.end_session().unwrap();
        let status = service.get_status();
        assert!(!status.is_tracking);
        assert!(!status.is_paused);
        assert!(status.current_app.is_none());
        
        // Check session history
        let history = service.get_session_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].app_name, "test_app");
        assert_eq!(history[0].duration, 60);
        assert!(history[0].was_paused);
        assert_eq!(history[0].pause_count, 1);
    }
}
