use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex, RwLock},
    thread,
    time::Duration,
};

use chrono;
use crossbeam_channel::{Sender, unbounded};
use lazy_static::lazy_static;

use log::info;

use crate::hook::init_hook;
use crate::native::{are_processes_running, ver_query_value};
use crate::receive_types::ReceiveTypes;
use crate::restable::Restable;
use crate::rpc::init_rpc;
use crate::structs::TrackingStatus;
use crate::websocket::{
    broadcast_apps_update, broadcast_tracking_status_update, has_active_broadcaster,
    has_active_notifier, init_web_socket,
};
use crate::{box_err, error::AddError};

#[derive(Clone, serde::Deserialize)]
#[serde(crate = "serde")]
pub struct TimeTrackingConfig {
    pub tracking_delay_ms: u64,
    pub check_delay_ms: u64,
}

impl Default for TimeTrackingConfig {
    fn default() -> Self {
        TimeTrackingConfig {
            tracking_delay_ms: 60000, // 60 seconds
            check_delay_ms: 10000,    // 10 seconds
        }
    }
}

lazy_static! {
    static ref PROCESS_MAP: Mutex<HashMap<String, (bool, bool)>> = Mutex::new(HashMap::new());
    static ref PAUSE: RwLock<bool> = RwLock::new(false);
}

macro_rules! active {
  { $($b:tt)* } => {{
    if !(*PAUSE.read().unwrap()) {
      $($b)*
    }
  }}
}

pub async fn init<T>(client: T, config: TimeTrackingConfig) -> Result<(), Box<dyn Error>>
where
    T: Restable + Clone + Sync + Send + 'static,
{
    client.setup().await?;

    for p in client.get_processes().await? {
        PROCESS_MAP.lock().unwrap().insert(p, (false, false));
    }

    init_hook(client.clone());

    let (tx, rx) = unbounded();
    let tx_arc = Arc::new(tx);

    let (spawn_tx, spawn_rx) = unbounded();

    let shared_config = Arc::new(RwLock::new(config));

    init_rpc(client.clone());

    init_web_socket(
        client.clone(),
        #[cfg(feature = "memory")]
        Arc::clone(&shared_config),
    );

    let client_clone = client.clone();
    client.init_event_loop(rx);
    check_processes(spawn_tx, Arc::clone(&shared_config), client_clone);

    while let Ok(p) = spawn_rx.recv() {
        let tx_arc_clone = tx_arc.clone();
        let config_clone = Arc::clone(&shared_config);

        thread::spawn(move || {
            #[cfg(not(feature = "memory"))]
            let tracking_delay_ms = config_clone.read().unwrap().tracking_delay_ms;

            active! { tx_arc_clone.send((p.to_owned(), ReceiveTypes::Launches)).unwrap(); };

            if has_active_broadcaster() {
                let start_time = chrono::Local::now()
                    .format("%Y-%m-%dT%H:%M:%S%.f")
                    .to_string();
                broadcast_tracking_status_update(TrackingStatus {
                    is_tracking: true,
                    is_paused: false,
                    current_app: Some(p.clone()),
                    current_session_duration: 0,
                    session_start_time: Some(start_time),
                    active_checkpoint_ids: vec![],
                });
            }

            let mut counter = 0;

            info!("Process: {} has started. at {}", p, chrono::Local::now());

            loop {
                #[cfg(feature = "memory")]
                let tracking_delay_ms = {
                    let config = config_clone.read().unwrap();
                    config.tracking_delay_ms
                };

                thread::sleep(Duration::from_millis(tracking_delay_ms));

                active! {
                  if let Some((fst, snd)) = PROCESS_MAP.lock().unwrap().get_mut(&p) {
                    if *fst {
                      tx_arc_clone.send((p.to_owned(), ReceiveTypes::Duration)).unwrap();
                      tx_arc_clone.send((p.to_owned(), ReceiveTypes::Timeline)).unwrap();
                      counter += 1;

                      if has_active_broadcaster() {
                          broadcast_tracking_status_update(TrackingStatus {
                              is_tracking: true,
                              is_paused: false,
                              current_app: Some(p.clone()),
                              current_session_duration: counter,
                              session_start_time: Some(chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.f").to_string()),
                              active_checkpoint_ids: vec![],
                          });
                      }
                    } else {
                      *snd = false;
                      break;
                    }
                  }
                }
            }

            active! { tx_arc_clone.send((format!("{};{}", p.to_owned(), counter), ReceiveTypes::LongestSession)).unwrap(); }

            if has_active_broadcaster() {
                broadcast_tracking_status_update(TrackingStatus {
                    is_tracking: false,
                    is_paused: false,
                    current_app: None,
                    current_session_duration: 0,
                    session_start_time: None,
                    active_checkpoint_ids: vec![],
                });
            }

            info!("Process: {} has finished at {}.", p, chrono::Local::now());
        });
    }

    Ok(())
}

fn check_processes<T: Restable + Send + 'static>(
    spawn_tx: Sender<String>,
    config: Arc<RwLock<TimeTrackingConfig>>,
    client: T,
) {
    #[cfg(not(feature = "memory"))]
    let check_delay_ms = config.read().unwrap().check_delay_ms;

    let rt = tokio::runtime::Runtime::new().unwrap();

    thread::spawn(move || {
        loop {
            let app_names = {
                let p_map = PROCESS_MAP.lock().unwrap();
                p_map.keys().cloned().collect::<Vec<String>>()
            };

            let processes = app_names.clone();

            #[cfg(feature = "memory")]
            let check_delay_ms = {
                let config = config.read().unwrap();
                config.check_delay_ms
            };

            let process_aliases = rt
                .block_on(client.get_process_aliases_by_app_names(&app_names))
                .unwrap_or_default();

            if let Ok(m) = are_processes_running(&processes[..], Some(&process_aliases)) {
                if m.is_empty() {
                    thread::sleep(Duration::from_millis(check_delay_ms));
                    continue;
                }

                let mut to_spawn = Vec::new();
                {
                    let mut p_map = PROCESS_MAP.lock().unwrap();
                    for (p, (fst, snd)) in p_map.iter_mut() {
                        if *m.get(p).unwrap() {
                            *fst = true;
                            if !*snd {
                                *snd = true;
                                to_spawn.push(p.to_owned());
                            }
                        } else {
                            *fst = false;
                        }
                    }
                }

                for p in to_spawn {
                    spawn_tx.send(p).unwrap();
                }
            }

            thread::sleep(Duration::from_millis(check_delay_ms));
        }
    });
}

pub async fn add_process<T: Restable>(
    process: &str,
    path: &str,
    client: &Arc<RwLock<T>>,
) -> Result<(), Box<dyn Error>> {
    if client
        .read()
        .unwrap()
        .get_processes()
        .await
        .map(|ref p| p.contains(&process.to_owned()))?
    {
        return box_err!(AddError(format!(
            "Process \"{}\" has already been added before.",
            process
        )));
    }

    let product_name = if let Some(p_name) = ver_query_value(path) {
        p_name
    } else {
        process.to_owned()
    };

    client
        .read()
        .unwrap()
        .put_data(process, &product_name)
        .await?;

    PROCESS_MAP
        .lock()
        .unwrap()
        .insert(process.to_owned(), (false, false));

    info!("Process \"{}\" has been added.", process);

    if has_active_broadcaster() {
        if let Ok(apps) = client.read().unwrap().get_all_apps().await {
            if let Ok(apps_vec) = serde_json::from_value::<Vec<crate::structs::App>>(apps) {
                broadcast_apps_update(apps_vec);
            }
        }
    }

    Ok(())
}

pub async fn delete_process<T: Restable>(
    process: &str,
    client: &Arc<RwLock<T>>,
) -> Result<(), Box<dyn Error>> {
    client.read().unwrap().delete_data(process).await?;

    PROCESS_MAP.lock().unwrap().remove(process);

    info!("Process \"{}\" has been deleted.", process);

    if has_active_broadcaster() {
        if let Ok(apps) = client.read().unwrap().get_all_apps().await {
            if let Ok(apps_vec) = serde_json::from_value::<Vec<crate::structs::App>>(apps) {
                broadcast_apps_update(apps_vec);
            }
        }
    }

    Ok(())
}

pub fn pause() {
    if !(*PAUSE.read().unwrap()) {
        *PAUSE.write().unwrap() = true;
        info!("\"time_tracker\" has been paused.");
    } else {
        *PAUSE.write().unwrap() = false;
        info!("\"time_tracker\" has been resumed.");
    }
}
