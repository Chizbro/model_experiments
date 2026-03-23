use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use api_types::WorkerLogEntry;

/// Local file logger that writes structured JSON log entries to disk.
///
/// Writes one JSONL file per job: `{log_dir}/{session_id}/{job_id}.jsonl`
#[derive(Debug, Clone)]
pub struct FileLogger {
    file_path: PathBuf,
}

impl FileLogger {
    /// Create a new FileLogger, ensuring the parent directory exists.
    pub fn new(log_dir: &Path, session_id: &str, job_id: &str) -> std::io::Result<Self> {
        let dir = log_dir.join(session_id);
        fs::create_dir_all(&dir)?;
        let file_path = dir.join(format!("{}.jsonl", job_id));
        Ok(Self { file_path })
    }

    /// Append a single log entry as a JSON line to the file.
    pub fn write_entry(&self, entry: &WorkerLogEntry) {
        if let Err(e) = self.write_entry_inner(entry) {
            tracing::warn!(path = %self.file_path.display(), %e, "failed to write log entry to file");
        }
    }

    fn write_entry_inner(&self, entry: &WorkerLogEntry) -> std::io::Result<()> {
        let json = serde_json::to_string(entry).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;
        writeln!(file, "{}", json)?;
        Ok(())
    }

    /// Returns the path to the log file.
    pub fn path(&self) -> &Path {
        &self.file_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::LogLevel;
    use chrono::Utc;

    #[test]
    fn test_file_logger_creates_and_writes() {
        let tmp = tempfile::tempdir().unwrap();
        let logger = FileLogger::new(tmp.path(), "sess-1", "job-1").unwrap();

        let entry = WorkerLogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            message: "hello world".to_string(),
            source: "test".to_string(),
        };
        logger.write_entry(&entry);

        let contents = fs::read_to_string(logger.path()).unwrap();
        assert!(contents.contains("hello world"));
        assert!(contents.contains("\"level\":"));
    }

    #[test]
    fn test_file_logger_appends_multiple() {
        let tmp = tempfile::tempdir().unwrap();
        let logger = FileLogger::new(tmp.path(), "sess-2", "job-2").unwrap();

        for i in 0..3 {
            let entry = WorkerLogEntry {
                timestamp: Utc::now(),
                level: LogLevel::Info,
                message: format!("line {}", i),
                source: "test".to_string(),
            };
            logger.write_entry(&entry);
        }

        let contents = fs::read_to_string(logger.path()).unwrap();
        let lines: Vec<&str> = contents.trim().lines().collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_file_logger_creates_nested_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let logger = FileLogger::new(tmp.path(), "deep/session", "job-3").unwrap();
        assert!(logger.path().parent().unwrap().exists());
    }

    #[test]
    fn test_file_logger_path() {
        let tmp = tempfile::tempdir().unwrap();
        let logger = FileLogger::new(tmp.path(), "s1", "j1").unwrap();
        assert!(logger.path().to_str().unwrap().ends_with("j1.jsonl"));
    }
}
