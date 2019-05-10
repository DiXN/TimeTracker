use std::thread;

use serde_json::Value;

use jsonrpc_core;
use jsonrpc_core::{IoHandler, Params};
use jsonrpc_http_server::{ServerBuilder};

use crate::time_tracking::add_process;

pub fn init_rpc() {
  thread::spawn(|| {
    let mut io = IoHandler::new();

    io.add_method("add_process", |params: Params| {
      let error_str = "params for \"add_process\" are invalid!";

      match params.parse::<Vec<Value>>() {
        Ok(param) => {
          if let Some(p) = param[0].as_str() {
            add_process(p);
            Ok(Value::String("process successfully added".to_owned()))
          } else {
            error!("{}", &error_str);
            Ok(Value::String(error_str.to_string()))
          }
        },
        Err(_) => {
          error!("{}", &error_str);
          Ok(Value::String(error_str.to_string()))
        }
      }
    });

    if let Ok(server) = ServerBuilder::new(io)
      .start_http(&"127.0.0.1:3030".parse().unwrap()) {
      info!("RPC server started.");
      server.wait();
    } else {
      error!("Could not start RPC server");
    }
  });
}
