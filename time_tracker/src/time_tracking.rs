use std::{
  collections::HashMap,
  sync::{Arc, Mutex, RwLock},
  thread,
  time::Duration,
  error::Error
};

use crossbeam_channel::{Sender, unbounded};

use crate::receive_types::ReceiveTypes;
use crate::rpc::init_rpc;
use crate::restable::Restable;
use crate::native::{are_processes_running, ver_query_value};
use crate::hook::init_hook;
use crate::error::AddError;
use crate::box_err;

lazy_static! {
  static ref PROCESS_MAP: Mutex<HashMap<String, (bool, bool)>> = {
    Mutex::new(HashMap::new())
  };

  static ref PAUSE: RwLock<bool> = {
    RwLock::new(false)
  };
}

//"time_tracker" is not paused.
macro_rules! active {
  { $($b:tt)* } => {{
    if !(*PAUSE.read().unwrap()) {
      $($b)*
    }
  }}
}

pub fn init<T>(client: T) -> Result<(), Box<dyn Error>> where T : Restable + Clone + Sync + Send + 'static {
  for p in client.get_processes()? {
    PROCESS_MAP
      .lock()
      .unwrap()
      .insert(p, (false, false));
  }

  init_hook(client.clone());

  let (tx, rx) = unbounded();
  let tx_arc = Arc::new(tx);

  let (spawn_tx, spawn_rx) = unbounded();

  init_rpc(client.clone());
  client.init_event_loop(rx);
  check_processes(spawn_tx);

  while let Ok(p) = spawn_rx.recv() {
    let tx_arc_clone = tx_arc.clone();

    thread::spawn(move || {
      active! { tx_arc_clone.send((p.to_owned(), ReceiveTypes::LAUNCHES)).unwrap(); };

      let mut counter = 0;

      loop {
        thread::sleep(Duration::from_secs(60));

        active! {
          if let Some((fst, snd)) = PROCESS_MAP.lock().unwrap().get_mut(&p) {
            if *fst {
              tx_arc_clone.send((p.to_owned(), ReceiveTypes::DURATION)).unwrap();
              tx_arc_clone.send((p.to_owned(), ReceiveTypes::TIMELINE)).unwrap();
              counter += 1;
            } else {
              *snd = false;
              break;
            }
          }
        }
      }

      active! { tx_arc_clone.send((format!("{};{}", p.to_owned(), counter.to_string()), ReceiveTypes::LONGEST_SESSION)).unwrap(); }

      info!("Process: {} has finished.", p)
    });
  }

  Ok(())
}

fn check_processes(spawn_tx: Sender<String>) {
  thread::spawn(move|| {
    loop {
      let p_map = PROCESS_MAP.lock().unwrap();

      let processes = p_map
                        .iter()
                        .map(|(key, _)| format!("{}.exe", key))
                        .collect::<Vec<String>>();

      drop(p_map);


      if let Ok(m) = are_processes_running(&processes[..]) {
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

      thread::sleep(Duration::from_millis(10000));
    }
  });
}

pub fn add_process<T: Restable>(process: &str, path: &str, client: &Arc<RwLock<T>>) -> Result<(), Box<dyn Error>> {
  if client
      .read()
      .unwrap()
      .get_processes()
      .map(|ref p| p.contains(&process.to_owned()))? {
    return box_err!(AddError(format!("Process \"{}\" has already been added before.", process)));
  }

  let product_name = if let Some(p_name) = ver_query_value(path) {
    p_name
  } else {
    process.to_owned()
  };

  client.read().unwrap().put_data(process, &product_name)?;

  PROCESS_MAP
    .lock()
    .unwrap()
    .insert(process.to_owned(), (false, false));

  info!("Process \"{}\" has been added.", process);

  Ok(())
}

pub fn delete_process<T: Restable>(process: &str, client: &Arc<RwLock<T>>) -> Result<(), Box<dyn Error>> {
  client.read().unwrap().delete_data(process)?;

  PROCESS_MAP
    .lock()
    .unwrap()
    .remove(process);

  info!("Process \"{}\" has been deleted.", process);

  Ok(())
}

pub fn pause() -> bool {
  if !(*PAUSE.read().unwrap()) {
    *PAUSE.write().unwrap() = true;
  } else {
    *PAUSE.write().unwrap() = false;
  }

  *PAUSE.read().unwrap()
}
