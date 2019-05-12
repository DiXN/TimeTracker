use std::thread;
use std::sync::{Arc, RwLock};

use serde_json::Value;
use serde_json::json;

use jsonrpc_core;

use jsonrpc_core::{
  IoHandler,
  Params,
  types::{Error, ErrorCode}
};

use jsonrpc_http_server::ServerBuilder;

use crate::restable::Restable;
use crate::time_tracking::{delete_process, add_process};

pub fn init_rpc<T>(client: T) where T : Restable + Sync + Send + 'static {
  let client_arc = Arc::new(RwLock::new(client));
  thread::spawn(move || {
    let mut io = IoHandler::new();

    let add_ref = client_arc.clone();
    io.add_method("add_process", move |params: Params| {
      let error_str = "params for \"add_process\" are invalid!";

      match params.parse::<Vec<Value>>() {
        Ok(param) => {
          if param.len() > 1 {
            if let Some(p) = param[0].as_str() {
              if let Some(path) = param[1].as_str() {
                if let Ok(_) = add_process(p, path, &add_ref) {
                  Ok(Value::String("process successfully added".to_owned()))
                } else {
                  error!("Could not add process \"{}\"", p);
                  Err(Error::new(ErrorCode::InternalError))
                }
              } else {
                error!("{}", &error_str);
                Err(Error::new(ErrorCode::ParseError))
              }
            } else {
              error!("{}", &error_str);
              Err(Error::new(ErrorCode::ParseError))
            }
          } else {
            error!("{}", &error_str);
            Err(Error::new(ErrorCode::ParseError))
          }
        },
        Err(_) => {
          error!("{}", &error_str);
          Err(Error::new(ErrorCode::ParseError))
        }
      }
    });

    let get_ref = client_arc.clone();
    io.add_method("get_processes", move |_| {
      let error_str = "Could not get processes!";

      if let Ok(processes) = get_ref.read().unwrap().get_processes() {
        Ok(json!(&processes))
      } else {
        error!("{}", &error_str);
        Err(Error::new(ErrorCode::InternalError))
      }
    });

    let delete_ref = client_arc.clone();
    io.add_method("delete_process", move |params: Params| {
      let error_str = "params for \"delete_process\" are invalid!";

      match params.parse::<Vec<Value>>() {
        Ok(param) => {
          if param.len() > 0 {
            if let Some(p) = param[0].as_str() {
                if let Ok(_) = delete_process(p, &delete_ref) {
                  Ok(Value::String("process successfully deleted".to_owned()))
                } else {
                  error!("Could not delete process \"{}\"", p);
                  Err(Error::new(ErrorCode::InternalError))
                }
            } else {
              error!("{}", &error_str);
              Err(Error::new(ErrorCode::ParseError))
            }
          } else {
            error!("{}", &error_str);
            Err(Error::new(ErrorCode::ParseError))
          }
        },
        Err(_) => {
          error!("{}", &error_str);
          Err(Error::new(ErrorCode::ParseError))
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
