use std::error::Error;
use serde_json::Value;

use crossbeam_channel::Receiver;

use crate::receive_types::ReceiveTypes;

pub trait Restable {
  fn get_data(&self, item: &str) -> Result<Value, Box<dyn Error>>;
  fn get_processes(&self) -> Result<Vec<String>, Box<dyn Error>>;
  fn put_data(&self, item: &str, product_name: &str) -> Result<Value, Box<dyn Error>>;
  fn delete_data(&self, item: &str) -> Result<Value, Box<dyn Error>>;
  fn patch_data(&self, item: &str, value: &Value) -> Result<Value, Box<dyn Error>>;
  fn init_event_loop(self, recv: Receiver<(String, ReceiveTypes)>);
}
