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
}
