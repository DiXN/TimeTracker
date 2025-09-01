use std::process::Command;
use std::{collections::HashMap, error::Error as Std_Error, io::Error, process::ExitStatus};
use log::info;

#[cfg(windows)]
use crate::windows::{
    nt_are_processes_running, nt_autostart, nt_get_foreground_meta, nt_init_tray,
    nt_ver_query_value,
};

#[cfg(not(windows))]
use crate::linux::ux_are_processes_running;
use crate::linux::ux_get_foreground_meta;

#[macro_export]
macro_rules! n_str {
    ($n_str:expr) => {
        std::ffi::CString::new($n_str).expect("could not create CString")
    };
}

#[macro_export]
macro_rules! ns_invoke {
  ($func:expr, $($param:expr)*) => {{
    let ret = unsafe { $func($($param.as_ptr()), *) };

    if !ret.is_null() {
      let n_str = unsafe { std::ffi::CStr::from_ptr(ret) };
      let mut n_str = n_str.to_string_lossy().into_owned();
      n_str.retain(|c| c != '\u{FFFD}');
      n_str
    } else {
      String::from("")
    }
  }};
}

#[cfg(windows)]
pub fn are_processes_running(processes: &[String]) -> Result<HashMap<&String, bool>, Error> {
    nt_are_processes_running(processes)
}

#[cfg(target_os = "linux")]
pub fn are_processes_running(processes: &[String]) -> Result<HashMap<&String, bool>, Error> {
    ux_are_processes_running(processes)
}

#[cfg(windows)]
pub fn ver_query_value(path: &str) -> Option<String> {
    nt_ver_query_value(path)
}

#[cfg(not(windows))]
pub fn ver_query_value(_path: &str) -> Option<String> {
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
pub fn init_tray() {}

#[cfg(target_os = "windows")]
pub fn get_foreground_meta() -> (Option<String>, Option<String>) {
    nt_get_foreground_meta()
}

#[cfg(target_os = "linux")]
pub fn get_foreground_meta() -> (Option<String>, Option<String>) {
    ux_get_foreground_meta()
}
