use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct AgentLogger {
    log_dir: PathBuf,
    log_file: Option<PathBuf>,
    index: usize,
}

impl AgentLogger {
    pub fn new() -> Self {
        let mut dir = dirs::home_dir().unwrap_or_default();
        dir.push(".miniagent");
        dir.push("log");
        let _ = fs::create_dir_all(&dir);
        Self {
            log_dir: dir,
            log_file: None,
            index: 0,
        }
    }

    pub fn start_new_run(&mut self) {
        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let file = self.log_dir.join(format!("agent_run_{}.log", ts));
        self.index = 0;
        self.log_file = Some(file.clone());

        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&file)
            .unwrap();
        let header = format!(
            "{sep}\nAgent Run Log - {}\n{sep}\n\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            sep = "=".repeat(80)
        );
        let _ = f.write_all(header.as_bytes());
    }

    pub fn log_request(&mut self, payload: &serde_json::Value) {
        self.write("REQUEST", payload);
    }

    pub fn log_response(&mut self, payload: &serde_json::Value) {
        self.write("RESPONSE", payload);
    }

    pub fn log_tool_result(&mut self, payload: &serde_json::Value) {
        self.write("TOOL_RESULT", payload);
    }

    fn write(&mut self, kind: &str, payload: &serde_json::Value) {
        if let Some(path) = &self.log_file {
            self.index += 1;
            let mut f = OpenOptions::new().append(true).open(path).unwrap();
            let content = format!(
                "\n{sep}\n[{}] {}\nTimestamp: {}\n{sep}\n{}\n",
                self.index,
                kind,
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                serde_json::to_string_pretty(payload).unwrap_or_default(),
                sep = "-".repeat(80),
            );
            let _ = f.write_all(content.as_bytes());
        }
    }

    pub fn log_path(&self) -> Option<&Path> {
        self.log_file.as_deref()
    }
}
