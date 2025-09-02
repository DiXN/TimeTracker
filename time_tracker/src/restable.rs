use serde_json::Value;
use std::error::Error;
use async_trait::async_trait;

use crossbeam_channel::Receiver;

use crate::receive_types::ReceiveTypes;

#[async_trait]
pub trait Restable {
    async fn get_processes(&self) -> Result<Vec<String>, Box<dyn Error>>;
    async fn put_data(&self, item: &str, product_name: &str) -> Result<Value, Box<dyn Error>>;
    async fn delete_data(&self, item: &str) -> Result<Value, Box<dyn Error>>;
    fn init_event_loop(self, recv: Receiver<(String, ReceiveTypes)>);
    async fn setup(&self) -> Result<(), Box<dyn Error>>;

    async fn get_all_apps(&self) -> Result<Value, Box<dyn Error>>;
    async fn get_timeline_data(&self, app_name: Option<&str>, days: i64)
    -> Result<Value, Box<dyn Error>>;
    async fn get_session_count_for_app(&self, app_id: i32) -> Result<i32, Box<dyn Error>>;
    async fn get_all_app_ids(&self) -> Result<Vec<i32>, Box<dyn Error>>;

    // Checkpoint methods
    async fn get_all_checkpoints(&self) -> Result<Value, Box<dyn Error>>;
    async fn get_checkpoints_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>>;
    async fn create_checkpoint(
        &self,
        name: &str,
        description: Option<&str>,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>>;
    async fn set_checkpoint_active(
        &self,
        checkpoint_id: i32,
        is_active: bool,
    ) -> Result<Value, Box<dyn Error>>;
    async fn delete_checkpoint(&self, checkpoint_id: i32) -> Result<Value, Box<dyn Error>>;
    async fn get_active_checkpoints(&self) -> Result<Value, Box<dyn Error>>;
    async fn get_active_checkpoints_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>>;

    // Checkpoint stats method
    async fn get_checkpoint_durations_by_ids(&self, checkpoint_ids: &[i32]) -> Result<Value, Box<dyn Error>>;

    // Timeline with checkpoints method
    async fn get_timeline_with_checkpoints(&self) -> Result<Value, Box<dyn Error>>;
}
