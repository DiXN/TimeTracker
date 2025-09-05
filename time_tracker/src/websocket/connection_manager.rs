use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;
use tracing::{debug, info};

use crate::restable::Restable;
use crate::structs::{App, Timeline, TrackingStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedData<T> {
    pub data: T,
    pub timestamp: u64,
    pub version: u32,
}

impl<T> CachedData<T> {
    pub fn new(data: T, version: u32) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        Self { data, timestamp, version }
    }

    pub fn is_expired(&self, ttl_seconds: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        now - self.timestamp > ttl_seconds
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamingMessage {
    DataUpdate {
        data_type: String,
        version: u32,
        data: serde_json::Value,
        is_delta: bool,
    },
    Heartbeat {
        timestamp: u64,
        active_subscriptions: Vec<String>,
    },
    Error {
        message: String,
    },
}

pub struct ConnectionManager<T: Restable> {
    db_client: Arc<RwLock<T>>,
    apps_cache: Arc<RwLock<Option<CachedData<Vec<App>>>>>,
    timeline_cache: Arc<RwLock<Option<CachedData<Vec<Timeline>>>>>,
    tracking_status_cache: Arc<RwLock<Option<CachedData<TrackingStatus>>>>,
    data_versions: Arc<RwLock<HashMap<String, u32>>>,
    broadcast_tx: broadcast::Sender<StreamingMessage>,
    cache_ttl: Duration,
}

impl<T: Restable + Clone + Send + Sync + 'static> ConnectionManager<T> {
    pub fn new(db_client: T) -> Self {
        let (broadcast_tx, _) = broadcast::channel(1024);
        
        Self {
            db_client: Arc::new(RwLock::new(db_client)),
            apps_cache: Arc::new(RwLock::new(None)),
            timeline_cache: Arc::new(RwLock::new(None)),
            tracking_status_cache: Arc::new(RwLock::new(None)),
            data_versions: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
            cache_ttl: Duration::from_secs(30),
        }
    }

    pub async fn start_background_tasks(&self) {
        self.start_cache_refresh_task().await;
        self.start_heartbeat_task().await;
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StreamingMessage> {
        self.broadcast_tx.subscribe()
    }

    pub async fn get_apps_with_cache(&self, force_refresh: bool) -> Result<Vec<App>> {
        if !force_refresh {
            if let Some(cached) = self.apps_cache.read().await.as_ref() {
                if !cached.is_expired(self.cache_ttl.as_secs()) {
                    debug!("Returning cached apps data");
                    return Ok(cached.data.clone());
                }
            }
        }

        info!("Fetching fresh apps data");
        let fresh_data = self.fetch_apps_from_db().await?;
        let new_version = self.increment_version("apps").await;
        
        *self.apps_cache.write().await = Some(CachedData::new(fresh_data.clone(), new_version));
        
        let _ = self.broadcast_tx.send(StreamingMessage::DataUpdate {
            data_type: "apps".to_string(),
            version: new_version,
            data: serde_json::to_value(&fresh_data)?,
            is_delta: false,
        });
        
        Ok(fresh_data)
    }

    pub async fn get_timeline_with_cache(&self, force_refresh: bool) -> Result<Vec<Timeline>> {
        if !force_refresh {
            if let Some(cached) = self.timeline_cache.read().await.as_ref() {
                if !cached.is_expired(self.cache_ttl.as_secs()) {
                    debug!("Returning cached timeline data");
                    return Ok(cached.data.clone());
                }
            }
        }

        info!("Fetching fresh timeline data");
        let fresh_data = self.fetch_timeline_from_db().await?;
        let new_version = self.increment_version("timeline").await;
        
        *self.timeline_cache.write().await = Some(CachedData::new(fresh_data.clone(), new_version));
        
        let _ = self.broadcast_tx.send(StreamingMessage::DataUpdate {
            data_type: "timeline".to_string(),
            version: new_version,
            data: serde_json::to_value(&fresh_data)?,
            is_delta: false,
        });
        
        Ok(fresh_data)
    }

    pub async fn update_tracking_status(&self, new_status: TrackingStatus) -> Result<()> {
        // Update the tracking service
        if let Some(service) = super::tracking_service::get_global_tracking_service() {
            service.update_from_legacy(new_status.clone())?;
        }
        
        let new_version = self.increment_version("tracking_status").await;
        *self.tracking_status_cache.write().await = Some(CachedData::new(new_status.clone(), new_version));
        
        let _ = self.broadcast_tx.send(StreamingMessage::DataUpdate {
            data_type: "tracking_status".to_string(),
            version: new_version,
            data: serde_json::to_value(&new_status)?,
            is_delta: false,
        });
        
        Ok(())
    }

    pub async fn get_tracking_status_with_cache(&self, force_refresh: bool) -> Result<TrackingStatus> {
        if !force_refresh {
            if let Some(cached) = self.tracking_status_cache.read().await.as_ref() {
                if !cached.is_expired(self.cache_ttl.as_secs()) {
                    debug!("Returning cached tracking status");
                    return Ok(cached.data.clone());
                }
            }
        }

        // Default tracking status
        let default_status = TrackingStatus {
            is_tracking: false,
            is_paused: false,
            current_app: None,
            current_session_duration: 0,
            session_start_time: None,
            active_checkpoint_ids: vec![],
        };
        
        let new_version = self.increment_version("tracking_status").await;
        *self.tracking_status_cache.write().await = Some(CachedData::new(default_status.clone(), new_version));
        
        Ok(default_status)
    }

    async fn increment_version(&self, data_type: &str) -> u32 {
        let mut versions = self.data_versions.write().await;
        let version = versions.get(data_type).unwrap_or(&0) + 1;
        versions.insert(data_type.to_string(), version);
        version
    }

    async fn start_cache_refresh_task(&self) {
        let manager = self.clone_for_task();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let _ = manager.refresh_stale_caches().await;
            }
        });
    }

    async fn start_heartbeat_task(&self) {
        let broadcast_tx = self.broadcast_tx.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let heartbeat = StreamingMessage::Heartbeat {
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    active_subscriptions: vec!["apps".to_string(), "timeline".to_string(), "tracking_status".to_string()],
                };
                let _ = broadcast_tx.send(heartbeat);
            }
        });
    }

    fn clone_for_task(&self) -> ConnectionManagerTask<T> {
        ConnectionManagerTask {
            db_client: Arc::clone(&self.db_client),
            apps_cache: Arc::clone(&self.apps_cache),
            timeline_cache: Arc::clone(&self.timeline_cache),
            tracking_status_cache: Arc::clone(&self.tracking_status_cache),
        }
    }

    async fn fetch_apps_from_db(&self) -> Result<Vec<App>> {
        let db_guard = self.db_client.read().await;
        let apps_json = db_guard.get_all_apps().await.map_err(|e| anyhow::anyhow!("Database error: {}", e))?;
        let apps: Vec<App> = serde_json::from_value(apps_json)?;
        Ok(apps)
    }

    async fn fetch_timeline_from_db(&self) -> Result<Vec<Timeline>> {
        let db_guard = self.db_client.read().await;
        let timeline_json = db_guard.get_timeline_data(None, 30).await.map_err(|e| anyhow::anyhow!("Database error: {}", e))?;
        let timeline: Vec<Timeline> = serde_json::from_value(timeline_json)?;
        Ok(timeline)
    }
}

#[derive(Clone)]
struct ConnectionManagerTask<T: Restable> {
    db_client: Arc<RwLock<T>>,
    apps_cache: Arc<RwLock<Option<CachedData<Vec<App>>>>>,
    timeline_cache: Arc<RwLock<Option<CachedData<Vec<Timeline>>>>>,
    tracking_status_cache: Arc<RwLock<Option<CachedData<TrackingStatus>>>>,
}

impl<T: Restable + Clone + Send + Sync + 'static> ConnectionManagerTask<T> {
    async fn refresh_stale_caches(&self) -> Result<()> {
        // Check and refresh stale caches
        let ttl = 30; // seconds

        if let Some(cached) = self.apps_cache.read().await.as_ref() {
            if cached.is_expired(ttl) {
                info!("Refreshing stale apps cache");
                // Would trigger refresh here
            }
        }

        if let Some(cached) = self.timeline_cache.read().await.as_ref() {
            if cached.is_expired(ttl) {
                info!("Refreshing stale timeline cache");
                // Would trigger refresh here
            }
        }

        Ok(())
    }
}
