use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use log::{error, info};

use super::broadcast::{BroadcastMessage, ClientId, SubscriptionBroadcaster, SubscriptionTopic, WebSocketClient};
use super::broadcast_logger::log_broadcast_status;
use super::client::ClientConnectionHandler;
use super::tracking_notifier;
use crate::restable::Restable;
use crate::structs::{App, Timeline, TrackingStatus};

#[cfg(feature = "memory")]
use crate::time_tracking::TimeTrackingConfig;

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
        #[cfg(feature = "memory")] config: Arc<RwLock<TimeTrackingConfig>>,
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
        self.clients.lock().unwrap().insert(client_id, client);
        let _ = self
            .broadcast_sender
            .send(BroadcastMessage::ClientConnected(client_id));
    }

    pub fn remove_client(&self, client_id: ClientId) {
        self.clients.lock().unwrap().remove(&client_id);
        let _ = self
            .broadcast_sender
            .send(BroadcastMessage::ClientDisconnected(client_id));
    }

    pub fn broadcast_tracking_status(&self, status: TrackingStatus) {
        *self.tracking_status.write().unwrap() = status.clone();
        let _ = self
            .broadcast_sender
            .send(BroadcastMessage::TrackingStatusUpdate(status));
    }

    pub fn broadcast_apps_update(&self, apps: Vec<App>) {
        let _ = self.broadcast_sender.send(BroadcastMessage::AppsUpdate(apps));
    }

    pub fn broadcast_timeline_update(&self, timeline: Vec<Timeline>) {
        let _ = self
            .broadcast_sender
            .send(BroadcastMessage::TimelineUpdate(timeline));
    }

    pub fn subscribe_client(&self, client_id: ClientId, topics: Vec<SubscriptionTopic>) {
        let _ = self
            .broadcast_sender
            .send(BroadcastMessage::Subscribe { client_id, topics });
    }

    pub fn unsubscribe_client(&self, client_id: ClientId, topics: Vec<SubscriptionTopic>) {
        let _ = self
            .broadcast_sender
            .send(BroadcastMessage::Unsubscribe { client_id, topics });
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
    #[cfg(feature = "memory")] config: Arc<RwLock<TimeTrackingConfig>>,
) -> Arc<RwLock<ServerState<T>>>
where
    T: Restable + Sync + Send + 'static,
{
    let (server_state, broadcast_receiver) =
        ServerState::new(client, #[cfg(feature = "memory")] config);
    let state_arc = Arc::new(RwLock::new(server_state));

    tracking_notifier::register_websocket_notifier(&state_arc);

    let state_for_broadcast = Arc::clone(&state_arc);
    thread::spawn(move || {
        SubscriptionBroadcaster::handle_broadcasts(broadcast_receiver, state_for_broadcast);
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
    let Ok(rt) = tokio::runtime::Runtime::new() else {
        error!("Failed to create async runtime");
        return;
    };

    let Ok(server) = TcpListener::bind("0.0.0.0:6754") else {
        error!("Failed to bind WebSocket server");
        return;
    };

    info!("WebSocket server started on 0.0.0.0:6754");
    log_broadcast_status();

    let mut client_id_counter = 0u64;

    loop {
        match server.accept() {
            Ok((stream, _)) => {
                client_id_counter += 1;
                let client_id = client_id_counter;
                let state_clone = Arc::clone(&state);
                let rt_clone = rt.handle().clone();

                thread::spawn(move || {
                    ClientConnectionHandler::handle_connection(
                        stream,
                        client_id,
                        state_clone,
                        rt_clone,
                    );
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}
