use inputbot::{KeybdKey::*, *};
use std::thread;

use crate::native::get_foreground_meta;

pub fn init_hook() {
  thread::spawn(move || {
    HomeKey.bind(|| {
      if LShiftKey.is_pressed() && LControlKey.is_pressed() {
        dbg!(get_foreground_meta());
      }
    });

    handle_input_events();
  });
}
