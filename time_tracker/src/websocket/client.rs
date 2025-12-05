use std::collections::HashSet;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use log::{error, info};
use tungstenite::{accept, Message};

use super::broadcast::{ClientId, WebSocketClient};
use super::handlers::MessageHandler;
use super::server::ServerState;
use crate::restable::Restable;
use crate::structs::{TrackingStatus, WebSocketMessage};

pub struct ClientConnectionHandler;

impl ClientConnectionHandler {
    pub fn handle_connection<T>(
        stream: TcpStream,
        client_id: ClientId,
        state: Arc<RwLock<ServerState<T>>>,
        rt: tokio::runtime::Handle,
    ) where
        T: Restable + Sync + Send,
    {
        let mut websocket = match accept(stream) {
            Ok(ws) => ws,
            Err(e) => {
                error!("Failed to accept WebSocket connection: {}", e);
                return;
            }
        };

        info!("New WebSocket connection established: {}", client_id);

        let (client_sender, _) = mpsc::channel::<String>();
        let client = WebSocketClient {
            id: client_id,
            sender: client_sender,
            subscriptions: Arc::new(RwLock::new(HashSet::new())),
        };

        let Ok(state_guard) = state.read() else {
            error!("Failed to acquire read lock");
            return;
        };

        state_guard.add_client(client);

        let current_status = state_guard.get_current_tracking_status();
        if Self::send_initial_status(&mut websocket, &current_status).is_err() {
            state_guard.remove_client(client_id);
            return;
        }
        drop(state_guard);

        let (outgoing_sender, outgoing_receiver) = mpsc::channel::<String>();

        if let Ok(state_guard) = state.read()
            && let Ok(mut clients_guard) = state_guard.clients().lock()
            && let Some(client) = clients_guard.get_mut(&client_id)
        {
            client.sender = outgoing_sender;
        }

        Self::handle_client_messages(websocket, client_id, state, rt, outgoing_receiver);
    }

    fn send_initial_status(
        websocket: &mut tungstenite::WebSocket<TcpStream>,
        status: &TrackingStatus,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let status_json = serde_json::to_string(status)?;
        let initial_message = WebSocketMessage::tracking_status_update(&status_json).to_json()?;
        websocket.send(Message::text(initial_message))?;
        Ok(())
    }

    fn handle_client_messages<T>(
        mut websocket: tungstenite::WebSocket<TcpStream>,
        client_id: ClientId,
        state: Arc<RwLock<ServerState<T>>>,
        rt: tokio::runtime::Handle,
        outgoing_receiver: mpsc::Receiver<String>,
    ) where
        T: Restable + Sync + Send,
    {
        loop {
            if let Ok(outgoing_message) = outgoing_receiver.try_recv()
                && websocket.send(Message::text(outgoing_message)).is_err()
            {
                break;
            }

            match websocket.read() {
                Ok(msg) if msg.is_close() => {
                    info!("WebSocket connection closed for client: {}", client_id);
                    break;
                }
                Ok(msg) if !msg.is_text() && !msg.is_binary() => continue,
                Ok(msg) => {
                    let response = rt.block_on(async {
                        MessageHandler::handle_message_with_client_id(&msg, &state, Some(client_id))
                            .await
                    });

                    let response_text = match response {
                        Ok(text) => text,
                        Err(e) => {
                            let Ok(error_json) =
                                WebSocketMessage::error(&e.to_string()).to_json()
                            else {
                                continue;
                            };
                            error_json
                        }
                    };

                    if websocket.send(Message::text(response_text)).is_err() {
                        break;
                    }
                }
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    error!("WebSocket error for client {}: {}", client_id, e);
                    break;
                }
            }
        }

        if let Ok(state_guard) = state.read() {
            state_guard.remove_client(client_id);
        }
        info!("Client {} disconnected", client_id);
    }
}
