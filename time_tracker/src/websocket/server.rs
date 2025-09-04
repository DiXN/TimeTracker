use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, RwLock, Mutex};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crate::restable::Restable;
use crate::structs::TrackingStatus;
use log::{error, info};

#[cfg(feature = "memory")]
use crate::time_tracking::TimeTrackingConfig;

use super::broadcast::{BroadcastMessage, WebSocketClient, ClientId, TrackingStatusBroadcaster};
use super::client::ClientConnectionHandler;
use super::tracking_notifier;

pub struct ServerState<T: Restable> {
    pub client: Arc<RwLock<T>>,
    tracking_status: Arc<RwLock<TrackingStatus>>,
    clients: Arc<Mutex<HashMap<ClientId, WebSocketClient>>>,
    broadcast_sender: Sender<BroadcastMessage>,
    #[cfg(feature = "memory")]
    config: Arc<RwLock<TimeTrackingConfig>>,
}

impl<T: Restable> ServerState<T> {
    pub fn new(
        client: T,
        #[cfg(feature = "memory")] config: Arc<RwLock<TimeTrackingConfig>>
    ) -> (Self, Receiver<BroadcastMessage>) {
        let (broadcast_sender, broadcast_receiver) = mpsc::channel();
        let state = Self {
            client: Arc::new(RwLock::new(client)),
            tracking_status: Arc::new(RwLock::new(TrackingStatus {
                is_tracking: false,
                is_paused: false,
                current_app: None,
                current_session_duration: 0,
                session_start_time: None,
                active_checkpoint_ids: vec![],
            })),
            clients: Arc::new(Mutex::new(HashMap::new())),
            broadcast_sender,
            #[cfg(feature = "memory")]
            config,
        };
        (state, broadcast_receiver)
    }

    pub fn add_client(&self, client: WebSocketClient) {
        let client_id = client.id;
        let mut clients = self.clients.lock().unwrap();
        clients.insert(client_id, client);
        let _ = self.broadcast_sender.send(BroadcastMessage::ClientConnected(client_id));
    }

    pub fn remove_client(&self, client_id: ClientId) {
        let mut clients = self.clients.lock().unwrap();
        clients.remove(&client_id);
        let _ = self.broadcast_sender.send(BroadcastMessage::ClientDisconnected(client_id));
    }

    pub fn broadcast_tracking_status(&self, status: TrackingStatus) {
        {
            let mut tracking_status = self.tracking_status.write().unwrap();
            *tracking_status = status.clone();
        }
        let _ = self.broadcast_sender.send(BroadcastMessage::TrackingStatusUpdate(status));
    }

    pub fn get_current_tracking_status(&self) -> TrackingStatus {
        self.tracking_status.read().unwrap().clone()
    }

    pub fn clients(&self) -> &Arc<Mutex<HashMap<ClientId, WebSocketClient>>> {
        &self.clients
    }

    #[cfg(feature = "memory")]
    pub fn config(&self) -> &Arc<RwLock<TimeTrackingConfig>> {
        &self.config
    }
}

pub fn init_web_socket<T>(
    client: T,
    #[cfg(feature = "memory")] config: Arc<RwLock<TimeTrackingConfig>>
) -> Arc<RwLock<ServerState<T>>>
where
    T: Restable + Sync + Send + 'static,
{
    let (server_state, broadcast_receiver) = ServerState::new(client, #[cfg(feature = "memory")] config);
    let state_arc = Arc::new(RwLock::new(server_state));

    // Register the WebSocket notifier for on-demand broadcasting
    tracking_notifier::register_websocket_notifier(&state_arc);

    let state_for_broadcast = Arc::clone(&state_arc);
    thread::spawn(move || {
        TrackingStatusBroadcaster::handle_broadcasts(broadcast_receiver, state_for_broadcast);
    });

    let state_for_server = Arc::clone(&state_arc);
    thread::spawn(move || {
        start_websocket_server(state_for_server);
    });

    state_arc
}

fn start_websocket_server<T>(state: Arc<RwLock<ServerState<T>>>)
where
    T: Restable + Sync + Send + 'static,
{
    let rt = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            error!("Failed to create async runtime: {}", e);
            return;
        }
    };

    let server = match TcpListener::bind("127.0.0.1:6754") {
        Ok(server) => server,
        Err(e) => {
            error!("Failed to bind WebSocket server: {}", e);
            return;
        }
    };
    info!("WebSocket server started on 127.0.0.1:6754");

    let mut client_id_counter = 0u64;

    loop {
        match server.accept() {
            Ok((stream, _)) => {
                client_id_counter += 1;
                let client_id = client_id_counter;
                let state_clone = Arc::clone(&state);
                let rt_clone = rt.handle().clone();

                thread::spawn(move || {
                    ClientConnectionHandler::handle_connection(stream, client_id, state_clone, rt_clone);
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}
