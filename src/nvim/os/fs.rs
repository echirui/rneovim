use std::path::Path;
use std::fs;
use std::io::{BufRead, BufReader};

pub fn os_path_exists(path: &str) -> bool {
    Path::new(path).exists()
}

pub fn os_canpath(path: &str) -> Result<String, std::io::Error> {
    let p = fs::canonicalize(path)?;
    Ok(p.to_string_lossy().into_owned())
}

pub fn os_mkdir(path: &str, mode: u32) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.mode(mode);
        builder.create(path)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(path)
    }
}

pub fn os_getcwd() -> Result<String, std::io::Error> {
    let p = std::env::current_dir()?;
    Ok(p.to_string_lossy().into_owned())
}

pub fn os_read_file(path: &str) -> Result<Vec<String>, std::io::Error> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    
    // lossyな変換を行うことでUTF-8エラーでのクラッシュを防ぐ
    for line_result in reader.split(b'\n') {
        let bytes = line_result?;
        lines.push(String::from_utf8_lossy(&bytes).to_string());
    }
    Ok(lines)
}

pub fn os_write_file(path: &str, lines: &[String]) -> Result<(), std::io::Error> {
    use std::io::Write;
    let mut file = fs::File::create(path)?;
    for line in lines {
        writeln!(file, "{}", line)?;
    }
    Ok(())
}

pub fn os_isdir(path: &str) -> bool {
    Path::new(path).is_dir()
}

pub struct OsFileWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl OsFileWatcher {
    pub fn new<F>(path: &str, mut callback: F) -> Result<Self, notify::Error>
    where
        F: FnMut(notify::Result<notify::Event>) + Send + 'static,
    {
        use notify::Watcher;
        let mut watcher = notify::recommended_watcher(move |res| {
            callback(res);
        })?;
        watcher.watch(Path::new(path), notify::RecursiveMode::NonRecursive)?;
        Ok(Self { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_os_read_write_file() {
        let test_file = "test_io.tmp";
        let lines = vec!["Line 1".to_string(), "日本語".to_string()];
        os_write_file(test_file, &lines).unwrap();
        let read_lines = os_read_file(test_file).unwrap();
        assert_eq!(lines[0], read_lines[0]);
        assert_eq!(lines[1], read_lines[1]);
        fs::remove_file(test_file).unwrap();
    }

    #[test]
    fn test_path_utils() {
        assert!(os_path_exists("Cargo.toml"));
        assert!(!os_path_exists("non_existent_file"));
        assert!(os_isdir("."));
        assert!(!os_isdir("Cargo.toml"));
        
        // test os_canpath
        let can = os_canpath("Cargo.toml").unwrap();
        assert!(can.ends_with("Cargo.toml"));
        assert!(can.starts_with("/")); // Absolute path
    }

    #[test]
    fn test_os_mkdir() {
        let test_dir = "test_mkdir_tmp";
        os_mkdir(test_dir, 0o755).unwrap();
        assert!(os_isdir(test_dir));
        fs::remove_dir(test_dir).unwrap();
    }
}
