use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, RwLock};

use log::error;

use super::server::ServerState;
use crate::restable::Restable;
use crate::structs::{App, Timeline, TrackingStatus, WebSocketMessage};

pub type ClientId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubscriptionTopic {
    Apps,
    Timeline,
    TrackingStatus,
}

impl SubscriptionTopic {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "apps" => Some(Self::Apps),
            "timeline" => Some(Self::Timeline),
            "tracking_status" => Some(Self::TrackingStatus),
            _ => None,
        }
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Apps => "apps",
            Self::Timeline => "timeline",
            Self::TrackingStatus => "tracking_status",
        }
    }
}

#[derive(Debug)]
pub enum BroadcastMessage {
    TrackingStatusUpdate(TrackingStatus),
    AppsUpdate(Vec<App>),
    TimelineUpdate(Vec<Timeline>),
    ClientConnected(ClientId),
    ClientDisconnected(ClientId),
    Subscribe { client_id: ClientId, topics: Vec<SubscriptionTopic> },
    Unsubscribe { client_id: ClientId, topics: Vec<SubscriptionTopic> },
}

#[derive(Clone)]
pub struct WebSocketClient {
    pub id: ClientId,
    pub sender: Sender<String>,
    pub subscriptions: Arc<RwLock<HashSet<SubscriptionTopic>>>,
}

pub struct SubscriptionBroadcaster;

impl SubscriptionBroadcaster {
    pub fn handle_broadcasts<T>(
        receiver: Receiver<BroadcastMessage>,
        state: Arc<RwLock<ServerState<T>>>
    ) where
        T: Restable + Sync + Send,
    {
        for message in receiver {
            match message {
                BroadcastMessage::TrackingStatusUpdate(status) => {
                    Self::broadcast_to_subscribers(&state, SubscriptionTopic::TrackingStatus, |_| {
                        Self::create_tracking_status_message(&status)
                    });
                }
                BroadcastMessage::AppsUpdate(apps) => {
                    Self::broadcast_to_subscribers(&state, SubscriptionTopic::Apps, |_| {
                        Self::create_apps_message(&apps)
                    });
                }
                BroadcastMessage::TimelineUpdate(timeline) => {
                    Self::broadcast_to_subscribers(&state, SubscriptionTopic::Timeline, |_| {
                        Self::create_timeline_message(&timeline)
                    });
                }
                BroadcastMessage::ClientConnected(client_id) => {
                    log::info!("Client {} connected to broadcast system", client_id);
                }
                BroadcastMessage::ClientDisconnected(client_id) => {
                    log::info!("Client {} disconnected from broadcast system", client_id);
                }
                BroadcastMessage::Subscribe { client_id, topics } => {
                    Self::handle_subscription(&state, client_id, topics, true);
                }
                BroadcastMessage::Unsubscribe { client_id, topics } => {
                    Self::handle_subscription(&state, client_id, topics, false);
                }
            }
        }
    }

    fn handle_subscription<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        client_id: ClientId,
        topics: Vec<SubscriptionTopic>,
        subscribe: bool,
    ) where
        T: Restable + Sync + Send,
    {
        let Ok(state_guard) = state.read() else {
            error!("Failed to acquire read lock for subscription");
            return;
        };

        let Ok(clients_guard) = state_guard.clients().lock() else {
            error!("Failed to acquire clients lock for subscription");
            return;
        };

        let Some(client) = clients_guard.get(&client_id) else {
            error!("Client {} not found for subscription operation", client_id);
            return;
        };

        let Ok(mut subscriptions) = client.subscriptions.write() else {
            error!(
                "Failed to acquire subscription lock for client {}",
                client_id
            );
            return;
        };

        for topic in topics {
            if subscribe {
                subscriptions.insert(topic.clone());
                log::info!("Client {} subscribed to {}", client_id, topic.as_str());
            } else {
                subscriptions.remove(&topic);
                log::info!("Client {} unsubscribed from {}", client_id, topic.as_str());
            }
        }
    }

    fn broadcast_to_subscribers<T, F>(
        state: &Arc<RwLock<ServerState<T>>>,
        topic: SubscriptionTopic,
        message_creator: F,
    ) where
        T: Restable + Sync + Send,
        F: Fn(ClientId) -> Option<String>,
    {
        let clients = {
            let Ok(state_guard) = state.read() else {
                error!("Failed to acquire read lock for broadcast");
                return;
            };
            let clients_guard = state_guard.clients().lock().unwrap();
            clients_guard
                .iter()
                .map(|(id, client)| (*id, client.clone()))
                .collect::<HashMap<_, _>>()
        };

        let mut disconnected_clients = Vec::new();
        for (client_id, client) in clients {
            let is_subscribed = client
                .subscriptions
                .read()
                .map(|subs| subs.contains(&topic))
                .unwrap_or_else(|e| {
                    error!(
                        "Failed to read subscriptions for client {}: {}",
                        client_id, e
                    );
                    false
                });

            if is_subscribed {
                if let Some(message) = message_creator(client_id) {
                    if client.sender.send(message).is_err() {
                        disconnected_clients.push(client_id);
                    }
                }
            }
        }

        if !disconnected_clients.is_empty() {
            let Ok(state_guard) = state.read() else {
                return;
            };
            let mut clients_guard = state_guard.clients().lock().unwrap();
            for client_id in disconnected_clients {
                clients_guard.remove(&client_id);
                log::info!("Removed disconnected client: {}", client_id);
            }
        }
    }

    pub fn create_tracking_status_message(status: &TrackingStatus) -> Option<String> {
        let status_json = serde_json::to_string(status)
            .inspect_err(|e| error!("Failed to serialize tracking status: {}", e))
            .ok()?;

        WebSocketMessage::tracking_status_update(&status_json)
            .to_json()
            .inspect_err(|e| error!("Failed to serialize tracking status broadcast message: {}", e))
            .ok()
    }

    pub fn create_apps_message(apps: &[App]) -> Option<String> {
        let apps_json = serde_json::to_string(apps)
            .inspect_err(|e| error!("Failed to serialize apps: {}", e))
            .ok()?;

        WebSocketMessage::apps_list(&apps_json)
            .to_json()
            .inspect_err(|e| error!("Failed to serialize apps broadcast message: {}", e))
            .ok()
    }

    pub fn create_timeline_message(timeline: &[Timeline]) -> Option<String> {
        let timeline_json = serde_json::to_string(timeline)
            .inspect_err(|e| error!("Failed to serialize timeline: {}", e))
            .ok()?;

        WebSocketMessage::timeline_data(&timeline_json)
            .to_json()
            .inspect_err(|e| error!("Failed to serialize timeline broadcast message: {}", e))
            .ok()
    }
}
