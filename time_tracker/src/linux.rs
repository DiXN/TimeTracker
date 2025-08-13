use std::{
    collections::HashMap,
    env,
    error::Error as Std_Error,
    fs,
    io::{Error, ErrorKind},
    process::Command,
    thread,
};

use procfs::process::Process;
use sysinfo::{Pid, System};

use niri_ipc::socket::Socket;
use niri_ipc::{Request, Response};

fn is_x11_session() -> bool {
    env::var("XDG_SESSION_TYPE").map_or(false, |session_type| session_type == "x11")
}

fn is_niri_session() -> bool {
    env::var("NIRI_SOCKET").is_ok()
}

fn get_active_window_x11() -> Result<(String, String), Box<dyn Std_Error>> {
    // Get active window ID using xprop
    let active_output = Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .output()?;

    if !active_output.status.success() {
        return Err("xprop failed to get active window".into());
    }

    let active_line = String::from_utf8_lossy(&active_output.stdout);
    let window_id = active_line
        .split_whitespace()
        .last()
        .ok_or("Could not parse window ID")?;

    // Get window list with PIDs using wmctrl
    let output = Command::new("wmctrl").args(["-l", "-p"]).output()?;

    if !output.status.success() {
        return Err("wmctrl failed".into());
    }

    let window_list = String::from_utf8_lossy(&output.stdout);

    // Find the active window in the list
    for line in window_list.lines() {
        if line.starts_with(window_id) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                if let Ok(pid) = parts[2].parse::<i32>() {
                    if let Ok(process) = Process::new(pid) {
                        if let Ok(exe) = process.exe() {
                            let path = exe.to_string_lossy().into_owned();
                            let name = exe
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "unknown".to_string());
                            return Ok((path, name));
                        }
                    }
                }
            }
        }
    }

    Err("Could not determine active window".into())
}

fn get_active_window_niri() -> Result<(String, String), Box<dyn Std_Error>> {
    let mut socket = Socket::connect()?;

    socket
        .send(Request::Windows)?
        .ok()
        .and_then(|resp| {
            if let Response::Windows(ws) = resp {
                Some(ws)
            } else {
                None
            }
        })
        .and_then(|windows| windows.into_iter().find(|w| w.is_focused))
        .and_then(|window| window.pid)
        .and_then(|pid| {
            let sys = System::new_all();
            let sys_pid = Pid::from(pid as usize);

            sys.process(sys_pid).map(|proc| {
                let exe = proc
                    .exe()
                    .map(|exe| exe.to_string_lossy().into_owned())
                    .unwrap_or_default();

                let cmd = proc
                    .cmd()
                    .iter()
                    .map(|os| os.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" ");

                (exe, cmd)
            })
        })
        .ok_or("Could not determine active window".into())
}

pub fn ux_are_processes_running<'a>(
    processes: &'a [String],
) -> Result<HashMap<&'a String, bool>, Error> {
    let mut map = HashMap::new();
    let sys = System::new_all();

    for (_, process) in sys.processes() {
        let process_name = process.name().to_str().unwrap_or("");
        for target_process in processes {
            if target_process == &format!("{}.exe", process_name) {
                map.insert(target_process, true);
            }
        }
    }

    Ok(map)
}

pub fn ux_get_foreground_meta() -> (Option<String>, Option<String>) {
    if is_x11_session() {
        match get_active_window_x11() {
            Ok((path, name)) => (Some(path), Some(name)),
            Err(_) => (None, None),
        }
    } else {
        if is_niri_session() {
            match get_active_window_niri() {
                Ok((path, name)) => (Some(path), Some(name)),
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        }
    }
}

pub fn init_tray() {
    thread::spawn(move || {});
}
