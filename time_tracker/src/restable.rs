use serde_json::Value;
use std::error::Error;
use async_trait::async_trait;

use crossbeam_channel::Receiver;

use crate::receive_types::ReceiveTypes;

#[async_trait]
pub trait Restable {
    async fn get_data(&self, item: &str) -> Result<Value, Box<dyn Error>>;
    async fn get_processes(&self) -> Result<Vec<String>, Box<dyn Error>>;
    async fn put_data(&self, item: &str, product_name: &str) -> Result<Value, Box<dyn Error>>;
    async fn delete_data(&self, item: &str) -> Result<Value, Box<dyn Error>>;
    fn init_event_loop(self, recv: Receiver<(String, ReceiveTypes)>);
    async fn setup(&self) -> Result<(), Box<dyn Error>>;

    async fn get_all_apps(&self) -> Result<Value, Box<dyn Error>>;
    async fn get_timeline_data(&self, app_name: Option<&str>, days: i64)
    -> Result<Value, Box<dyn Error>>;
    async fn get_all_timeline(&self) -> Result<Value, Box<dyn Error>>;
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
    async fn get_timeline_checkpoint_associations(&self) -> Result<Value, Box<dyn Error>>;

    // Timeline checkpoint methods
    async fn create_timeline_checkpoint(
        &self,
        timeline_id: i32,
        checkpoint_id: i32,
    ) -> Result<Value, Box<dyn Error>>;
    async fn delete_timeline_checkpoint(
        &self,
        timeline_id: i32,
        checkpoint_id: i32,
    ) -> Result<Value, Box<dyn Error>>;
    async fn get_timeline_checkpoints_for_timeline(
        &self,
        timeline_id: i32,
    ) -> Result<Value, Box<dyn Error>>;

    // Checkpoint duration methods
    async fn get_checkpoint_durations(&self) -> Result<Value, Box<dyn Error>>;
    async fn get_checkpoint_durations_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>>;
    async fn update_checkpoint_duration(
        &self,
        checkpoint_id: i32,
        app_id: i32,
        duration: i32,
        sessions_count: i32,
    ) -> Result<Value, Box<dyn Error>>;

    // Active checkpoint table methods
    async fn get_all_active_checkpoints_table(&self) -> Result<Value, Box<dyn Error>>;
    async fn get_active_checkpoints_for_app_table(&self, app_id: i32) -> Result<Value, Box<dyn Error>>;
    async fn activate_checkpoint(&self, checkpoint_id: i32, app_id: i32)
    -> Result<Value, Box<dyn Error>>;
    async fn deactivate_checkpoint(
        &self,
        checkpoint_id: i32,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>>;
    async fn is_checkpoint_active(&self, checkpoint_id: i32, app_id: i32)
    -> Result<bool, Box<dyn Error>>;

    // Checkpoint stats method
    async fn get_checkpoint_durations_by_ids(&self, checkpoint_ids: &[i32]) -> Result<Value, Box<dyn Error>>;
}
