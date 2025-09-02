use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex, RwLock},
    thread,
    time::Duration,
};

use crossbeam_channel::{Sender, unbounded};

use log::info;

use crate::hook::init_hook;
use crate::native::{are_processes_running, ver_query_value};
use crate::receive_types::ReceiveTypes;
use crate::restable::Restable;
use crate::rpc::init_rpc;
use crate::web_socket::init_web_socket;
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

//"time_tracker" is not paused.
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

    // Create a shared config that can be updated
    let shared_config = Arc::new(RwLock::new(config));

    init_rpc(client.clone());

    init_web_socket(client.clone(), Arc::clone(&shared_config));

    client.init_event_loop(rx);
    check_processes(spawn_tx, Arc::clone(&shared_config));

    while let Ok(p) = spawn_rx.recv() {
        let tx_arc_clone = tx_arc.clone();
        let config_clone = Arc::clone(&shared_config);

        thread::spawn(move || {
            #[cfg(not(feature = "memory"))]
            let tracking_delay_ms = config_clone.read().unwrap().tracking_delay_ms;

            active! { tx_arc_clone.send((p.to_owned(), ReceiveTypes::Launches)).unwrap(); };

            let mut counter = 0;

            loop {
                // Read the current config values (only for memory feature)
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
                    } else {
                      *snd = false;
                      break;
                    }
                  }
                }
            }

            active! { tx_arc_clone.send((format!("{};{}", p.to_owned(), counter), ReceiveTypes::LongestSession)).unwrap(); }

            info!("Process: {} has finished.", p)
        });
    }

    Ok(())
}

fn check_processes(spawn_tx: Sender<String>, config: Arc<RwLock<TimeTrackingConfig>>) {
    #[cfg(not(feature = "memory"))]
    let check_delay_ms = config.read().unwrap().check_delay_ms;

    thread::spawn(move || {
        loop {
            let p_map = PROCESS_MAP.lock().unwrap();

            let processes = p_map
                .keys()
                .map(|key| format!("{}.exe", key))
                .collect::<Vec<String>>();

            drop(p_map);

            // Read the current config values (only for memory feature)
            #[cfg(feature = "memory")]
            let check_delay_ms = {
                let config = config.read().unwrap();
                config.check_delay_ms
            };

            if let Ok(m) = are_processes_running(&processes[..]) {
                if m.is_empty() {
                    thread::sleep(Duration::from_millis(check_delay_ms));
                    continue;
                }

                for (p, (fst, snd)) in PROCESS_MAP.lock().unwrap().iter_mut() {
                    if *m.get(&format!("{}.exe", p)).unwrap() {
                        *fst = true;
                        if !*snd {
                            *snd = true;
                            spawn_tx.send(p.to_owned()).unwrap();
                        }
                    } else {
                        *fst = false;
                    }

                    //debug!("{}, {}, {}", p, fst, snd);
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

    Ok(())
}

pub async fn delete_process<T: Restable>(
    process: &str,
    client: &Arc<RwLock<T>>,
) -> Result<(), Box<dyn Error>> {
    client.read().unwrap().delete_data(process).await?;

    PROCESS_MAP.lock().unwrap().remove(process);

    info!("Process \"{}\" has been deleted.", process);

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
