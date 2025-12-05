use std::collections::{HashMap, HashSet};
use std::str::FromStr;
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

impl FromStr for SubscriptionTopic {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "apps" => Ok(Self::Apps),
            "timeline" => Ok(Self::Timeline),
            "tracking_status" => Ok(Self::TrackingStatus),
            _ => Err(()),
        }
    }
}

impl SubscriptionTopic {
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
    Subscribe {
        client_id: ClientId,
        topics: Vec<SubscriptionTopic>,
    },
    Unsubscribe {
        client_id: ClientId,
        topics: Vec<SubscriptionTopic>,
    },
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
        state: Arc<RwLock<ServerState<T>>>,
    ) where
        T: Restable + Sync + Send,
    {
        for message in receiver {
            match message {
                BroadcastMessage::TrackingStatusUpdate(status) => {
                    let msg = Self::create_tracking_status_message(&status);
                    Self::broadcast_to_subscribers(&state, SubscriptionTopic::TrackingStatus, msg);
                }
                BroadcastMessage::AppsUpdate(apps) => {
                    let msg = Self::create_apps_message(&apps);
                    Self::broadcast_to_subscribers(&state, SubscriptionTopic::Apps, msg);
                }
                BroadcastMessage::TimelineUpdate(timeline) => {
                    let msg = Self::create_timeline_message(&timeline);
                    Self::broadcast_to_subscribers(&state, SubscriptionTopic::Timeline, msg);
                }
                BroadcastMessage::ClientConnected(client_id) => {
                    log::info!("Client {} connected to broadcast system", client_id);
                }
                BroadcastMessage::ClientDisconnected(client_id) => {
                    log::info!("Client {} disconnected from broadcast system", client_id);
                }
                BroadcastMessage::Subscribe { client_id, topics } => {
                    Self::handle_subscription(&state, client_id, &topics, true);
                }
                BroadcastMessage::Unsubscribe { client_id, topics } => {
                    Self::handle_subscription(&state, client_id, &topics, false);
                }
            }
        }
    }

    fn handle_subscription<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        client_id: ClientId,
        topics: &[SubscriptionTopic],
        subscribe: bool,
    ) where
        T: Restable + Sync + Send,
    {
        let Ok(state_guard) = state.read() else {
            error!("Failed to acquire read lock for subscription");
            return;
        };

        let subscriptions = {
            let Ok(clients_guard) = state_guard.clients().lock() else {
                error!("Failed to acquire clients lock for subscription");
                return;
            };

            let Some(client) = clients_guard.get(&client_id) else {
                error!("Client {} not found for subscription operation", client_id);
                return;
            };

            Arc::clone(&client.subscriptions)
        };

        let Ok(mut subs) = subscriptions.write() else {
            error!("Failed to acquire subscription lock for client {}", client_id);
            return;
        };

        for topic in topics {
            if subscribe {
                subs.insert(topic.clone());
                log::info!("Client {} subscribed to {}", client_id, topic.as_str());
            } else {
                subs.remove(topic);
                log::info!("Client {} unsubscribed from {}", client_id, topic.as_str());
            }
        }
    }

    fn broadcast_to_subscribers<T>(
        state: &Arc<RwLock<ServerState<T>>>,
        topic: SubscriptionTopic,
        message: Option<String>,
    ) where
        T: Restable + Sync + Send,
    {
        let Some(message) = message else {
            return;
        };

        let clients = {
            let Ok(state_guard) = state.read() else {
                error!("Failed to acquire read lock for broadcast");
                return;
            };
            let Ok(clients_guard) = state_guard.clients().lock() else {
                return;
            };
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
                .unwrap_or(false);

            if is_subscribed && client.sender.send(message.clone()).is_err() {
                disconnected_clients.push(client_id);
            }
        }

        if !disconnected_clients.is_empty() {
            let Ok(state_guard) = state.read() else {
                return;
            };
            let Ok(mut clients_guard) = state_guard.clients().lock() else {
                return;
            };
            for client_id in disconnected_clients {
                clients_guard.remove(&client_id);
                log::info!("Removed disconnected client: {}", client_id);
            }
        }
    }

    pub fn create_tracking_status_message(status: &TrackingStatus) -> Option<String> {
        serde_json::to_string(status)
            .inspect_err(|e| error!("Failed to serialize tracking status: {}", e))
            .ok()
            .and_then(|json| {
                WebSocketMessage::tracking_status_update(&json)
                    .to_json()
                    .inspect_err(|e| {
                        error!("Failed to serialize tracking status broadcast message: {}", e)
                    })
                    .ok()
            })
    }

    pub fn create_apps_message(apps: &[App]) -> Option<String> {
        serde_json::to_string(apps)
            .inspect_err(|e| error!("Failed to serialize apps: {}", e))
            .ok()
            .and_then(|json| {
                WebSocketMessage::apps_list(&json)
                    .to_json()
                    .inspect_err(|e| error!("Failed to serialize apps broadcast message: {}", e))
                    .ok()
            })
    }

    pub fn create_timeline_message(timeline: &[Timeline]) -> Option<String> {
        serde_json::to_string(timeline)
            .inspect_err(|e| error!("Failed to serialize timeline: {}", e))
            .ok()
            .and_then(|json| {
                WebSocketMessage::timeline_data(&json)
                    .to_json()
                    .inspect_err(|e| error!("Failed to serialize timeline broadcast message: {}", e))
                    .ok()
            })
    }
}
