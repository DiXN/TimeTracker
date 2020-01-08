use std::{
  thread,
  sync::{Arc, RwLock},
  io::{Cursor, BufReader}
};

use inputbot::{KeybdKey::*, *};
use rodio::{default_output_device, play_once};

use crate::native::get_foreground_meta;
use crate::restable::Restable;
use crate::time_tracking::add_process;
use crate::Asset;

pub fn init_hook<T>(client: T) where T : Restable + Sync + Send + 'static {
  let client_arc = Arc::new(RwLock::new(client));
  thread::spawn(move || {

    let add_ref = client_arc.clone();
    HomeKey.bind(move || {
      if LShiftKey.is_pressed() && LControlKey.is_pressed() {
        if let (Some(path), Some(file_name)) = get_foreground_meta() {
          if let Err(e) = add_process(&file_name, &path, &add_ref) {
            error!("Cannot add process. \n{}", e);
          } else {
            let device = default_output_device().unwrap();
            let cursor = Cursor::new(Asset::get("when.ogg").unwrap());

            let when = play_once(&device, BufReader::new(cursor)).unwrap();
            when.set_volume(0.2);
            when.detach();
          }
        } else {
          error!("Cannot add process.");
        }
      }
    });

    handle_input_events();
  });
}
