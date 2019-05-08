use crate::restable::Restable;

use std::{
  ffi::CString,
  collections::HashMap,
  sync::{Arc, Mutex},
  thread,
  time::Duration,
  error::Error
};

use crossbeam_channel::{Sender, unbounded};

#[macro_use]
use crate::native::is_process_running;

use crate::receive_types::ReceiveTypes;

pub fn init<T: Restable>(client: T) -> Result<(), Box<dyn Error>> {
  let mut processes_map = HashMap::new();

  for p in client.get_processes()? {
    processes_map.insert(p, (false, false));
  }

  let processes_map = Arc::new(Mutex::new(processes_map));

  let (tx, rx) = unbounded();
  let tx_arc = Arc::new(tx);

  let (spawn_tx, spawn_rx) = unbounded();

  client.init_event_loop(rx);
  check_processes(spawn_tx, processes_map.clone());

  while let Ok(p) = spawn_rx.recv() {
    let thread_process_map = processes_map.clone();
    let tx_arc_clone = tx_arc.clone();

    thread::spawn(move || {
      tx_arc_clone.send((p.to_owned(), ReceiveTypes::LAUNCHES)).unwrap();
      let mut counter = 0;

      loop {
        if let Some((fst, snd)) = thread_process_map.lock().unwrap().get_mut(&p) {
          if *fst {
            tx_arc_clone.send((p.to_owned(), ReceiveTypes::DURATION)).unwrap();
            tx_arc_clone.send((p.to_owned(), ReceiveTypes::TIMELINE)).unwrap();
            counter += 1;
          } else {
            *snd = false;
            break;
          }
        }

        thread::sleep(Duration::from_millis(60000));
      }

      tx_arc_clone.send((format!("{};{}", p.to_owned(), counter.to_string()), ReceiveTypes::LONGEST_SESSION)).unwrap();
      info!("Process: {} has finished.", p)
    });
  }



  Ok(())
}

fn check_processes(spawn_tx: Sender<String>, process_map: Arc<Mutex<HashMap<String, (bool, bool)>>>) {
  thread::spawn(move|| {
    loop {
      for (p, (fst, snd)) in process_map.lock().unwrap().iter_mut() {
        if unsafe { is_process_running(CString::new(format!("{}.exe", p)).unwrap().as_ptr()) } {
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

      thread::sleep(Duration::from_millis(10000));
    }
  });
}
