use crate::filter::FilterEngine;
use memmap2::Mmap;
use rayon::prelude::*;
use serde::Serialize;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Indexer {
    path: PathBuf,
    mmap: Arc<Mmap>,
    offsets: Vec<usize>,
}

#[derive(Serialize, Clone)]
pub struct MinimapBucket {
    pub intensity: f32,
    pub color: Option<String>,
    pub count: usize,
}

impl Indexer {
    pub fn new(path: PathBuf) -> Result<Self, String> {
        let file = File::open(&path).map_err(|e| format!("Failed to open file: {}", e))?;
        let mmap = unsafe { Mmap::map(&file).map_err(|e| format!("Failed to map file: {}", e))? };

        Ok(Self {
            path,
            mmap: Arc::new(mmap),
            offsets: Vec::new(),
        })
    }

    pub async fn index<F>(&mut self, mut progress_callback: F)
    where
        F: FnMut(f64),
    {
        let data = &self.mmap[..];
        let mut offsets = Vec::new();
        let total_size = data.len();

        if total_size == 0 {
            self.offsets = offsets;
            progress_callback(1.0);
            return;
        }

        offsets.push(0);
        let chunk_size = 1_048_576;
        let mut last_reported_pos = 0;

        for (i, &byte) in data.iter().enumerate() {
            if byte == b'\n' {
                if i + 1 < total_size {
                    offsets.push(i + 1);
                }
            }

            if i - last_reported_pos >= chunk_size {
                progress_callback(i as f64 / total_size as f64);
                last_reported_pos = i;
                tokio::task::yield_now().await;
            }
        }

        self.offsets = offsets;
        progress_callback(1.0);
    }

    /// Filters the offsets based on a FilterEngine.
    /// Returns a new vector of offsets that pass the filters.
    pub fn apply_filters(&self, engine: &FilterEngine) -> Vec<usize> {
        self.offsets
            .par_iter()
            .filter_map(|&offset| {
                let line_data = self.get_line_at_offset(offset);
                if engine.matches(&line_data) {
                    Some(offset)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_line(&self, index: usize) -> Option<String> {
        self.get_line_from_offsets(&self.offsets, index)
    }

    pub fn get_line_from_offsets(&self, custom_offsets: &[usize], index: usize) -> Option<String> {
        if index >= custom_offsets.len() {
            return None;
        }

        let start = custom_offsets[index];
        // Find end offset
        let end = if index + 1 < custom_offsets.len() {
            // This is only true if we are using original contiguous offsets.
            // For filtered offsets, we need to find the newline character.
            self.find_newline(start)
        } else {
            // Last line
            self.find_newline(start)
        };

        let line_data = &self.mmap[start..end];
        let line = String::from_utf8_lossy(line_data).into_owned();

        Some(line.trim_end_matches(['\r', '\n']).to_string())
    }

    /// Optimized helper to get line content starting at offset
    fn get_line_at_offset(&self, offset: usize) -> String {
        let end = self.find_newline(offset);
        let line_data = &self.mmap[offset..end];
        String::from_utf8_lossy(line_data).into_owned()
    }

    pub fn get_line_at_offset_raw(&self, offset: usize) -> String {
        let end = self.find_newline(offset);
        let line_data = &self.mmap[offset..end];
        String::from_utf8_lossy(line_data).into_owned()
    }

    fn find_newline(&self, start: usize) -> usize {
        let data = &self.mmap[start..];
        match data.iter().position(|&b| b == b'\n') {
            Some(pos) => start + pos + 1,
            None => self.mmap.len(),
        }
    }

    pub fn line_count(&self) -> usize {
        self.offsets.len()
    }

    pub fn get_minimap_data(
        &self,
        engine: &FilterEngine,
        buckets: usize,
        custom_offsets: Option<&[usize]>,
    ) -> Vec<MinimapBucket> {
        let active_offsets = custom_offsets.unwrap_or(&self.offsets);

        if active_offsets.is_empty() || buckets == 0 {
            return Vec::new();
        }

        let total_lines = active_offsets.len();
        let bucket_size = (total_lines as f64 / buckets as f64).max(1.0);

        (0..buckets)
            .into_par_iter()
            .map(|b| {
                let start_idx = (b as f64 * bucket_size) as usize;
                let end_idx = ((b + 1) as f64 * bucket_size) as usize;
                let end_idx = std::cmp::min(end_idx, total_lines);

                if start_idx >= end_idx {
                    return MinimapBucket {
                        intensity: 0.0,
                        color: None,
                        count: 0,
                    };
                }

                let mut matched_count = 0;
                let mut first_color = None;

                for i in start_idx..end_idx {
                    let offset = active_offsets[i];
                    let line_data = self.get_line_at_offset(offset);
                    if let Some(color) = engine.get_match_color(&line_data) {
                        matched_count += 1;
                        if first_color.is_none() {
                            first_color = Some(color);
                        }
                    }
                }

                MinimapBucket {
                    intensity: matched_count as f32 / (end_idx - start_idx) as f32,
                    color: first_color,
                    count: matched_count,
                }
            })
            .collect()
    }

    pub fn file_path(&self) -> &PathBuf {
        &self.path
    }
}
