use std::{
  mem,
  str,
  env,
  ptr,
  thread,
  ffi::OsString,
  collections::HashMap,
  io::Error,
  io::ErrorKind,
  error::Error as Std_Error,
  process::{Command, Stdio, ExitStatus, exit},
  io::{BufWriter, Write},
  path::Path
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

use winapi::um::winuser::{ShowWindow, GetForegroundWindow, GetWindowThreadProcessId};
use winapi::um::wincon::GetConsoleWindow;

use winapi::um::winnt::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
use winapi::um::processthreadsapi::OpenProcess;
use winapi::um::winbase::QueryFullProcessImageNameW;

use wio::wide::FromWide;
use systray::Application;

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

pub fn nt_autostart() -> Result<ExitStatus, Box<dyn Std_Error>> {
  let auto_cmd = Command::new("powershell")
    .args(&["-Command", "[environment]::getfolderpath(\"Startup\")"])
    .output()?;

  let auto_path = str::from_utf8(&auto_cmd.stdout)?.trim();

  let mut process = Command::new("powershell")
    .args(&["-Command", "-"])
    .stdin(Stdio::piped())
    .spawn()?;

  {
    let mut out_stdin = process.stdin.as_mut().expect("Could not collect stdin.");

    let mut writer = BufWriter::new(&mut out_stdin);

    match env::current_exe() {
      Ok(exe_path) => {
        //reference: https://stackoverflow.com/a/47340271
        writer.write_all("$WshShell = New-Object -comObject WScript.Shell;".as_bytes())?;
        writer.write_all(format!("$Shortcut = $WshShell.CreateShortcut(\"{}\\time_tracker.lnk\");", auto_path).as_bytes())?;
        writer.write_all(format!("$Shortcut.TargetPath = \"{}\";", exe_path.display()).as_bytes())?;
        writer.write_all("$Shortcut.WindowStyle = 7;".as_bytes())?;
        writer.write_all("$Shortcut.Save();".as_bytes())?;
      },
      Err(e) => panic!(e)
    };
  }

  Ok(process.wait()?)
}

pub fn nt_get_foreground_meta() -> (Option<String>, Option<String>) {
  let window = unsafe { GetForegroundWindow() };
  let mut thread_id : u32 = 0;
  unsafe { GetWindowThreadProcessId(window, &mut thread_id) };
  let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, thread_id) };

  if handle != ptr::null_mut() {
    let mut buffer : Vec<u16> = Vec::with_capacity(MAX_PATH);
    unsafe { buffer.set_len(MAX_PATH) };

    if unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut (MAX_PATH as u32)) } > 0 {
      let process_name = OsString::from_wide(&buffer)
                          .to_string_lossy()
                          .into_owned();

      let process_name = process_name.find(".exe")
                          .and_then(|idx| Some(
                            process_name
                              .chars()
                              .take(idx + 4)
                              .collect::<String>()
                          ));

      let file_name = process_name
                        .to_owned()
                        .and_then(|pn| Path::new(&pn)
                                        .file_name()
                                        .and_then(|p| Some(p.to_string_lossy().into_owned())));

      return (process_name, file_name);
    }
  }

  (None, None)
}

pub fn nt_init_tray() {
  use crate::time_tracking::pause;

  thread::spawn(move || {
    if let Ok(mut app) = Application::new() {
      let window = unsafe { GetConsoleWindow() };

      if window != ptr::null_mut() {
        unsafe {
          ShowWindow(window, 0);
        }
      }

      match app.set_icon_from_resource(&"tray_icon".to_string()) {
        Ok(_) => (),
        Err(e) => error!("{}", e)
      };

      app.set_tooltip(&"time_tracker".to_owned()).ok();

      app.add_menu_item(&"Show".to_string(), move |_| {
        if window != ptr::null_mut() {
          unsafe {
            ShowWindow(window, 5);
          }
        }
      }).ok();

      app.add_menu_item(&"Hide".to_string(), move |_| {
        if window != ptr::null_mut() {
          unsafe {
            ShowWindow(window, 0);
          }
        }
      }).ok();

      app.add_menu_separator().ok();

      app.add_menu_item(&"Pause".to_string(), move |_| {
        if pause() {
          info!("\"time_tracker\" has been paused.");
        } else {
          info!("\"time_tracker\" has been resumed.");
        }
      }).ok();

      app.add_menu_separator().ok();

      app.add_menu_item(&"Quit".to_string(), |_| exit(0)).ok();
      app.wait_for_message();
    }
  });
}
