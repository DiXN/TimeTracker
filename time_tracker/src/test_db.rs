//! Test database utilities for creating in-memory SQLite databases for testing.

use sea_orm::{Database, DatabaseConnection, DbErr};
use std::sync::Arc;

/// Create an in-memory SQLite database connection for testing.
pub async fn create_test_db() -> Result<Arc<DatabaseConnection>, DbErr> {
    println!("Creating in-memory SQLite database");
    let db = Database::connect("sqlite::memory:").await?;
    Ok(Arc::new(db))
}

/// Initialize the test database with the required schema.
pub async fn initialize_test_db(db: &DatabaseConnection) -> Result<(), DbErr> {
    println!("Initializing test database schema");
    use sea_orm_migration::MigratorTrait;
    crate::migration::Migrator::up(db, None).await?;
    Ok(())
}

/// Create and initialize a test database.
pub async fn create_and_initialize_test_db() -> Result<Arc<DatabaseConnection>, DbErr> {
    println!("Creating and initializing test database");
    let db = create_test_db().await?;
    initialize_test_db(&*db).await?;
    Ok(db)
}

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