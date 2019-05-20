use std::{
  mem,
  ffi::OsString,
  collections::HashMap,
  io::Error,
  io::ErrorKind
};

use regex::Regex;

use winapi::um::tlhelp32::{
  CreateToolhelp32Snapshot,
  Process32FirstW,
  Process32NextW,
  TH32CS_SNAPPROCESS,
  PROCESSENTRY32W
};

use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::shared::minwindef::{DWORD, MAX_PATH};

use winapi::um::winver::{GetFileVersionInfoSizeA, GetFileVersionInfoA};
use winapi::ctypes::c_void;

use wio::wide::FromWide;

use crate::n_str;

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

pub fn nt_ver_query_value(path: &str) -> Option<String> {
  let mut ver_handle : DWORD = 0;
  let n_path = n_str!(path);
  let ver_size = unsafe { GetFileVersionInfoSizeA(n_path.as_ptr(), &mut ver_handle) };

  if ver_size == 0 {
    return None;
  }

  let mut buffer : Vec<u8> = Vec::with_capacity(ver_size as usize);
  unsafe { buffer.set_len(ver_size as usize) };

  if unsafe { GetFileVersionInfoA(n_path.as_ptr(), ver_handle, ver_size, buffer.as_mut_ptr() as *mut c_void) } != 0 {
    let file_info = String::from_utf8_lossy(&buffer).into_owned();

    let replace_regex = Regex::new(r"[^0-9a-zA-Z :]").unwrap();
    let res = replace_regex.replace_all(&file_info, "");

    let product_name_regex = Regex::new(r"ProductName(.*).ProductVersion").unwrap();

    let mut matches = product_name_regex.captures_iter(&res);

    matches
      .next()
      .and_then(|r| r.get(1))
      .map(|p| p.as_str().to_owned())
  } else {
    None
  }
}
