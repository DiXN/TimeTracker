#[macro_use] extern crate log;
#[macro_use] extern crate lazy_static;

use std::{
  fs,
  error::Error
};

use libc::c_char;

use env_logger::{Builder, Env};
use serde_json::Value;

mod receive_types;
mod time_tracking;

mod firebase;
use crate::firebase::FirebaseClient;

mod restable;

mod rpc;

extern {
  pub fn query_file_info(process: *const c_char) -> *const c_char;
  pub fn is_process_running(process: *const c_char) -> bool;
}

#[macro_export(local_inner_macros)]
macro_rules! n_str {
  ($n_str:expr) => (std::ffi::CString::new($n_str).expect("could not create CString"));
}

#[macro_export(local_inner_macros)]
macro_rules! ns_invoke {
  ($func:expr, $($param:expr)*) => {{
    let ret = unsafe { $func($($param.as_ptr()), *) };

    if !ret.is_null() {
      let n_str = unsafe { std::ffi::CStr::from_ptr(ret) };
      let mut n_str = n_str.to_string_lossy().into_owned();
      n_str.retain(|c| c != '\u{FFFD}');
      n_str
    } else {
      String::from("")
    }
  }};
}

fn main() -> Result<(), Box<dyn Error>> {
  let env = Env::default()
    .filter_or(env_logger::DEFAULT_FILTER_ENV, "info");

  Builder::from_env(env).init();

  let config = fs::read_to_string("env.json")?;
  let config : Value = serde_json::from_str(&config)?;
  let firebase_client = FirebaseClient::new(config["url"].as_str().unwrap(), config["key"].as_str().unwrap());
  time_tracking::init(firebase_client).unwrap();

  Ok(())
}
