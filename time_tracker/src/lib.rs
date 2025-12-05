//! TimeTracker library crate.
//! This crate provides the core functionality for the TimeTracker application,
//! including database access, REST API implementations, and time tracking utilities.

extern crate lazy_static;

// Export the main modules
pub mod entities;
pub mod migration;
pub mod seaorm_client;
pub mod seaorm_queries;
pub mod receive_types;
pub mod restable;
pub mod structs;

// Export test modules for use in tests
pub mod test_db;
pub mod test_client;

// Re-export commonly used types
pub use seaorm_client::SeaORMClient;

// Export time_tracking module for tests
pub mod time_tracking;

// Export other modules that time_tracking depends on
pub mod error;
pub mod hook;
pub mod native;
pub mod rpc;
pub mod websocket;
pub mod linux;

#[macro_use]
extern crate rust_embed;

#[derive(RustEmbed)]
#[folder = "resource/"]
pub struct Asset;
