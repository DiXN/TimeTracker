use std::sync::{Arc, RwLock, Weak};

use lazy_static::lazy_static;

use super::server::ServerState;
use crate::restable::Restable;
use crate::structs::{App, Timeline, TrackingStatus};

#[allow(dead_code)]
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

impl<T: Restable> WebSocketNotifier<T> {
    fn with_state<F>(&self, f: F)
    where
        F: FnOnce(&ServerState<T>),
    {
        if let Some(state) = self.server_state.upgrade()
            && let Ok(guard) = state.read()
        {
            f(&guard);
        }
    }
}

impl<T: Restable + Sync + Send> TrackingStatusNotifier for WebSocketNotifier<T> {
    fn notify_tracking_status(&self, status: TrackingStatus) {
        self.with_state(|s| s.broadcast_tracking_status(status));
    }
}

impl<T: Restable + Sync + Send> DataBroadcaster for WebSocketNotifier<T> {
    fn broadcast_apps_update(&self, apps: Vec<App>) {
        self.with_state(|s| s.broadcast_apps_update(apps));
    }

    fn broadcast_timeline_update(&self, timeline: Vec<Timeline>) {
        self.with_state(|s| s.broadcast_timeline_update(timeline));
    }

    fn broadcast_tracking_status(&self, status: TrackingStatus) {
        self.with_state(|s| s.broadcast_tracking_status(status));
    }
}

lazy_static! {
    static ref NOTIFIER: RwLock<Option<Box<dyn TrackingStatusNotifier>>> = RwLock::new(None);
    static ref BROADCASTER: RwLock<Option<Box<dyn DataBroadcaster>>> = RwLock::new(None);
}

pub fn register_websocket_notifier<T: Restable + Sync + Send + 'static>(
    server_state: &Arc<RwLock<ServerState<T>>>,
) {
    let weak = Arc::downgrade(server_state);

    if let Ok(mut global_notifier) = NOTIFIER.write() {
        *global_notifier = Some(Box::new(WebSocketNotifier {
            server_state: weak.clone(),
        }));
    }

    if let Ok(mut global_broadcaster) = BROADCASTER.write() {
        *global_broadcaster = Some(Box::new(WebSocketNotifier {
            server_state: weak,
        }));
    }
}

#[allow(dead_code)]
pub fn notify_tracking_status(status: TrackingStatus) {
    if let Ok(guard) = NOTIFIER.read()
        && let Some(notifier) = guard.as_ref()
    {
        notifier.notify_tracking_status(status);
    }
}

#[allow(dead_code)]
pub fn has_active_notifier() -> bool {
    NOTIFIER.read().map(|g| g.is_some()).unwrap_or(false)
}

pub fn has_active_broadcaster() -> bool {
    BROADCASTER.read().map(|g| g.is_some()).unwrap_or(false)
}

pub fn broadcast_apps_update(apps: Vec<App>) {
    if let Ok(guard) = BROADCASTER.read()
        && let Some(broadcaster) = guard.as_ref()
    {
        broadcaster.broadcast_apps_update(apps);
    }
}

pub fn broadcast_timeline_update(timeline: Vec<Timeline>) {
    if let Ok(guard) = BROADCASTER.read()
        && let Some(broadcaster) = guard.as_ref()
    {
        broadcaster.broadcast_timeline_update(timeline);
    }
}

pub fn broadcast_tracking_status_update(status: TrackingStatus) {
    if let Ok(guard) = BROADCASTER.read()
        && let Some(broadcaster) = guard.as_ref()
    {
        broadcaster.broadcast_tracking_status(status);
    }
}
