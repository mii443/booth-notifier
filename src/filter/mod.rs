pub mod engine;

pub use engine::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    #[serde(default)]
    pub groups: Vec<FilterGroup>,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterGroup {
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub field: Field,
    pub op: Op,
    pub pattern: Pattern,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex_flags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_mode: Option<TagMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Tags,
    Name,
    Description,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Include, // has
    Exclude, // not has
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TagMode {
    Any,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Pattern {
    Text { value: String },
    Regex { value: String },
}

fn default_schema_version() -> u32 {
    1
}
