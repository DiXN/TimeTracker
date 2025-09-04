use std::sync::{Arc, RwLock, Weak};
use lazy_static::lazy_static;
use crate::structs::TrackingStatus;
use crate::restable::Restable;
use super::server::ServerState;

pub trait TrackingStatusNotifier: Send + Sync {
    fn notify_tracking_status(&self, status: TrackingStatus);
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

lazy_static! {
    static ref NOTIFIER: RwLock<Option<Box<dyn TrackingStatusNotifier>>> = RwLock::new(None);
}

pub fn register_websocket_notifier<T: Restable + Sync + Send + 'static>(
    server_state: &Arc<RwLock<ServerState<T>>>,
) {
    let notifier = WebSocketNotifier {
        server_state: Arc::downgrade(server_state),
    };

    let mut global_notifier = NOTIFIER.write().unwrap();
    *global_notifier = Some(Box::new(notifier));
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
