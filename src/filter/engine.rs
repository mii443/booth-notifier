use crate::{
    booth::item::{BoothItem, Tag},
    filter::{Field, Filter, FilterGroup, Rule, TagMode},
};

pub struct FilteringEngine {
    filter: Filter,
}

impl FilteringEngine {
    pub fn new(filter: Filter) -> Self {
        Self { filter }
    }

    pub fn check(&self, item: &BoothItem) -> bool {
        for group in &self.filter.groups {
            if !self.check_group(group, item) {
                return false;
            }
        }
        true
    }

    fn check_group(&self, group: &FilterGroup, item: &BoothItem) -> bool {
        for rule in &group.rules {
            if self.check_rule(rule, item) {
                return true;
            }
        }
        false
    }

    fn check_rule(&self, rule: &Rule, item: &BoothItem) -> bool {
        let matched = match rule.field {
            Field::Name | Field::Description => {
                let target = if rule.field == Field::Name {
                    &item.name
                } else {
                    &item.description
                };

                self.check_string_rule(rule, target)
            }
            Field::Tags => self.check_tags_rule(rule, &item.tags),
        };

        if rule.op == crate::filter::Op::Include {
            matched
        } else {
            !matched
        }
    }

    fn check_string_rule(&self, rule: &Rule, value: &str) -> bool {
        match &rule.pattern {
            crate::filter::Pattern::Text { value: pattern } => {
                if rule.case_sensitive {
                    value.contains(pattern)
                } else {
                    value.to_lowercase().contains(&pattern.to_lowercase())
                }
            }
            crate::filter::Pattern::Regex { value: pattern } => {
                let regex = if rule.case_sensitive {
                    regex::Regex::new(pattern)
                } else {
                    regex::Regex::new(&format!("(?i){pattern}"))
                };
                match regex {
                    Ok(re) => re.is_match(value),
                    Err(_) => false,
                }
            }
        }
    }

    fn check_tags_rule(&self, rule: &Rule, tags: &Vec<Tag>) -> bool {
        let tag_mode = rule.tag_mode.unwrap_or(TagMode::Any);

        if tags.is_empty() {
            return true;
        }

        if tag_mode == TagMode::Any {
            for tag in tags {
                if self.check_string_rule(rule, &tag.name) {
                    return true;
                }
            }
            false
        } else {
            for tag in tags {
                if !self.check_string_rule(rule, &tag.name) {
                    return false;
                }
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::booth::item::{BoothItem, Tag};
    use crate::filter::{Field, Filter, FilterGroup, Op, Pattern, Rule, TagMode};

    #[test]
    fn test_text_pattern() {
        let filter = Filter {
            groups: vec![FilterGroup {
                rules: vec![Rule {
                    field: Field::Name,
                    op: Op::Include,
                    pattern: Pattern::Text {
                        value: "test".to_string(),
                    },
                    case_sensitive: false,
                    regex_flags: None,
                    tag_mode: None,
                }],
            }],
            schema_version: 1,
        };

        let engine = FilteringEngine::new(filter);
        let mut item = BoothItem {
            name: String::new(),
            ..Default::default()
        };

        item.name = "test".to_string();
        assert!(engine.check(&item));

        item.name = "TEST".to_string();
        assert!(engine.check(&item));
    }

    #[test]
    fn test_text_pattern_case_sensitive() {
        let filter = Filter {
            groups: vec![FilterGroup {
                rules: vec![Rule {
                    field: Field::Name,
                    op: Op::Include,
                    pattern: Pattern::Text {
                        value: "test".to_string(),
                    },
                    case_sensitive: true,
                    regex_flags: None,
                    tag_mode: None,
                }],
            }],
            schema_version: 1,
        };

        let engine = FilteringEngine::new(filter);
        let mut item = BoothItem {
            name: String::new(),
            ..Default::default()
        };

        item.name = "test".to_string();
        assert!(engine.check(&item));

        item.name = "TEST".to_string();
        assert!(!engine.check(&item));
    }

    #[test]
    fn test_regex_pattern() {
        let filter = Filter {
            groups: vec![FilterGroup {
                rules: vec![Rule {
                    field: Field::Name,
                    op: Op::Include,
                    pattern: Pattern::Regex {
                        value: r"t.st".to_string(),
                    },
                    case_sensitive: false,
                    regex_flags: None,
                    tag_mode: None,
                }],
            }],
            schema_version: 1,
        };

        let engine = FilteringEngine::new(filter);
        let mut item = BoothItem {
            name: String::new(),
            ..Default::default()
        };

        item.name = "test".to_string();
        assert!(engine.check(&item));

        item.name = "tast".to_string();
        assert!(engine.check(&item));

        item.name = "tXst".to_string();
        assert!(engine.check(&item));

        item.name = "toast".to_string();
        assert!(!engine.check(&item));
    }

    #[test]
    fn test_regex_pattern_case_sensitive() {
        let filter = Filter {
            groups: vec![FilterGroup {
                rules: vec![Rule {
                    field: Field::Name,
                    op: Op::Include,
                    pattern: Pattern::Regex {
                        value: r"t.st".to_string(),
                    },
                    case_sensitive: true,
                    regex_flags: None,
                    tag_mode: None,
                }],
            }],
            schema_version: 1,
        };

        let engine = FilteringEngine::new(filter);
        let mut item = BoothItem {
            name: String::new(),
            ..Default::default()
        };

        item.name = "test".to_string();
        assert!(engine.check(&item));

        item.name = "Test".to_string();
        assert!(!engine.check(&item));

        item.name = "TaST".to_string();
        assert!(!engine.check(&item));

        item.name = "tXst".to_string();
        assert!(engine.check(&item));

        item.name = "toast".to_string();
        assert!(!engine.check(&item));
    }

    #[test]
    fn test_tag_mode_any() {
        let filter = Filter {
            groups: vec![FilterGroup {
                rules: vec![Rule {
                    field: Field::Tags,
                    op: Op::Include,
                    pattern: Pattern::Text {
                        value: "fuga".to_string(),
                    },
                    case_sensitive: false,
                    regex_flags: None,
                    tag_mode: Some(TagMode::Any),
                }],
            }],
            schema_version: 1,
        };

        let engine = FilteringEngine::new(filter);
        let mut item = BoothItem {
            tags: vec![],
            ..Default::default()
        };

        item.tags = vec![
            Tag {
                name: "hoge".to_string(),
                ..Default::default()
            },
            Tag {
                name: "fuga".to_string(),
                ..Default::default()
            },
        ];

        assert!(engine.check(&item));
    }

    #[test]
    fn test_tag_mode_all() {
        let filter = Filter {
            groups: vec![FilterGroup {
                rules: vec![Rule {
                    field: Field::Tags,
                    op: Op::Include,
                    pattern: Pattern::Text {
                        value: "fuga".to_string(),
                    },
                    case_sensitive: false,
                    regex_flags: None,
                    tag_mode: Some(TagMode::All),
                }],
            }],
            schema_version: 1,
        };

        let engine = FilteringEngine::new(filter);
        let mut item = BoothItem {
            tags: vec![],
            ..Default::default()
        };

        item.tags = vec![
            Tag {
                name: "hoge".to_string(),
                ..Default::default()
            },
            Tag {
                name: "fuga".to_string(),
                ..Default::default()
            },
        ];

        assert!(!engine.check(&item));

        item.tags = vec![Tag {
            name: "fuga".to_string(),
            ..Default::default()
        }];

        assert!(engine.check(&item));
    }

    #[test]
    fn test_exclude_op() {
        let filter = Filter {
            groups: vec![FilterGroup {
                rules: vec![Rule {
                    field: Field::Name,
                    op: Op::Exclude,
                    pattern: Pattern::Text {
                        value: "test".to_string(),
                    },
                    case_sensitive: false,
                    regex_flags: None,
                    tag_mode: None,
                }],
            }],
            schema_version: 1,
        };

        let engine = FilteringEngine::new(filter);
        let mut item = BoothItem {
            name: String::new(),
            ..Default::default()
        };

        item.name = "example".to_string();
        assert!(engine.check(&item));

        item.name = "test".to_string();
        assert!(!engine.check(&item));
    }

    #[test]
    fn test_multiple_groups() {
        let filter = Filter {
            groups: vec![
                FilterGroup {
                    rules: vec![Rule {
                        field: Field::Name,
                        op: Op::Include,
                        pattern: Pattern::Text {
                            value: "test".to_string(),
                        },
                        case_sensitive: false,
                        regex_flags: None,
                        tag_mode: None,
                    }],
                },
                FilterGroup {
                    rules: vec![Rule {
                        field: Field::Description,
                        op: Op::Include,
                        pattern: Pattern::Text {
                            value: "example".to_string(),
                        },
                        case_sensitive: false,
                        regex_flags: None,
                        tag_mode: None,
                    }],
                },
            ],
            schema_version: 1,
        };

        let engine = FilteringEngine::new(filter);
        let mut item = BoothItem {
            name: String::new(),
            description: String::new(),
            ..Default::default()
        };

        item.name = "test".to_string();
        item.description = "example".to_string();
        assert!(engine.check(&item));

        item.name = "test".to_string();
        item.description = "sample".to_string();
        assert!(!engine.check(&item));

        item.name = "sample".to_string();
        item.description = "example".to_string();
        assert!(!engine.check(&item));

        item.name = "sample".to_string();
        item.description = "sample".to_string();
        assert!(!engine.check(&item));
    }

    #[test]
    fn test_multiple_rules() {
        let filter = Filter {
            groups: vec![FilterGroup {
                rules: vec![
                    Rule {
                        field: Field::Name,
                        op: Op::Include,
                        pattern: Pattern::Text {
                            value: "test".to_string(),
                        },
                        case_sensitive: false,
                        regex_flags: None,
                        tag_mode: None,
                    },
                    Rule {
                        field: Field::Name,
                        op: Op::Include,
                        pattern: Pattern::Text {
                            value: "example".to_string(),
                        },
                        case_sensitive: false,
                        regex_flags: None,
                        tag_mode: None,
                    },
                ],
            }],
            schema_version: 1,
        };

        let engine = FilteringEngine::new(filter);
        let mut item = BoothItem {
            name: String::new(),
            ..Default::default()
        };

        item.name = "test".to_string();
        assert!(engine.check(&item));

        item.name = "example".to_string();
        assert!(engine.check(&item));

        item.name = "hoge".to_string();
        assert!(!engine.check(&item));
    }
}
