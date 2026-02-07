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
    pub async fn index(&mut self) {
        let data = &self.mmap[..];
        let mut offsets = Vec::new();
        
        if !data.is_empty() {
            offsets.push(0);
        }

        for (i, &byte) in data.iter().enumerate() {
            if byte == b'\n' {
                if i + 1 < data.len() {
                    offsets.push(i + 1);
                }
            }
        }

        self.offsets = offsets;
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
    async fn test_indexing() {
        let mut file = NamedTempFile::new().unwrap();
        // Use raw strings or explicit escape sequences to ensure correct writing
        file.write_all(b"Line 1\nLine 2\r\nLine 3").unwrap();
        
        let path = file.path().to_path_buf();
        let mut indexer = Indexer::new(path).unwrap();
        indexer.index().await;

        assert_eq!(indexer.line_count(), 3);
        assert_eq!(indexer.get_line(0), Some("Line 1".to_string()));
        assert_eq!(indexer.get_line(1), Some("Line 2".to_string()));
        assert_eq!(indexer.get_line(2), Some("Line 3".to_string()));
    }
}