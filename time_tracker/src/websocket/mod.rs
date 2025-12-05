use std::sync::{Arc, RwLock};

use crate::restable::Restable;
use crate::structs::TrackingStatus;

mod broadcast;
mod broadcast_logger;
mod client;
mod handlers;
mod server;
mod tracking_notifier;

pub use broadcast_logger::{
    broadcast_apps_update, broadcast_timeline_update, broadcast_tracking_status_update,
};
pub use server::{init_web_socket, ServerState};
pub use tracking_notifier::has_active_broadcaster;

pub fn update_tracking_status<T>(state: &Arc<RwLock<ServerState<T>>>, new_status: TrackingStatus)
where
    T: Restable + Sync + Send,
{
    if let Ok(state_guard) = state.read() {
        state_guard.broadcast_tracking_status(new_status);
    }
}
