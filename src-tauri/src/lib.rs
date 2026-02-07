pub mod adb;
pub mod filter;
pub mod indexer;

use crate::adb::AdbManager;
use crate::filter::{FilterEngine, LogFilter};
use rayon::prelude::*;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

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

#[derive(Clone, Serialize)]
struct FilterResult {
    visible_count: usize,
    filter_counts: std::collections::HashMap<String, usize>,
}

#[tauri::command]
async fn open_file(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    // Stop ADB if active when opening a file
    let mut adb_active = state.is_adb_active.lock().await;
    if *adb_active {
        let mut adb_manager = state.adb_manager.lock().await;
        adb_manager.stop();
        *adb_active = false;
    }

    let mut indexer = indexer::Indexer::new(PathBuf::from(path))?;

    indexer
        .index(|p| {
            let _ = app.emit("indexing-progress", IndexProgress { progress: p });
        })
        .await;

    let line_count = indexer.line_count();

    let mut filtered_offsets = state.filtered_offsets.lock().await;
    *filtered_offsets = None;

    let mut state_indexer = state.indexer.lock().await;
    *state_indexer = Some(indexer);

    Ok(line_count)
}

#[tauri::command]
async fn apply_filters(
    filters: Vec<LogFilter>,
    search_query: String,
    state: State<'_, AppState>,
) -> Result<FilterResult, String> {
    let adb_active = state.is_adb_active.lock().await;

    if *adb_active {
        // For ADB, we don't have static offsets, filters are applied at ingestion.
        let buffer = state.adb_buffer.lock().await;
        let mut engine = FilterEngine::new(filters)?;
        engine.set_search_query(&search_query)?;

        let counts = engine.get_match_counts(&buffer);
        return Ok(FilterResult {
            visible_count: buffer.len(),
            filter_counts: counts,
        });
    }

    let indexer_lock = state.indexer.lock().await;
    let indexer = indexer_lock.as_ref().ok_or("No file opened")?;

    let has_search = !search_query.is_empty();
    let has_filters = !filters.is_empty()
        && filters
            .iter()
            .any(|f| f.is_enabled && !f.pattern.is_empty());

    if !has_search && !has_filters {
        let mut filtered_offsets = state.filtered_offsets.lock().await;
        *filtered_offsets = None;
        return Ok(FilterResult {
            visible_count: indexer.line_count(),
            filter_counts: std::collections::HashMap::new(),
        });
    }

    let mut engine = FilterEngine::new(filters)?;
    if has_search {
        engine.set_search_query(&search_query)?;
    }

    let new_offsets = indexer.apply_filters(&engine);
    let count = new_offsets.len();

    let counts = engine.get_match_counts_from_offsets(indexer, &new_offsets);

    let mut filtered_offsets = state.filtered_offsets.lock().await;
    *filtered_offsets = Some(new_offsets);

    Ok(FilterResult {
        visible_count: count,
        filter_counts: counts,
    })
}

#[tauri::command]
async fn get_log_lines(
    start_index: usize,
    count: usize,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
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
async fn start_adb(
    filters: Vec<LogFilter>,
    search_query: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut adb_active = state.is_adb_active.lock().await;
    let mut buffer = state.adb_buffer.lock().await;
    let mut adb_manager = state.adb_manager.lock().await;

    // Reset buffer for new stream
    buffer.clear();

    let has_search = !search_query.is_empty();
    let has_filters = !filters.is_empty()
        && filters
            .iter()
            .any(|f| f.is_enabled && !f.pattern.is_empty());

    let engine = if !has_search && !has_filters {
        None
    } else {
        let mut eng = FilterEngine::new(filters)?;
        if has_search {
            eng.set_search_query(&search_query)?;
        }
        Some(eng)
    };

    adb_manager
        .start_streaming(app, engine, Arc::clone(&state.adb_buffer))
        .await?;
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

#[tauri::command]
async fn get_minimap_data(
    filters: Vec<LogFilter>,
    search_query: String,
    buckets: usize,
    state: State<'_, AppState>,
) -> Result<Vec<indexer::MinimapBucket>, String> {
    let adb_active = state.is_adb_active.lock().await;

    if *adb_active {
        // For ADB, we can use the current buffer
        let buffer = state.adb_buffer.lock().await;
        if buffer.is_empty() || buckets == 0 {
            return Ok(Vec::new());
        }

        let mut engine = FilterEngine::new(filters)?;
        engine.set_search_query(&search_query)?;

        let total_lines = buffer.len();
        let bucket_size = (total_lines as f64 / buckets as f64).max(1.0);

        let data = (0..buckets)
            .into_par_iter()
            .map(|b| {
                let start_idx = (b as f64 * bucket_size) as usize;
                let end_idx = ((b + 1) as f64 * bucket_size) as usize;
                let end_idx = std::cmp::min(end_idx, total_lines);

                if start_idx >= end_idx {
                    return indexer::MinimapBucket {
                        intensity: 0.0,
                        color: None,
                        count: 0,
                    };
                }

                let mut matched_count = 0;
                let mut first_color = None;

                for i in start_idx..end_idx {
                    if let Some(color) = engine.get_match_color(&buffer[i]) {
                        matched_count += 1;
                        if first_color.is_none() {
                            first_color = Some(color);
                        }
                    }
                }

                indexer::MinimapBucket {
                    intensity: matched_count as f32 / (end_idx - start_idx) as f32,
                    color: first_color,
                    count: matched_count,
                }
            })
            .collect();

        return Ok(data);
    }

    let indexer_lock = state.indexer.lock().await;
    let indexer = indexer_lock.as_ref().ok_or("No file opened")?;

    let filtered_lock = state.filtered_offsets.lock().await;

    let mut engine = FilterEngine::new(filters)?;
    engine.set_search_query(&search_query)?;

    Ok(indexer.get_minimap_data(&engine, buckets, filtered_lock.as_deref()))
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
            stop_adb,
            get_minimap_data
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
