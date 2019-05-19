use std::{
  mem,
  ffi::OsString,
  collections::HashMap,
  io::Error,
  io::ErrorKind
};

use winapi::um::tlhelp32::{
  CreateToolhelp32Snapshot,
  Process32FirstW,
  Process32NextW,
  TH32CS_SNAPPROCESS,
  PROCESSENTRY32W
};

use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::shared::minwindef::MAX_PATH;

use wio::wide::FromWide;

//reference: https://users.rust-lang.org/t/comparing-a-string-to-an-array-of-i8/5120
pub fn nt_are_processes_running<'a>(processes: &'a [String]) -> Result<HashMap<&'a String, bool>, Error> {
  let mut map = HashMap::new();

  let process_handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };

  if process_handle == INVALID_HANDLE_VALUE {
    return Err(Error::new(ErrorKind::Other, "invalid Handle"))
  }

  let mut process_entry : PROCESSENTRY32W = PROCESSENTRY32W {
    dwSize: mem::size_of::<PROCESSENTRY32W>() as u32,
    cntUsage: 0,
    th32ProcessID: 0,
    th32DefaultHeapID: 0,
    th32ModuleID: 0,
    cntThreads: 0,
    th32ParentProcessID: 0,
    pcPriClassBase: 0,
    dwFlags: 0,
    szExeFile: [0; MAX_PATH],
  };

  match unsafe { Process32FirstW(process_handle, &mut process_entry) } {
    1 => {
      let mut success : i32 = 1;

      while success == 1 {
        let process_name = OsString::from_wide(&process_entry.szExeFile);
        let mut s = process_name.to_string_lossy().into_owned();
        s.retain(|c| c != '\u{0}');

        for process in processes.iter() {
          if &s == process {
            map.insert(process, true);
            continue;
          }
        }

        success = unsafe { Process32NextW(process_handle, &mut process_entry) };
      }
    },
    0 | _ => {
      unsafe { CloseHandle(process_handle) };
      return Err(Error::new(ErrorKind::Other, "invalid Handle"))
    }
  }

  for process in processes.iter() {
    if !map.contains_key(process) {
      map.insert(process, false);
    }
  }

  unsafe { CloseHandle(process_handle) };

  Ok(map)
}
