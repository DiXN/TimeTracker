use std::sync::{Arc, RwLock, Weak};
use lazy_static::lazy_static;
use crate::structs::{TrackingStatus, App, Timeline};
use crate::restable::Restable;
use super::server::ServerState;

pub trait TrackingStatusNotifier: Send + Sync {
    fn notify_tracking_status(&self, status: TrackingStatus);
}

pub trait DataBroadcaster: Send + Sync {
    fn broadcast_apps_update(&self, apps: Vec<App>);
    fn broadcast_timeline_update(&self, timeline: Vec<Timeline>);
    fn broadcast_tracking_status(&self, status: TrackingStatus);
}

struct WebSocketNotifier<T: Restable> {
    server_state: Weak<RwLock<ServerState<T>>>,
}

impl<T: Restable + Sync + Send> TrackingStatusNotifier for WebSocketNotifier<T> {
    fn notify_tracking_status(&self, status: TrackingStatus) {
        if let Some(state) = self.server_state.upgrade() {
            if let Ok(state_guard) = state.read() {
                state_guard.broadcast_tracking_status(status);
            }
        }
    }
}

impl<T: Restable + Sync + Send> DataBroadcaster for WebSocketNotifier<T> {
    fn broadcast_apps_update(&self, apps: Vec<App>) {
        if let Some(state) = self.server_state.upgrade() {
            if let Ok(state_guard) = state.read() {
                state_guard.broadcast_apps_update(apps);
            }
        }
    }

    fn broadcast_timeline_update(&self, timeline: Vec<Timeline>) {
        if let Some(state) = self.server_state.upgrade() {
            if let Ok(state_guard) = state.read() {
                state_guard.broadcast_timeline_update(timeline);
            }
        }
    }

    fn broadcast_tracking_status(&self, status: TrackingStatus) {
        if let Some(state) = self.server_state.upgrade() {
            if let Ok(state_guard) = state.read() {
                state_guard.broadcast_tracking_status(status);
            }
        }
    }
}

lazy_static! {
    static ref NOTIFIER: RwLock<Option<Box<dyn TrackingStatusNotifier>>> = RwLock::new(None);
    static ref BROADCASTER: RwLock<Option<Box<dyn DataBroadcaster>>> = RwLock::new(None);
}

pub fn register_websocket_notifier<T: Restable + Sync + Send + 'static>(
    server_state: &Arc<RwLock<ServerState<T>>>,
) {
    let notifier = WebSocketNotifier {
        server_state: Arc::downgrade(server_state),
    };

    // Register both the notifier and broadcaster
    {
        let mut global_notifier = NOTIFIER.write().unwrap();
        *global_notifier = Some(Box::new(notifier));
    }

    let broadcaster = WebSocketNotifier {
        server_state: Arc::downgrade(server_state),
    };

    {
        let mut global_broadcaster = BROADCASTER.write().unwrap();
        *global_broadcaster = Some(Box::new(broadcaster));
    }
}

pub fn notify_tracking_status(status: TrackingStatus) {
    if let Ok(notifier_guard) = NOTIFIER.read() {
        if let Some(notifier) = notifier_guard.as_ref() {
            notifier.notify_tracking_status(status);
        }
    }
}

pub fn has_active_notifier() -> bool {
    NOTIFIER.read().map(|guard| guard.is_some()).unwrap_or(false)
}

pub fn has_active_broadcaster() -> bool {
    BROADCASTER.read().map(|guard| guard.is_some()).unwrap_or(false)
}

pub fn broadcast_apps_update(apps: Vec<App>) {
    if let Ok(broadcaster_guard) = BROADCASTER.read() {
        if let Some(broadcaster) = broadcaster_guard.as_ref() {
            broadcaster.broadcast_apps_update(apps);
        }
    }
}

pub fn broadcast_timeline_update(timeline: Vec<Timeline>) {
    if let Ok(broadcaster_guard) = BROADCASTER.read() {
        if let Some(broadcaster) = broadcaster_guard.as_ref() {
            broadcaster.broadcast_timeline_update(timeline);
        }
    }
}

pub fn broadcast_tracking_status_update(status: TrackingStatus) {
    if let Ok(broadcaster_guard) = BROADCASTER.read() {
        if let Some(broadcaster) = broadcaster_guard.as_ref() {
            broadcaster.broadcast_tracking_status(status);
        }
    }
}
