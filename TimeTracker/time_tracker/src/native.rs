use libc::c_char;

extern {
  pub fn query_file_info(process: *const c_char) -> *const c_char;
  pub fn is_process_running(process: *const c_char) -> bool;
}

macro_rules! n_str {
  ($n_str:expr) => (CString::new($n_str).expect("could not create CString"));
}

macro_rules! ns_invoke {
  ($func:expr, $($param:expr)*) => {{
    let n_str = unsafe { CStr::from_ptr($func($($param.as_ptr()), *)) };
    let mut n_str = n_str.to_string_lossy().into_owned();
    n_str.retain(|c| c != '\u{FFFD}');
    n_str
  }};
}
