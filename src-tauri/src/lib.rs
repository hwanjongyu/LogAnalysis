pub mod indexer;
pub mod filter;
pub mod adb;

use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{State, AppHandle, Emitter};
use std::path::PathBuf;
use serde::Serialize;
use crate::filter::{LogFilter, FilterEngine};
use crate::adb::AdbManager;

pub struct AppState {
    pub indexer: Arc<Mutex<Option<indexer::Indexer>>>,
    pub filtered_offsets: Arc<Mutex<Option<Vec<usize>>>>,
    pub adb_manager: Arc<Mutex<AdbManager>>,
    pub adb_buffer: Arc<Mutex<Vec<String>>>,
    pub is_adb_active: Arc<Mutex<bool>>,
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
    // Stop ADB if active when opening a file
    let mut adb_active = state.is_adb_active.lock().await;
    if *adb_active {
        let mut adb_manager = state.adb_manager.lock().await;
        adb_manager.stop();
        *adb_active = false;
    }

    let mut indexer = indexer::Indexer::new(PathBuf::from(path))?;
    
    indexer.index(|p| {
        let _ = app.emit("indexing-progress", IndexProgress { progress: p });
    }).await;
    
    let line_count = indexer.line_count();
    
    let mut filtered_offsets = state.filtered_offsets.lock().await;
    *filtered_offsets = None;

    let mut state_indexer = state.indexer.lock().await;
    *state_indexer = Some(indexer);
    
    Ok(line_count)
}

#[tauri::command]
async fn apply_filters(filters: Vec<LogFilter>, state: State<'_, AppState>) -> Result<usize, String> {
    let adb_active = state.is_adb_active.lock().await;
    
    if *adb_active {
        // For ADB, we don't have static offsets, filters are applied at ingestion.
        // But if user changes filters, we might want to clear buffer or re-apply?
        // For MVP, we just return the current buffer size.
        let buffer = state.adb_buffer.lock().await;
        return Ok(buffer.len());
    }

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
    let adb_active = state.is_adb_active.lock().await;
    if *adb_active {
        let buffer = state.adb_buffer.lock().await;
        let end = std::cmp::min(start_index + count, buffer.len());
        if start_index >= buffer.len() {
            return Ok(Vec::new());
        }
        return Ok(buffer[start_index..end].to_vec());
    }

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

#[tauri::command]
async fn start_adb(filters: Vec<LogFilter>, app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut adb_active = state.is_adb_active.lock().await;
    let mut buffer = state.adb_buffer.lock().await;
    let mut adb_manager = state.adb_manager.lock().await;

    // Reset buffer for new stream
    buffer.clear();
    
    let engine = if filters.is_empty() {
        None
    } else {
        Some(FilterEngine::new(filters)?)
    };

    adb_manager.start_streaming(app, engine, Arc::clone(&state.adb_buffer)).await?;
    *adb_active = true;
    
    Ok(())
}

#[tauri::command]
async fn stop_adb(state: State<'_, AppState>) -> Result<(), String> {
    let mut adb_active = state.is_adb_active.lock().await;
    let mut adb_manager = state.adb_manager.lock().await;
    
    adb_manager.stop();
    *adb_active = false;
    
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            indexer: Arc::new(Mutex::new(None)),
            filtered_offsets: Arc::new(Mutex::new(None)),
            adb_manager: Arc::new(Mutex::new(AdbManager::new())),
            adb_buffer: Arc::new(Mutex::new(Vec::new())),
            is_adb_active: Arc::new(Mutex::new(false)),
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_file, 
            get_log_lines, 
            apply_filters,
            start_adb,
            stop_adb
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
