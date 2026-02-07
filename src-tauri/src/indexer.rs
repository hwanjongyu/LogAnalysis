use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use memmap2::Mmap;
use crate::filter::FilterEngine;
use rayon::prelude::*;

pub struct Indexer {
    path: PathBuf,
    mmap: Arc<Mmap>,
    offsets: Vec<usize>,
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
        F: FnMut(f64)
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
        self.offsets.par_iter().filter_map(|&offset| {
            let line_data = self.get_line_at_offset(offset);
            if engine.matches(&line_data) {
                Some(offset)
            } else {
                None
            }
        }).collect()
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

    pub fn file_path(&self) -> &PathBuf {
        &self.path
    }
}