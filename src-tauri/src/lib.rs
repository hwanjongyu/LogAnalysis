pub mod indexer;
pub mod filter;

use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{State, AppHandle, Emitter};
use std::path::PathBuf;
use serde::Serialize;
use crate::filter::{LogFilter, FilterEngine};

pub struct AppState {
    pub indexer: Arc<Mutex<Option<indexer::Indexer>>>,
    pub filtered_offsets: Arc<Mutex<Option<Vec<usize>>>>,
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
    
    indexer.index(|p| {
        let _ = app.emit("indexing-progress", IndexProgress { progress: p });
    }).await;
    
    let line_count = indexer.line_count();
    
    // Reset filtered offsets on new file
    let mut filtered_offsets = state.filtered_offsets.lock().await;
    *filtered_offsets = None;

    let mut state_indexer = state.indexer.lock().await;
    *state_indexer = Some(indexer);
    
    Ok(line_count)
}

#[tauri::command]
async fn apply_filters(filters: Vec<LogFilter>, state: State<'_, AppState>) -> Result<usize, String> {
    let indexer_lock = state.indexer.lock().await;
    let indexer = indexer_lock.as_ref().ok_or("No file opened")?;
    
    if filters.is_empty() || filters.iter().all(|f| !f.is_enabled || f.pattern.is_empty()) {
        let mut filtered_offsets = state.filtered_offsets.lock().await;
        *filtered_offsets = None;
        return Ok(indexer.line_count());
    }

    let engine = FilterEngine::new(filters)?;
    let new_offsets = indexer.apply_filters(&engine);
    let count = new_offsets.len();

    let mut filtered_offsets = state.filtered_offsets.lock().await;
    *filtered_offsets = Some(new_offsets);

    Ok(count)
}

#[tauri::command]
async fn get_log_lines(start_index: usize, count: usize, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let indexer_lock = state.indexer.lock().await;
    let indexer = indexer_lock.as_ref().ok_or("No file opened")?;
    
    let filtered_lock = state.filtered_offsets.lock().await;
    
    let mut lines = Vec::new();
    for i in start_index..(start_index + count) {
        let line = if let Some(offsets) = filtered_lock.as_ref() {
            indexer.get_line_from_offsets(offsets, i)
        } else {
            indexer.get_line(i)
        };

        if let Some(l) = line {
            lines.push(l);
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
            filtered_offsets: Arc::new(Mutex::new(None)),
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![open_file, get_log_lines, apply_filters])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}