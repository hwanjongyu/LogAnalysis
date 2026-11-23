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

/// Ensures every [`FilterRule::id`] across `tabs` is unique by regenerating
/// duplicate UUIDs while preserving the first occurrence.
pub fn deduplicate_filter_tab_ids(tabs: &mut [FilterTabData]) {
    let mut seen = std::collections::HashSet::new();
    for tab in tabs {
        for rule in &mut tab.filters {
            if !seen.insert(rule.id) {
                rule.id = Uuid::new_v4();
                seen.insert(rule.id);
            }
        }
    }
}

/// Parses a JSON string containing filter data according to the 3 supported formats:
/// 1. Single tab object: `{"name": "...", "enabled": true, "filters": [...]}`
/// 2. Array of tab objects: `[{"name": "...", "filters": [...]}, ...]`
/// 3. Array of raw filter rules: `[{...rule...}, ...]`
pub fn parse_filters_json(json_str: &str) -> Result<Vec<FilterTabData>, serde_json::Error> {
    let val: serde_json::Value = serde_json::from_str(json_str)?;
    let mut tabs = match val {
        serde_json::Value::Object(_) => {
            let tab: FilterTabData = serde_json::from_value(val)?;
            vec![tab]
        }
        serde_json::Value::Array(arr) => {
            if arr.iter().any(|v| {
                v.get("filters").is_some()
                    || (v.get("name").is_some() && v.get("text").is_none())
            }) {
                serde_json::from_value(serde_json::Value::Array(arr))?
            } else {
                let filters: Vec<FilterRule> =
                    serde_json::from_value(serde_json::Value::Array(arr))?;
                vec![FilterTabData {
                    name: "Imported Filters".to_string(),
                    enabled: true,
                    filters,
                    modified: false,
                    file_path: None,
                }]
            }
        }
        _ => return Err(serde::de::Error::custom("Expected JSON object or array")),
    };
    deduplicate_filter_tab_ids(&mut tabs);
    Ok(tabs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_deduplicate_filter_tab_ids_preserves_unique_and_regenerates_duplicates() {
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();
        let mut tabs = vec![
            FilterTabData {
                name: "Tab 1".into(),
                filters: vec![
                    FilterRule {
                        id: id_a,
                        text: "first".into(),
                        ..Default::default()
                    },
                    FilterRule {
                        id: id_a,
                        text: "dup in same tab".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            FilterTabData {
                name: "Tab 2".into(),
                filters: vec![
                    FilterRule {
                        id: id_b,
                        text: "unique second".into(),
                        ..Default::default()
                    },
                    FilterRule {
                        id: id_a,
                        text: "dup across tabs".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        ];

        deduplicate_filter_tab_ids(&mut tabs);

        assert_eq!(tabs[0].filters[0].id, id_a);
        assert_ne!(tabs[0].filters[1].id, id_a);
        assert_eq!(tabs[1].filters[0].id, id_b);
        assert_ne!(tabs[1].filters[1].id, id_a);

        let unique: HashSet<Uuid> = tabs
            .iter()
            .flat_map(|t| t.filters.iter().map(|r| r.id))
            .collect();
        assert_eq!(unique.len(), 4);
    }

    #[test]
    fn test_parse_filters_json_deduplicates_ids_across_all_formats() {
        let dup_id = "11111111-1111-4111-8111-111111111111";

        // Format 1: single tab object
        let json_single = format!(
            r#"{{"name":"Single","filters":[{{"id":"{dup_id}","text":"a"}},{{"id":"{dup_id}","text":"b"}}]}}"#
        );
        let tabs_single = parse_filters_json(&json_single).unwrap();
        assert_eq!(tabs_single[0].filters.len(), 2);
        assert_eq!(tabs_single[0].filters[0].id.to_string(), dup_id);
        assert_ne!(
            tabs_single[0].filters[0].id,
            tabs_single[0].filters[1].id
        );

        // Format 2: array of tab objects
        let json_multi = format!(
            r#"[{{"name":"T1","filters":[{{"id":"{dup_id}","text":"a"}}]}},{{"name":"T2","filters":[{{"id":"{dup_id}","text":"b"}}]}}]"#
        );
        let tabs_multi = parse_filters_json(&json_multi).unwrap();
        assert_eq!(tabs_multi[0].filters[0].id.to_string(), dup_id);
        assert_ne!(tabs_multi[0].filters[0].id, tabs_multi[1].filters[0].id);

        // Format 3: array of raw filter rules
        let json_rules = format!(
            r#"[{{"id":"{dup_id}","text":"a"}},{{"id":"{dup_id}","text":"b"}}]"#
        );
        let tabs_rules = parse_filters_json(&json_rules).unwrap();
        assert_eq!(tabs_rules[0].filters[0].id.to_string(), dup_id);
        assert_ne!(tabs_rules[0].filters[0].id, tabs_rules[0].filters[1].id);
    }

    #[test]
    fn test_parse_filters_json_handles_tab_array_without_leading_filters_field() {
        let json_empty_first = r#"[{"name": "Empty Tab"}, {"name": "Errors", "filters": [{"text": "ERR"}]}]"#;
        let tabs = parse_filters_json(json_empty_first).unwrap();
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].name, "Empty Tab");
        assert!(tabs[0].filters.is_empty());
        assert_eq!(tabs[1].name, "Errors");
        assert_eq!(tabs[1].filters.len(), 1);
        assert_eq!(tabs[1].filters[0].text, "ERR");

        let json_only_name = r#"[{"name": "OnlyName"}]"#;
        let only_name_tabs = parse_filters_json(json_only_name).unwrap();
        assert_eq!(only_name_tabs.len(), 1);
        assert_eq!(only_name_tabs[0].name, "OnlyName");
        assert!(only_name_tabs[0].filters.is_empty());
    }
}
