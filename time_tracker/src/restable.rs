use serde_json::Value;
use std::error::Error;

use crossbeam_channel::Receiver;

use crate::receive_types::ReceiveTypes;

pub trait Restable {
    fn get_data(&self, item: &str) -> Result<Value, Box<dyn Error>>;
    fn get_processes(&self) -> Result<Vec<String>, Box<dyn Error>>;
    fn put_data(&self, item: &str, product_name: &str) -> Result<Value, Box<dyn Error>>;
    fn delete_data(&self, item: &str) -> Result<Value, Box<dyn Error>>;
    fn init_event_loop(self, recv: Receiver<(String, ReceiveTypes)>);
    fn setup(&self) -> Result<(), Box<dyn Error>>;

    fn get_all_apps(&self) -> Result<Value, Box<dyn Error>>;
    fn get_timeline_data(&self, app_name: Option<&str>, days: i64)
    -> Result<Value, Box<dyn Error>>;
    fn get_all_timeline(&self) -> Result<Value, Box<dyn Error>>;

    // Checkpoint methods
    fn get_all_checkpoints(&self) -> Result<Value, Box<dyn Error>>;
    fn get_checkpoints_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>>;
    fn create_checkpoint(
        &self,
        name: &str,
        description: Option<&str>,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>>;
    fn set_checkpoint_active(
        &self,
        checkpoint_id: i32,
        is_active: bool,
    ) -> Result<Value, Box<dyn Error>>;
    fn delete_checkpoint(&self, checkpoint_id: i32) -> Result<Value, Box<dyn Error>>;
    fn get_active_checkpoints(&self) -> Result<Value, Box<dyn Error>>;
    fn get_active_checkpoints_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>>;
    fn get_timeline_checkpoint_associations(&self) -> Result<Value, Box<dyn Error>>;

    // Timeline checkpoint methods
    fn create_timeline_checkpoint(
        &self,
        timeline_id: i32,
        checkpoint_id: i32,
    ) -> Result<Value, Box<dyn Error>>;
    fn delete_timeline_checkpoint(
        &self,
        timeline_id: i32,
        checkpoint_id: i32,
    ) -> Result<Value, Box<dyn Error>>;
    fn get_timeline_checkpoints_for_timeline(
        &self,
        timeline_id: i32,
    ) -> Result<Value, Box<dyn Error>>;

    // Checkpoint duration methods
    fn get_checkpoint_durations(&self) -> Result<Value, Box<dyn Error>>;
    fn get_checkpoint_durations_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>>;
    fn update_checkpoint_duration(
        &self,
        checkpoint_id: i32,
        app_id: i32,
        duration: i32,
        sessions_count: i32,
    ) -> Result<Value, Box<dyn Error>>;

    // Active checkpoint table methods
    fn get_all_active_checkpoints_table(&self) -> Result<Value, Box<dyn Error>>;
    fn get_active_checkpoints_for_app_table(&self, app_id: i32) -> Result<Value, Box<dyn Error>>;
    fn activate_checkpoint(&self, checkpoint_id: i32, app_id: i32)
    -> Result<Value, Box<dyn Error>>;
    fn deactivate_checkpoint(
        &self,
        checkpoint_id: i32,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>>;
    fn is_checkpoint_active(&self, checkpoint_id: i32, app_id: i32)
    -> Result<bool, Box<dyn Error>>;
}
