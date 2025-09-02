//! Test database utilities for creating in-memory SQLite databases for testing.

// These imports are only used in tests, so we'll mark them as such
#[cfg(test)]
use sea_orm::{Database, DatabaseConnection, DbErr};
#[cfg(test)]
use std::sync::Arc;




#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_db() {
        println!("Running test_create_test_db");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt.block_on(create_test_db());
        assert!(db.is_ok());
        println!("test_create_test_db passed");
    }

    #[test]
    fn test_create_and_initialize_test_db() {
        println!("Running test_create_and_initialize_test_db");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt.block_on(create_and_initialize_test_db());
        assert!(db.is_ok());
        println!("test_create_and_initialize_test_db passed");
    }
}