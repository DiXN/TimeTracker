#[cfg(target_os = "windows")]
fn main() {
  embed_resource::compile("src/windows/time_tracker.rc");
}

#[cfg(not(target_os = "windows"))]
fn main() { }
