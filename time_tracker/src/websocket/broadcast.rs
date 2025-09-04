use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::sync::mpsc::{Receiver, Sender};
use log::error;

use crate::structs::{TrackingStatus, WebSocketMessage};
use crate::restable::Restable;
use super::server::ServerState;

pub type ClientId = u64;

#[derive(Debug)]
pub enum BroadcastMessage {
    TrackingStatusUpdate(TrackingStatus),
    ClientConnected(ClientId),
    ClientDisconnected(ClientId),
}

#[derive(Clone)]
pub struct WebSocketClient {
    pub id: ClientId,
    pub sender: Sender<String>,
}

pub struct TrackingStatusBroadcaster;

impl TrackingStatusBroadcaster {
    pub fn handle_broadcasts<T>(
        receiver: Receiver<BroadcastMessage>,
        state: Arc<RwLock<ServerState<T>>>
    ) where
        T: Restable + Sync + Send,
    {
        for message in receiver {
            match message {
                BroadcastMessage::TrackingStatusUpdate(status) => {
                    Self::broadcast_tracking_status(&state, status);
                }
                BroadcastMessage::ClientConnected(client_id) => {
                    log::info!("Client {} connected to broadcast system", client_id);
                }
                BroadcastMessage::ClientDisconnected(client_id) => {
                    log::info!("Client {} disconnected from broadcast system", client_id);
                }
            }
        }
    }

    fn broadcast_tracking_status<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        status: TrackingStatus
    ) where
        T: Restable + Sync + Send,
    {
        let status_json = match serde_json::to_string(&status) {
            Ok(json) => json,
            Err(e) => {
                error!("Failed to serialize tracking status: {}", e);
                return;
            }
        };

        let broadcast_message = match WebSocketMessage::tracking_status_update(&status_json).to_json() {
            Ok(json) => json,
            Err(e) => {
                error!("Failed to serialize broadcast message: {}", e);
                return;
            }
        };

        let clients = {
            let state_guard = match state.read() {
                Ok(guard) => guard,
                Err(e) => {
                    error!("Failed to acquire read lock: {}", e);
                    return;
                }
            };
            let clients_guard = state_guard.clients().lock().unwrap();
            clients_guard.iter().map(|(id, client)| (*id, client.clone())).collect::<HashMap<_, _>>()
        };

        let mut disconnected_clients = Vec::new();
        for (client_id, client) in clients {
            if client.sender.send(broadcast_message.clone()).is_err() {
                disconnected_clients.push(client_id);
            }
        }

        if !disconnected_clients.is_empty() {
            let state_guard = match state.read() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            let mut clients_guard = state_guard.clients().lock().unwrap();
            for client_id in disconnected_clients {
                clients_guard.remove(&client_id);
                log::info!("Removed disconnected client: {}", client_id);
            }
        }
    }
}
