use std::sync::{Arc, RwLock};
use crate::restable::Restable;
use crate::structs::TrackingStatus;

mod server;
mod client;
mod broadcast;
mod handlers;
mod tracking_notifier;

pub use server::{ServerState, init_web_socket};
pub use tracking_notifier::{notify_tracking_status, has_active_notifier};

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
