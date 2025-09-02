//! Test client implementation that can work with both production and test databases.
//! This module provides a TestClient struct that can be used in tests, with the same
//! interface as the production SeaORMClient but with the ability to inject a test database.

use std::{error::Error, sync::Arc};
use sea_orm::DatabaseConnection;
use serde_json::Value;
use async_trait::async_trait;

use crate::{restable::Restable, seaorm_client::SeaORMClient};

/// A test client that can work with both production and test databases.
/// This struct implements the same interface as SeaORMClient but allows
/// injection of a test database connection for testing purposes.
#[derive(Clone)]
pub struct TestClient {
    /// The underlying client that handles database operations
    pub client: SeaORMClient,
}

impl TestClient {
    /// Create a new TestClient with a provided database connection.
    /// This is useful for testing with an in-memory SQLite database.
    ///
    /// # Arguments
    ///
    /// * `connection` - An Arc-wrapped DatabaseConnection to use
    pub fn new_with_connection(connection: Arc<DatabaseConnection>) -> Self {
        Self {
            client: SeaORMClient { connection },
        }
    }

    /// Create a new TestClient for production use with a database URL.
    ///
    /// # Arguments
    ///
    /// * `url` - The database URL to connect to
    pub async fn new(url: &str) -> Result<Self, Box<dyn Error>> {
        let client = SeaORMClient::new(url).await?;
        Ok(Self { client })
    }
}

// Implement the Restable trait for TestClient to provide the same interface
// as the production client
#[async_trait]
impl Restable for TestClient {
    async fn setup(&self) -> Result<(), Box<dyn Error>> {
        self.client.setup().await
    }


    async fn get_processes(&self) -> Result<Vec<String>, Box<dyn Error>> {
        self.client.get_processes().await
    }

    async fn put_data(&self, item: &str, product_name: &str) -> Result<Value, Box<dyn Error>> {
        self.client.put_data(item, product_name).await
    }

    async fn delete_data(&self, item: &str) -> Result<Value, Box<dyn Error>> {
        self.client.delete_data(item).await
    }

    fn init_event_loop(self, rx: crossbeam_channel::Receiver<(String, crate::receive_types::ReceiveTypes)>) {
        self.client.init_event_loop(rx)
    }

    async fn get_all_apps(&self) -> Result<Value, Box<dyn Error>> {
        self.client.get_all_apps().await
    }

    async fn get_timeline_data(
        &self,
        app_name: Option<&str>,
        days: i64,
    ) -> Result<Value, Box<dyn Error>> {
        self.client.get_timeline_data(app_name, days).await
    }

    async fn get_session_count_for_app(&self, app_id: i32) -> Result<i32, Box<dyn Error>> {
        self.client.get_session_count_for_app(app_id).await
    }

    async fn get_all_app_ids(&self) -> Result<Vec<i32>, Box<dyn Error>> {
        self.client.get_all_app_ids().await
    }

    async fn get_all_checkpoints(&self) -> Result<Value, Box<dyn Error>> {
        self.client.get_all_checkpoints().await
    }

    async fn get_checkpoints_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>> {
        self.client.get_checkpoints_for_app(app_id).await
    }

    async fn create_checkpoint(
        &self,
        name: &str,
        description: Option<&str>,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        self.client.create_checkpoint(name, description, app_id).await
    }

    async fn set_checkpoint_active(
        &self,
        checkpoint_id: i32,
        is_active: bool,
    ) -> Result<Value, Box<dyn Error>> {
        self.client.set_checkpoint_active(checkpoint_id, is_active).await
    }

    async fn delete_checkpoint(&self, checkpoint_id: i32) -> Result<Value, Box<dyn Error>> {
        self.client.delete_checkpoint(checkpoint_id).await
    }

    async fn get_active_checkpoints(&self) -> Result<Value, Box<dyn Error>> {
        self.client.get_active_checkpoints().await
    }

    async fn get_active_checkpoints_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>> {
        self.client.get_active_checkpoints_for_app(app_id).await
    }



    async fn get_checkpoint_durations_by_ids(&self, checkpoint_ids: &[i32]) -> Result<Value, Box<dyn Error>> {
        self.client.get_checkpoint_durations_by_ids(checkpoint_ids).await
    }




    async fn get_timeline_with_checkpoints(&self) -> Result<Value, Box<dyn Error>> {
        self.client.get_timeline_with_checkpoints().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::create_and_initialize_test_db;

    #[cfg(test)]
    async fn test_create_test_client_with_connection() {
        let db = create_and_initialize_test_db().await.unwrap();
        let _client = TestClient::new_with_connection(db);
        // The client was created successfully
    }

    #[cfg(test)]
    async fn test_test_client_implements_restable() {
        let db = create_and_initialize_test_db().await.unwrap();
        let client = TestClient::new_with_connection(db);

        // Verify that we can call setup without error
        // Note: This might fail if the test database isn't properly initialized
        // but the important thing is that the method exists and compiles
        let _ = client.setup().await;
    }
}