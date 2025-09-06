use std::sync::{Arc, RwLock};
use crate::restable::Restable;
use crate::structs::{TrackingStatus, App, Timeline};

mod server;
mod client;
mod broadcast;
mod handlers;
mod tracking_notifier;
mod broadcast_logger;

pub use server::{ServerState, init_web_socket};
pub use tracking_notifier::{
    notify_tracking_status, has_active_notifier, has_active_broadcaster,
};
pub use broadcast_logger::{
    broadcast_apps_update, broadcast_timeline_update, broadcast_tracking_status_update,
    log_broadcast_status, get_broadcast_stats, with_broadcast_logging
};
pub use broadcast::{SubscriptionTopic, SubscriptionBroadcaster};

// Legacy function - kept for backward compatibility but not recommended
// Use broadcast_tracking_status_update() instead for subscription-based broadcasting
pub fn update_tracking_status<T>(
    state: &Arc<RwLock<ServerState<T>>>,
    new_status: TrackingStatus,
) where
    T: Restable + Sync + Send,
{
    if let Ok(state_guard) = state.read() {
        state_guard.broadcast_tracking_status(new_status);
    }
}
