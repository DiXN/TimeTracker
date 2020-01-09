use std::{
  thread,
  sync::{Arc, RwLock},
  io::{Cursor, BufReader}
};

use inputbot::{KeybdKey::*, *};
use rodio::{default_output_device, play_once};

use crate::native::get_foreground_meta;
use crate::restable::Restable;
use crate::time_tracking::{add_process, pause};
use crate::Asset;

pub fn init_hook<T>(client: T) where T : Restable + Sync + Send + 'static {
  let client_arc = Arc::new(RwLock::new(client));

  thread::spawn(move || {
    let device = Arc::new(default_output_device().unwrap());
    let cursor = Arc::new(Cursor::new(Asset::get("when.ogg").unwrap()));

    let add_device_ref = device.clone();
    let add_cursor_ref = cursor.clone();

    HomeKey.bind(move || {
      if LShiftKey.is_pressed() && LControlKey.is_pressed() {
        if let (Some(path), Some(file_name)) = get_foreground_meta() {
          if let Err(e) = add_process(&file_name, &path, &client_arc) {
            error!("{}", e);
          } else {
              let when = play_once(&(*add_device_ref), BufReader::new((*add_cursor_ref).clone())).unwrap();
              when.set_volume(0.1);
              when.detach();
          }
        } else {
          error!("Cannot add process.");
        }
      }
    });

    let pause_device_ref = device.clone();
    let pause_cursor_ref = cursor.clone();

    PKey.bind(move || {
      if LShiftKey.is_pressed() && LControlKey.is_pressed() {
        let when = play_once(&(*pause_device_ref), BufReader::new((*pause_cursor_ref).clone())).unwrap();
        when.set_volume(0.1);
        when.detach();

        pause();
      }
    });

    handle_input_events();
  });
}
