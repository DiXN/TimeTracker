use std::{
  fs,
  error::Error
};

use serde_json::Value;

mod receive_types;
mod time_tracking;

mod firebase;
use crate::firebase::FirebaseClient;

mod restable;

#[macro_use]
mod native;

fn main() -> Result<(), Box<dyn Error>> {
  let config = fs::read_to_string("env.json")?;
  let config : Value = serde_json::from_str(&config)?;
  let firebase_client = FirebaseClient::new(config["url"].as_str().unwrap(), config["key"].as_str().unwrap());
  time_tracking::init(firebase_client).unwrap();
  Ok(())
}
