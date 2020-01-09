#[macro_use] extern crate log;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate rust_embed;

use std::error::Error;

use env_logger::{Builder, Env};

mod error;
mod receive_types;
mod time_tracking;

#[cfg(feature = "firebase")]
mod firebase;
#[cfg(feature = "firebase")]
use crate::firebase::FirebaseClient;

mod restable;

mod rpc;
mod hook;

#[cfg(feature = "psql")]
mod sql;
#[cfg(feature = "psql")]
use crate::sql::PgClient;
#[cfg(feature = "psql")]
mod sql_queries;

mod native;
use crate::native::{init_tray, autostart};

#[cfg(windows)]
mod windows;

#[derive(RustEmbed)]
#[folder = "resource/"]
pub struct Asset;

#[cfg(feature = "firebase")]
fn init_client() -> Result<(), Box<dyn Error>> {
  use std::fs;
  use serde_json::Value;

  let config = fs::read_to_string("env.json")?;
  let config : Value = serde_json::from_str(&config)?;
  let firebase_client = FirebaseClient::new(config["url"].as_str().unwrap(), config["key"].as_str().unwrap());
  time_tracking::init(firebase_client).unwrap();

  Ok(())
}

#[cfg(feature = "psql")]
fn init_client() -> Result<(), Box<dyn Error>> {
  let pg_client = PgClient::new("postgres://postgres:root@10.0.0.5:5432/time_tracker");
  time_tracking::init(pg_client).unwrap();

  Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
  let env = Env::default()
    .filter_or(env_logger::DEFAULT_FILTER_ENV, "info");

  Builder::from_env(env).init();

  autostart()?;
  init_tray();
  init_client()?;

  Ok(())
}
