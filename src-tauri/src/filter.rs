use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFilter {
    pub id: String,
    pub pattern: String,
    pub is_include: bool, // true = inclusion, false = exclusion
    pub is_enabled: bool,
    pub color: Option<String>,
    pub text_color: Option<String>,
}

pub struct FilterEngine {
    filters: Vec<(LogFilter, Regex)>,
    search_regex: Option<Regex>,
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
        Ok(Self {
            filters: compiled,
            search_regex: None,
        })
    }

    pub fn set_search_query(&mut self, query: &str) -> Result<(), String> {
        if query.is_empty() {
            self.search_regex = None;
        } else {
            let re = Regex::new(&format!("(?i){}", query))
                .map_err(|e| format!("Invalid search regex: {}", e))?;
            self.search_regex = Some(re);
        }
        Ok(())
    }

    /// Returns true if the line passes all filter criteria.
    pub fn matches(&self, line: &str) -> bool {
        let include_filters: Vec<_> = self.filters.iter().filter(|(f, _)| f.is_include).collect();
        let exclude_filters: Vec<_> = self.filters.iter().filter(|(f, _)| !f.is_include).collect();

        // 1. Check Exclusions
        for (_, re) in exclude_filters {
            if re.is_match(line) {
                return false;
            }
        }

        // 2. Check Search Query
        if let Some(re) = &self.search_regex {
            if !re.is_match(line) {
                return false;
            }
        }

        // 3. Check Inclusions
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

    /// Returns the color of the first matching filter, if any.
    pub fn get_match_color(&self, line: &str) -> Option<String> {
        let include_filters: Vec<_> = self.filters.iter().filter(|(f, _)| f.is_include).collect();
        let exclude_filters: Vec<_> = self.filters.iter().filter(|(f, _)| !f.is_include).collect();

        // 1. Check Exclusions
        for (_, re) in exclude_filters {
            if re.is_match(line) {
                return None;
            }
        }

        // 2. Check Search Query
        if let Some(re) = &self.search_regex {
            if !re.is_match(line) {
                return None;
            }
        }

        // 3. Check Inclusions
        if include_filters.is_empty() {
            // Only return a color if there's actually a search or filters
            return self.search_regex.as_ref().map(|_| "#fa5feb".to_string());
        }

        for (f, re) in include_filters {
            if re.is_match(line) {
                return f.color.clone().or_else(|| Some("#fa5feb".to_string()));
            }
        }

        None
    }

    pub fn get_match_counts(&self, lines: &[String]) -> std::collections::HashMap<String, usize> {
        self.filters
            .par_iter()
            .map(|(f, re)| {
                let count = lines.iter().filter(|l| re.is_match(l)).count();
                (f.id.clone(), count)
            })
            .collect()
    }

    pub fn get_match_counts_from_offsets(
        &self,
        indexer: &crate::indexer::Indexer,
        offsets: &[usize],
    ) -> std::collections::HashMap<String, usize> {
        self.filters
            .par_iter()
            .map(|(f, re)| {
                let count = offsets
                    .iter()
                    .filter(|&&offset| {
                        let line_data = indexer.get_line_at_offset_raw(offset);
                        re.is_match(&line_data)
                    })
                    .count();
                (f.id.clone(), count)
            })
            .collect()
    }

    /// Multi-threaded batch filtering
    pub fn filter_indices(&self, lines: &[String]) -> Vec<usize> {
        lines
            .par_iter()
            .enumerate()
            .filter_map(
                |(idx, line)| {
                    if self.matches(line) {
                        Some(idx)
                    } else {
                        None
                    }
                },
            )
            .collect()
    }
}
