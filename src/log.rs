//! `MultipleAutomaticShoutouts.log` next to the DLL, overwritten on each launch. Lines go to a buffer and a
//! background thread writes them, so the game thread never waits on the disk.

use std::{fs::File, io::Write, path::Path, sync::Mutex, time::Duration};

static BUFFER: Mutex<String> = Mutex::new(String::new());

pub fn init(path: &Path) {
    let Ok(mut file) = File::create(path) else { return };
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let text = std::mem::take(&mut *BUFFER.lock().unwrap_or_else(|e| e.into_inner()));
            if !text.is_empty() {
                let _ = file.write_all(text.as_bytes());
                let _ = file.flush();
            }
        }
    });
}

pub fn write(text: &str) {
    BUFFER.lock().unwrap_or_else(|e| e.into_inner()).push_str(text);
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        $crate::log::write(&format!($($arg)*));
        $crate::log::write("\n");
    }};
}
