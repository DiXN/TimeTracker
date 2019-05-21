use std::{
  io::Error,
  collections::HashMap,
  error::Error as Std_Error,
  process::ExitStatus
};

use crate::windows::{nt_are_processes_running, nt_ver_query_value, nt_autostart, nt_init_tray};

#[cfg(windows)]
pub fn are_processes_running<'a>(processes: &'a [String]) -> Result<HashMap<&'a String, bool>, Error> {
  nt_are_processes_running(processes)
}

#[cfg(not(windows))]
pub fn are_processes_running<'a>(processes: &'a [String]) -> Result<HashMap<&'a String, bool>, Error> {
  Err(Error::new(ErrorKind::Other, "Not implemented on this platform!"))
}

#[cfg(windows)]
pub fn ver_query_value(path: &str) -> Option<String> {
  nt_ver_query_value(path)
}

#[cfg(not(windows))]
pub fn ver_query_value(path: &str) -> Option<String> {
  None
}

#[cfg(target_os = "windows")]
pub fn autostart() -> Result<ExitStatus, Box<dyn Std_Error>> {
  nt_autostart()
}

#[cfg(not(target_os = "windows"))]
pub fn autostart() -> Result<ExitStatus, Box<dyn Std_Error>> {
  info!("\"autostart\" is currently not supported on your system.");

  let mut process = Command::new("echo")
    .arg("\"autostart\" is currently not supported on your system.")
    .spawn()?;

  Ok(process.wait()?)
}

#[cfg(target_os = "windows")]
pub fn init_tray() {
  nt_init_tray();
}

#[cfg(not(target_os = "windows"))]
pub fn init_tray() {

}
