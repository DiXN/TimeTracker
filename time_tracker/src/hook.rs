use std::{
    io::{BufReader, Cursor},
    sync::{Arc, RwLock},
    thread,
};

use inputbot::{KeybdKey::*, *};

use crate::Asset;
use crate::native::get_foreground_meta;
use crate::restable::Restable;
use crate::time_tracking::{add_process, pause};

pub fn init_hook<T>(client: T)
where
    T: Restable + Sync + Send + 'static,
{
    let client_arc = Arc::new(RwLock::new(client));

    thread::spawn(move || {
        let stream_handle = Arc::new(
            rodio::OutputStreamBuilder::open_default_stream().expect("open default audio stream"),
        );

        let embedded = Asset::get("when.ogg").expect("missing asset");

        let cursor = Arc::new(Cursor::new(embedded.data));

        let add_device_ref = stream_handle.clone();
        let add_cursor_ref = cursor.clone();

        HomeKey.bind(move || {
            if LShiftKey.is_pressed() && LControlKey.is_pressed() {
                if let (Some(path), Some(file_name)) = get_foreground_meta() {
                    if let Err(e) = add_process(&file_name, &path, &client_arc) {
                        error!("{}", e);
                    } else {
                        let file = BufReader::new((*add_cursor_ref).clone());

                        rodio::play((add_device_ref).clone().mixer(), file)
                            .map(|op| op.set_volume(0.1))
                            .expect("Cannot play audio");
                    }
                } else {
                    error!("Cannot add process.");
                }
            }
        });

        let pause_device_ref = stream_handle.clone();
        let pause_cursor_ref = cursor.clone();

        PKey.bind(move || {
            if LShiftKey.is_pressed() && LControlKey.is_pressed() {
                let file = BufReader::new((*pause_cursor_ref).clone());

                rodio::play((pause_device_ref).clone().mixer(), file)
                    .map(|op| op.set_volume(0.1))
                    .expect("Cannot play audio.");

                pause();
            }
        });

        handle_input_events();
    });
}
