use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use memmap2::Mmap;

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

    /// Indexes the file by finding the start offset of each line.
    /// Reports progress via the provided callback (0.0 to 1.0).
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
        
        // Report progress every 1MB or so to avoid callback overhead
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
                // Yield to the executor to keep it responsive
                tokio::task::yield_now().await;
            }
        }

        self.offsets = offsets;
        progress_callback(1.0);
    }

    pub fn get_line(&self, index: usize) -> Option<String> {
        if index >= self.offsets.len() {
            return None;
        }

        let start = self.offsets[index];
        let end = if index + 1 < self.offsets.len() {
            self.offsets[index + 1]
        } else {
            self.mmap.len()
        };

        let line_data = &self.mmap[start..end];
        let line = String::from_utf8_lossy(line_data).into_owned();
        
        Some(line.trim_end_matches(['\r', '\n']).to_string())
    }

    pub fn line_count(&self) -> usize {
        self.offsets.len()
    }

    pub fn file_path(&self) -> &PathBuf {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_indexing_with_progress() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"Line 1\nLine 2\r\nLine 3").unwrap();
        
        let path = file.path().to_path_buf();
        let mut indexer = Indexer::new(path).unwrap();
        
        let mut progress_values = Vec::new();
        indexer.index(|p| progress_values.push(p)).await;

        assert_eq!(indexer.line_count(), 3);
        assert!(progress_values.contains(&1.0));
    }
}
