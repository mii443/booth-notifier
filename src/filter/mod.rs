pub mod engine;

pub use engine::*;

use serde::{
    Deserialize, Serialize,
    de::{self, Visitor},
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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
    Category,
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
    Text {
        #[serde(deserialize_with = "deserialize_string_value")]
        value: String,
    },
    Regex {
        #[serde(deserialize_with = "deserialize_string_value")]
        value: String,
    },
}

fn deserialize_string_value<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringValueVisitor;

    impl<'de> Visitor<'de> for StringValueVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a string, number, boolean, or null")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
            Ok(value.to_string())
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
            Ok(value.to_string())
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(value.to_string())
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
            Ok(value.to_string())
        }

        fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
            Ok(value.to_string())
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(String::new())
        }
    }

    deserializer.deserialize_any(StringValueVisitor)
}

fn default_schema_version() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_pattern_values_deserialize_as_strings() {
        let filter: Filter = serde_yaml::from_str(
            r#"
groups:
- rules:
  - field: description
    op: include
    pattern:
      type: text
      value: 3470989
    case_sensitive: false
schema_version: 1
"#,
        )
        .unwrap();

        let Pattern::Text { value } = &filter.groups[0].rules[0].pattern else {
            panic!("expected text pattern");
        };

        assert_eq!(value, "3470989");
    }
}
