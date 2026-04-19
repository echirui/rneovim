use crate::nvim::error::Result;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// スワップファイル (.swp) の簡易実装
pub struct MemFile {
    pub path: PathBuf,
    file: Option<File>,
}

impl MemFile {
    pub fn new(buffer_id: i32) -> Self {
        // 簡易的に /tmp/ またはカレントディレクトリに作成
        let mut path = std::env::temp_dir();
        path.push(format!("rneovim_buf_{}.swp", buffer_id));
        
        Self {
            path,
            file: None,
        }
    }

    pub fn open(&mut self) -> Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(true) // 簡易化のため毎回リセット
            .open(&self.path)?;
        self.file = Some(file);
        Ok(())
    }

    /// 変更のログなどを書き込む
    pub fn write_log(&mut self, text: &str) -> Result<()> {
        if let Some(file) = &mut self.file {
            writeln!(file, "{}", text)?;
            file.flush()?;
        }
        Ok(())
    }

    pub fn close(&mut self) {
        self.file = None;
        let _ = fs::remove_file(&self.path);
    }
}

impl Drop for MemFile {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap_file_creation() {
        let mut mf = MemFile::new(999);
        assert!(!mf.path.exists());
        
        mf.open().unwrap();
        assert!(mf.path.exists());
        
        mf.write_log("EDIT: line 1").unwrap();
        
        // 読み込んで確認
        let content = fs::read_to_string(&mf.path).unwrap();
        assert_eq!(content, "EDIT: line 1\n");
        
        mf.close();
        assert!(!mf.path.exists());
    }
}
