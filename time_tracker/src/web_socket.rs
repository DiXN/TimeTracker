use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use crate::restable::Restable;
use crate::structs::{App, Timeline, WebSocketCommand, WebSocketMessage};
use crate::time_tracking::{add_process, delete_process};
use tungstenite::{Message, accept};

pub fn init_web_socket<T>(client: T)
where
    T: Restable + Sync + Send + 'static,
{
    let client_arc = Arc::new(RwLock::new(client));

    thread::spawn(move || {
        let server = TcpListener::bind("127.0.0.1:6754").unwrap();
        info!("WebSocket server started on 127.0.0.1:6754");

        loop {
            match server.accept() {
                Ok((stream, _)) => {
                    let client_clone = Arc::clone(&client_arc);
                    thread::spawn(move || {
                        let mut websocket = accept(stream).unwrap();
                        info!("New WebSocket connection established");

                        loop {
                            let msg = match websocket.read() {
                                Ok(msg) => msg,
                                Err(e) => {
                                    error!("WebSocket error: {}", e);
                                    break;
                                }
                            };

                            if msg.is_close() {
                                info!("WebSocket connection closed");
                                break;
                            }

                            if !msg.is_text() && !msg.is_binary() {
                                // Ping/pong messages are handled automatically
                                continue;
                            }

                            let response = handle_message(&msg, &client_clone);

                            let response_text = match response {
                                Ok(text) => text,
                                Err(e) => {
                                    let error_msg = WebSocketMessage::error(&format!(
                                        "Error processing request: {}",
                                        e
                                    ));
                                    match error_msg.to_json() {
                                        Ok(error_json) => error_json,
                                        Err(_) => continue,
                                    }
                                }
                            };

                            if let Err(e) = websocket.send(Message::text(response_text)) {
                                error!("Failed to send WebSocket response: {}", e);
                                break;
                            }
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    });
}

fn handle_message<T>(
    msg: &Message,
    client: &Arc<RwLock<T>>,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let text = msg.to_text()?;
    let command = WebSocketCommand::from_json(text)?;

    match command {
        WebSocketCommand::GetApps(_) => handle_get_apps(client),
        WebSocketCommand::GetTimeline(payload) => handle_get_timeline(client, &payload),
        WebSocketCommand::GetAppByName(payload) => handle_get_app_by_name(client, &payload),
        WebSocketCommand::AddProcess(payload) => handle_add_process(client, &payload),
        WebSocketCommand::DeleteProcess(payload) => handle_delete_process(client, &payload),
        _ => {
            let error_msg = WebSocketMessage::error("Unknown or unimplemented command");
            Ok(error_msg.to_json()?)
        }
    }
}

fn handle_get_apps<T>(client: &Arc<RwLock<T>>) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let apps_data = client_guard.get_all_apps()?;
    let mut apps = Vec::new();

    if let Some(rows) = apps_data.as_array() {
        for row in rows {
            if let Some(obj) = row.as_object() {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_owned()))
                    .collect();

                if let Some(app) = App::from_pg_row(&row_map) {
                    apps.push(app);
                }
            }
        }
    }

    let apps_json = serde_json::to_string(&apps)?;
    let response = WebSocketMessage::apps_list(&apps_json);

    Ok(response.to_json()?)
}

fn handle_get_timeline<T>(
    client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;

    let (app_name_owned, days) = if payload.is_empty() {
        (None, 30i64)
    } else {
        let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
        let app_name = params
            .get("app_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let days = params.get("days").and_then(|v| v.as_i64()).unwrap_or(30);
        (app_name, days)
    };

    let timeline_data = client_guard.get_timeline_data(app_name_owned.as_deref(), days)?;
    let mut timeline = Vec::new();

    if let Some(rows) = timeline_data.as_array() {
        for row in rows {
            if let Some(obj) = row.as_object() {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_owned()))
                    .collect();

                if let Some(timeline_entry) = Timeline::from_pg_row(&row_map) {
                    timeline.push(timeline_entry);
                }
            }
        }
    }

    let timeline_json = serde_json::to_string(&timeline)?;
    let response = WebSocketMessage::timeline_data(&timeline_json);

    Ok(response.to_json()?)
}

fn handle_get_app_by_name<T>(
    client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let client_guard = client
        .read()
        .map_err(|e| format!("Failed to acquire read lock: {}", e))?;
    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let app_name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Missing name parameter")?;

    let apps_data = client_guard.get_all_apps()?;

    if let Some(rows) = apps_data.as_array() {
        for row in rows {
            if let Some(obj) = row.as_object() {
                let row_map: HashMap<String, String> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_owned()))
                    .collect();

                if let Some(app) = App::from_pg_row(&row_map) {
                    let matches = app
                        .name
                        .as_ref()
                        .map(|name| name == app_name)
                        .unwrap_or(false);

                    if matches {
                        let app_json = serde_json::to_string(&app)?;
                        let response = WebSocketMessage::app(&app_json);
                        return Ok(response.to_json()?);
                    }
                }
            }
        }
    }

    let error_msg = WebSocketMessage::error("App not found");
    Ok(error_msg.to_json()?)
}

fn handle_add_process<T>(
    client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let process_name = params
        .get("process_name")
        .and_then(|v| v.as_str())
        .ok_or("Missing process_name parameter")?;
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing path parameter")?;

    match add_process(process_name, path, client) {
        Ok(_) => {
            let response =
                WebSocketMessage::success(&format!("Successfully added process: {}", process_name));
            Ok(response.to_json()?)
        }
        Err(e) => {
            let error_msg =
                WebSocketMessage::error(&format!("Failed to add process {}: {}", process_name, e));
            Ok(error_msg.to_json()?)
        }
    }
}

fn handle_delete_process<T>(
    client: &Arc<RwLock<T>>,
    payload: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: Restable + Sync + Send,
{
    let params: HashMap<String, serde_json::Value> = serde_json::from_str(payload)?;
    let process_name = params
        .get("process_name")
        .and_then(|v| v.as_str())
        .ok_or("Missing process_name parameter")?;

    match delete_process(process_name, client) {
        Ok(_) => {
            let response = WebSocketMessage::success(&format!(
                "Successfully deleted process: {}",
                process_name
            ));
            Ok(response.to_json()?)
        }
        Err(e) => {
            let error_msg = WebSocketMessage::error(&format!(
                "Failed to delete process {}: {}",
                process_name, e
            ));
            Ok(error_msg.to_json()?)
        }
    }
}
