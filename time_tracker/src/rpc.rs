use std::sync::{Arc, RwLock};
use std::thread;

use serde_json::Value;
use serde_json::json;

use jsonrpc_core::{
    IoHandler, Params,
    types::{Error, ErrorCode},
};

use jsonrpc_http_server::ServerBuilder;

use crate::restable::Restable;
use crate::time_tracking::{add_process, delete_process};

pub fn init_rpc<T>(client: T)
where
    T: Restable + Sync + Send + 'static,
{
    let client_arc = Arc::new(RwLock::new(client));
    thread::spawn(move || {
        let mut io = IoHandler::new();

        let add_ref = client_arc.clone();
        io.add_method("add_process", move |params: Params| {
            let error_cb = || {
                error!("params for \"add_process\" are invalid!");
                Err(Error::new(ErrorCode::ParseError))
            };

            match params.parse::<Vec<Value>>() {
                Ok(ref param) if param.len() > 1 => match (param[0].as_str(), param[1].as_str()) {
                    (Some(p), Some(path)) => {
                        if add_process(p, path, &add_ref).is_ok() {
                            Ok(Value::String("process successfully added".to_owned()))
                        } else {
                            error!("Could not add process \"{}\"", p);
                            Err(Error::new(ErrorCode::InternalError))
                        }
                    }
                    _ => error_cb(),
                },
                _ => error_cb(),
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
            let error_cb = || {
                error!("params for \"delete_process\" are invalid!");
                Err(Error::new(ErrorCode::ParseError))
            };

            match params.parse::<Vec<Value>>() {
                Ok(ref param) if !param.is_empty() => match param[0].as_str() {
                    Some(p) => {
                        if delete_process(p, &delete_ref).is_ok() {
                            Ok(Value::String("process successfully deleted".to_owned()))
                        } else {
                            error!("Could not delete process \"{}\"", p);
                            Err(Error::new(ErrorCode::InternalError))
                        }
                    }
                    _ => error_cb(),
                },
                _ => error_cb(),
            }
        });

        match ServerBuilder::new(io).start_http(&"0.0.0.0:3030".parse().unwrap()) {
            Ok(server) => {
                info!("RPC server started.");
                server.wait();
            }
            Err(err) => {
                error!("Could not start RPC server: {}", err);
            }
        }
    });
}
