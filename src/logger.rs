use std::fs::OpenOptions;
use std::io::Write;

/// Initialize the logger
pub fn init() {
    // Create directory if it doesn't exist
    let _ = std::fs::create_dir_all("/tmp/scame");
    log("=== SCAME SESSION START ===");
}

/// Log a message to /tmp/scame/logs
pub fn log(message: &str) {
    // Open file each time (less efficient but won't deadlock)
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/scame/logs")
    {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(file, "[{}] {}", timestamp, message);
    }
}

/// Log a debug message
#[allow(unused)]
pub fn debug(message: &str) {
    log(&format!("DEBUG: {}", message));
}

/// Log an error message
#[allow(unused)]
pub fn error(message: &str) {
    log(&format!("ERROR: {}", message));
}

/// Close the logger
pub fn close() {
    log("=== SCAME SESSION END ===");
}
