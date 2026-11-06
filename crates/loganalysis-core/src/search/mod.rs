pub mod fuzzy;
pub mod searcher;

pub use fuzzy::{fuzzy_match, fuzzy_match_detailed, FuzzyMatchResult};
pub use searcher::{search_in_log, SearchQuery, SearchResult};
