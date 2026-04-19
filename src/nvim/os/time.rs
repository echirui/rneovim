use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the high-resolution time in nanoseconds.
/// Equivalent to Neovim's os_hrtime.
pub fn os_hrtime() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos() as u64
}

/// Returns the high-resolution time in microseconds.
/// Equivalent to Neovim's os_microtime.
pub fn os_microtime() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_increments() {
        let t1 = os_hrtime();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let t2 = os_hrtime();
        assert!(t2 > t1);
    }
}
