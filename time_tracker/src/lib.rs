//! TimeTracker library crate.
//! This crate provides the core functionality for the TimeTracker application,
//! including database access, REST API implementations, and time tracking utilities.

// Export the main modules
pub mod entities;
pub mod migration;
pub mod seaorm_client;
pub mod seaorm_queries;
pub mod receive_types;
pub mod restable;

// Export test modules for use in tests
pub mod test_db;
pub mod test_client;

// Re-export commonly used types
pub use seaorm_client::SeaORMClient;
