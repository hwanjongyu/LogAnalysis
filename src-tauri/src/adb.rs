use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tauri::{AppHandle, Emitter};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::filter::FilterEngine;

#[derive(Clone, Serialize)]
pub struct AdbLine {
    pub content: String,
    pub timestamp: u64,
}

pub struct AdbManager {
    child: Option<tokio::process::Child>,
}

impl AdbManager {
    pub fn new() -> Self {
        Self { child: None }
    }

    pub async fn start_streaming(
        &mut self,
        app: AppHandle,
        filters: Option<FilterEngine>,
        lines_buffer: Arc<Mutex<Vec<String>>>,
    ) -> Result<(), String> {
        self.stop();

        let mut child = Command::new("adb")
            .arg("logcat")
            .arg("-v")
            .arg("time")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start adb: {}", e))?;

        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        self.child = Some(child);

        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let mut batch = Vec::new();
            let mut last_emit = std::time::Instant::now();

            while let Ok(Some(line)) = reader.next_line().await {
                let should_add = if let Some(ref engine) = filters {
                    engine.matches(&line)
                } else {
                    true
                };

                if should_add {
                    let mut buffer = lines_buffer.lock().await;
                    buffer.push(line.clone());
                    batch.push(line);
                }

                // Batch emissions every 100ms or 100 lines to reduce IPC overhead
                if batch.len() >= 100 || (last_emit.elapsed().as_millis() >= 100 && !batch.is_empty()) {
                    let _ = app.emit("adb-new-lines", &batch);
                    batch.clear();
                    last_emit = std::time::Instant::now();
                }
            }
        });

        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
    }
}
