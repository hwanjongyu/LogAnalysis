pub mod indexer;

use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{State, AppHandle, Emitter};
use std::path::PathBuf;
use serde::Serialize;

pub struct AppState {
    pub indexer: Arc<Mutex<Option<indexer::Indexer>>>,
}

#[derive(Clone, Serialize)]
struct IndexProgress {
    progress: f64,
}

#[tauri::command]
async fn open_file(
    path: String, 
    app: AppHandle,
    state: State<'_, AppState>
) -> Result<usize, String> {
    let mut indexer = indexer::Indexer::new(PathBuf::from(path))?;
    
    // We can use a move closure to capture the app handle and emit events
    indexer.index(|p| {
        let _ = app.emit("indexing-progress", IndexProgress { progress: p });
    }).await;
    
    let line_count = indexer.line_count();
    let mut state_indexer = state.indexer.lock().await;
    *state_indexer = Some(indexer);
    
    Ok(line_count)
}

#[tauri::command]
async fn get_log_lines(start_index: usize, count: usize, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let indexer_lock = state.indexer.lock().await;
    let indexer = indexer_lock.as_ref().ok_or("No file opened")?;
    
    let mut lines = Vec::new();
    for i in start_index..(start_index + count) {
        if let Some(line) = indexer.get_line(i) {
            lines.push(line);
        } else {
            break;
        }
    }
    
    Ok(lines)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            indexer: Arc::new(Mutex::new(None)),
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![open_file, get_log_lines])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
