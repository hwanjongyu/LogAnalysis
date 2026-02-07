use serde::{Deserialize, Serialize};
use regex::Regex;
use rayon::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFilter {
    pub id: String,
    pub pattern: String,
    pub is_include: bool, // true = inclusion, false = exclusion
    pub is_enabled: bool,
    pub color: Option<String>,
}

pub struct FilterEngine {
    filters: Vec<(LogFilter, Regex)>,
}

impl FilterEngine {
    pub fn new(filters: Vec<LogFilter>) -> Result<Self, String> {
        let mut compiled = Vec::new();
        for f in filters {
            if f.is_enabled && !f.pattern.is_empty() {
                let re = Regex::new(&f.pattern).map_err(|e| format!("Invalid regex: {}", e))?;
                compiled.push((f, re));
            }
        }
        Ok(Self { filters: compiled })
    }

    /// Returns true if the line passes all filter criteria.
    /// Inclusion logic: Must match AT LEAST ONE "Include" filter (if any exist).
    /// Exclusion logic: Must match ZERO "Exclude" filters.
    pub fn matches(&self, line: &str) -> bool {
        if self.filters.is_empty() {
            return true;
        }

        let include_filters: Vec<_> = self.filters.iter().filter(|(f, _)| f.is_include).collect();
        let exclude_filters: Vec<_> = self.filters.iter().filter(|(f, _)| !f.is_include).collect();

        // 1. Check Exclusions (Fail fast)
        for (_, re) in exclude_filters {
            if re.is_match(line) {
                return false;
            }
        }

        // 2. Check Inclusions
        if include_filters.is_empty() {
            return true;
        }

        for (_, re) in include_filters {
            if re.is_match(line) {
                return true;
            }
        }

        false
    }

    /// Multi-threaded batch filtering
    pub fn filter_indices(&self, lines: &[String]) -> Vec<usize> {
        lines
            .par_iter()
            .enumerate()
            .filter_map(|(idx, line)| {
                if self.matches(line) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }
}
