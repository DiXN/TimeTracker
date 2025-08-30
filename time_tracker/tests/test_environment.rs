//! Comprehensive test environment setup for TimeTracker application.
//! This module provides utilities for setting up an in-memory SQLite database
//! for testing, with the same schema as the production database.

use std::sync::Arc;
use sea_orm::DatabaseConnection;
use time_tracker::{test_client::TestClient, test_db::{create_and_initialize_test_db, create_test_db}, restable::Restable};

/// A comprehensive test environment that includes a database connection
/// and a test client ready for use in tests.
pub struct TestEnvironment {
    /// The database connection (in-memory SQLite)
    pub db: Arc<DatabaseConnection>,
    /// The test client that uses the database connection
    pub client: TestClient,
}

impl TestEnvironment {
    /// Create a new test environment with a fully initialized database.
    /// This function creates an in-memory SQLite database, runs all migrations,
    /// and creates a test client ready for use in tests.
    ///
    /// # Returns
    ///
    /// A Result containing the TestEnvironment or an error.
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let db = create_and_initialize_test_db().await?;
        let client = TestClient::new_with_connection(db.clone());

        // Run setup to ensure the database is ready
        client.setup().await?;

        Ok(Self { db, client })
    }

    /// Create a new test environment with a database but without running setup.
    /// This is useful when you want to customize the setup process or when
    /// setup is not required for a specific test.
    ///
    /// # Returns
    ///
    /// A Result containing the TestEnvironment or an error.
    pub async fn new_without_setup() -> Result<Self, Box<dyn std::error::Error>> {
        let db = create_and_initialize_test_db().await?;
        let client = TestClient::new_with_connection(db.clone());

        Ok(Self { db, client })
    }

    /// Get a reference to the database connection.
    ///
    /// # Returns
    ///
    /// A reference to the Arc-wrapped DatabaseConnection.
    pub fn db(&self) -> &Arc<DatabaseConnection> {
        &self.db
    }

    /// Get a reference to the test client.
    ///
    /// # Returns
    ///
    /// A reference to the TestClient.
    pub fn client(&self) -> &TestClient {
        &self.client
    }
}

/// A builder for creating customized test environments.
/// This allows for more fine-grained control over the test environment setup.
pub struct TestEnvironmentBuilder {
    /// Whether to initialize the database with migrations
    initialize_db: bool,
    /// Whether to run the client setup
    run_setup: bool,
}

impl TestEnvironmentBuilder {
    /// Create a new TestEnvironmentBuilder with default settings.
    /// By default, the database will be initialized and setup will be run.
    pub fn new() -> Self {
        Self {
            initialize_db: true,
            run_setup: true,
        }
    }

    /// Set whether to initialize the database with migrations.
    ///
    /// # Arguments
    ///
    /// * `initialize` - Whether to initialize the database
    pub fn initialize_db(mut self, initialize: bool) -> Self {
        self.initialize_db = initialize;
        self
    }

    /// Set whether to run the client setup.
    ///
    /// # Arguments
    ///
    /// * `setup` - Whether to run the client setup
    pub fn run_setup(mut self, setup: bool) -> Self {
        self.run_setup = setup;
        self
    }

    /// Build the test environment with the specified settings.
    ///
    /// # Returns
    ///
    /// A Result containing the TestEnvironment or an error.
    pub async fn build(self) -> Result<TestEnvironment, Box<dyn std::error::Error>> {
        let db = if self.initialize_db {
            create_and_initialize_test_db().await?
        } else {
            create_test_db().await?
        };

        let client = TestClient::new_with_connection(db.clone());

        if self.run_setup {
            client.setup().await?;
        }

        Ok(TestEnvironment { db, client })
    }
}

impl Default for TestEnvironmentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    async fn test_create_test_environment() {
        let env = TestEnvironment::new().await;
        assert!(env.is_ok());
    }

    #[cfg(test)]
    async fn test_create_test_environment_without_setup() {
        let env = TestEnvironment::new_without_setup().await;
        assert!(env.is_ok());
    }

    #[cfg(test)]
    async fn test_test_environment_builder() {
        let env = TestEnvironmentBuilder::new()
            .initialize_db(true)
            .run_setup(false)
            .build()
            .await;
        assert!(env.is_ok());
    }

    #[cfg(test)]
    async fn test_test_environment_builder_default() {
        let env = TestEnvironmentBuilder::default()
            .build()
            .await;
        assert!(env.is_ok());
    }
}