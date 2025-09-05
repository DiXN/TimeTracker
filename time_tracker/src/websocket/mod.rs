//! Enhanced WebSocket system for real-time time tracking updates
//!
//! This module provides an async-first WebSocket server with intelligent caching,
//! real-time broadcasting, and session management capabilities.

mod async_server;
mod connection_manager;
mod integration_example;
mod tracking_service;

pub use integration_example::WebSocketSystem;
pub use tracking_service::{
    initialize_tracking_service, notify_process_started, notify_process_ended,
    notify_duration_update, notify_tracking_paused
};
