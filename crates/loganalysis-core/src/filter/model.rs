use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::constants::parse_color_spec;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilterRule {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub exclude: bool,
    #[serde(default = "default_none_str")]
    pub bg_color: String,
    #[serde(default = "default_none_str")]
    pub text_color: String,
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default)]
    pub description: String,
}

fn default_none_str() -> String {
    "None".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for FilterRule {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            text: String::new(),
            case_sensitive: false,
            regex: false,
            exclude: false,
            bg_color: "None".to_string(),
            text_color: "None".to_string(),
            active: true,
            description: String::new(),
        }
    }
}

impl FilterRule {
    pub fn resolved_bg_hex(&self) -> Option<u32> {
        parse_color_spec(&self.bg_color, true)
    }

    pub fn resolved_fg_hex(&self) -> Option<u32> {
        parse_color_spec(&self.text_color, false)
    }

    pub fn describe_prefix(&self) -> String {
        let mut prefix = String::new();
        if self.exclude {
            prefix.push_str("NOT: ");
        }
        if self.regex {
            prefix.push_str("REGEX: ");
        }
        if self.case_sensitive {
            prefix.push_str("CASE: ");
        }
        prefix
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilterTabData {
    #[serde(default = "default_tab_name")]
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub filters: Vec<FilterRule>,
    #[serde(skip)]
    pub modified: bool,
    #[serde(skip)]
    pub file_path: Option<String>,
}

fn default_tab_name() -> String {
    "Filter 1".to_string()
}

impl Default for FilterTabData {
    fn default() -> Self {
        Self {
            name: default_tab_name(),
            enabled: true,
            filters: Vec::new(),
            modified: false,
            file_path: None,
        }
    }
}

/// Parses a JSON string containing filter data according to the 3 supported formats:
/// 1. Single tab object: `{"name": "...", "enabled": true, "filters": [...]}`
/// 2. Array of tab objects: `[{"name": "...", "filters": [...]}, ...]`
/// 3. Array of raw filter rules: `[{...rule...}, ...]`
pub fn parse_filters_json(json_str: &str) -> Result<Vec<FilterTabData>, serde_json::Error> {
    let val: serde_json::Value = serde_json::from_str(json_str)?;
    match val {
        serde_json::Value::Object(_) => {
            let tab: FilterTabData = serde_json::from_value(val)?;
            Ok(vec![tab])
        }
        serde_json::Value::Array(arr) => {
            if let Some(first) = arr.first() {
                if first.get("filters").is_some() {
                    let tabs: Vec<FilterTabData> = serde_json::from_value(serde_json::Value::Array(arr))?;
                    return Ok(tabs);
                }
            }
            let filters: Vec<FilterRule> = serde_json::from_value(serde_json::Value::Array(arr))?;
            Ok(vec![FilterTabData {
                name: "Imported Filters".to_string(),
                enabled: true,
                filters,
                modified: false,
                file_path: None,
            }])
        }
        _ => Err(serde::de::Error::custom("Expected JSON object or array")),
    }
}
