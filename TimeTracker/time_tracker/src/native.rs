use std::io::Error;
use std::collections::HashMap;

use crate::windows::{nt_are_processes_running, nt_ver_query_value};

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

