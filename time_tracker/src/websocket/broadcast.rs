use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::sync::mpsc::{Receiver, Sender};
use log::error;

use crate::structs::{TrackingStatus, WebSocketMessage, App, Timeline};
use crate::restable::Restable;
use super::server::ServerState;

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

    pub fn as_str(&self) -> &'static str {
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
        subscribe: bool
    ) where
        T: Restable + Sync + Send,
    {
        let state_guard = match state.read() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Failed to acquire read lock for subscription: {}", e);
                return;
            }
        };

        let clients_guard = match state_guard.clients().lock() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Failed to acquire clients lock for subscription: {}", e);
                return;
            }
        };

        if let Some(client) = clients_guard.get(&client_id) {
            let mut subscriptions = match client.subscriptions.write() {
                Ok(guard) => guard,
                Err(e) => {
                    error!("Failed to acquire subscription lock for client {}: {}", client_id, e);
                    return;
                }
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
        } else {
            error!("Client {} not found for subscription operation", client_id);
        }
    }

    fn broadcast_to_subscribers<T, F>(
        state: &Arc<RwLock<ServerState<T>>>,
        topic: SubscriptionTopic,
        message_creator: F
    ) where
        T: Restable + Sync + Send,
        F: Fn(ClientId) -> Option<String>,
    {
        let clients = {
            let state_guard = match state.read() {
                Ok(guard) => guard,
                Err(e) => {
                    error!("Failed to acquire read lock for broadcast: {}", e);
                    return;
                }
            };
            let clients_guard = state_guard.clients().lock().unwrap();
            clients_guard.iter().map(|(id, client)| (*id, client.clone())).collect::<HashMap<_, _>>()
        };

        let mut disconnected_clients = Vec::new();
        for (client_id, client) in clients {
            let is_subscribed = {
                match client.subscriptions.read() {
                    Ok(subscriptions) => subscriptions.contains(&topic),
                    Err(e) => {
                        error!("Failed to read subscriptions for client {}: {}", client_id, e);
                        continue;
                    }
                }
            };

            if is_subscribed {
                if let Some(message) = message_creator(client_id) {
                    if client.sender.send(message).is_err() {
                        disconnected_clients.push(client_id);
                    }
                }
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

    pub fn create_tracking_status_message(status: &TrackingStatus) -> Option<String> {
        let status_json = match serde_json::to_string(status) {
            Ok(json) => json,
            Err(e) => {
                error!("Failed to serialize tracking status: {}", e);
                return None;
            }
        };

        match WebSocketMessage::tracking_status_update(&status_json).to_json() {
            Ok(json) => Some(json),
            Err(e) => {
                error!("Failed to serialize tracking status broadcast message: {}", e);
                None
            }
        }
    }

    pub fn create_apps_message(apps: &[App]) -> Option<String> {
        let apps_json = match serde_json::to_string(apps) {
            Ok(json) => json,
            Err(e) => {
                error!("Failed to serialize apps: {}", e);
                return None;
            }
        };

        match WebSocketMessage::apps_list(&apps_json).to_json() {
            Ok(json) => Some(json),
            Err(e) => {
                error!("Failed to serialize apps broadcast message: {}", e);
                None
            }
        }
    }

    pub fn create_timeline_message(timeline: &[Timeline]) -> Option<String> {
        let timeline_json = match serde_json::to_string(timeline) {
            Ok(json) => json,
            Err(e) => {
                error!("Failed to serialize timeline: {}", e);
                return None;
            }
        };

        match WebSocketMessage::timeline_data(&timeline_json).to_json() {
            Ok(json) => Some(json),
            Err(e) => {
                error!("Failed to serialize timeline broadcast message: {}", e);
                None
            }
        }
    }
}
