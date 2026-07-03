#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub pattern: String,
    pub classification: String,
    pub category: String,
    pub risk: String,
    pub explanation: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub path: String,
    pub classification: String,
    pub category: String,
    pub risk: String,
    pub explanation: String,
    pub recommendation: String,
    pub known: bool,
}

pub static UNKNOWN: ClassificationStatic = ClassificationStatic {
    classification: "Unknown growth",
    category: "Unclassified",
    risk: "Unknown",
    explanation: "Growth occurred in unclassified locations.",
    recommendation: "Inspect this location before taking any cleanup action.",
    known: false,
};

pub struct ClassificationStatic {
    pub classification: &'static str,
    pub category: &'static str,
    pub risk: &'static str,
    pub explanation: &'static str,
    pub recommendation: &'static str,
    pub known: bool,
}

const RULE_FILES: &[&str] = &[
    include_str!("../rules/common.yaml"),
    include_str!("../rules/cargo.yaml"),
    include_str!("../rules/codex.yaml"),
    include_str!("../rules/copilot.yaml"),
    include_str!("../rules/npm.yaml"),
    include_str!("../rules/podman.yaml"),
    include_str!("../rules/repos.yaml"),
];

pub fn load_rules() -> Vec<Rule> {
    let mut rules = RULE_FILES
        .iter()
        .flat_map(|text| parse_rules(text))
        .collect::<Vec<_>>();
    rules.sort_by(|left, right| right.pattern.len().cmp(&left.pattern.len()));
    rules
}

fn parse_rules(text: &str) -> Vec<Rule> {
    text.split("\n---")
        .filter_map(|part| {
            let document = part.trim();
            if document.is_empty() {
                None
            } else {
                Some(parse_rule_document(document))
            }
        })
        .collect()
}

fn parse_rule_document(text: &str) -> Rule {
    let mut values = std::collections::HashMap::<String, String>::new();
    let mut current_key: Option<String> = None;
    let mut block_lines: Vec<String> = Vec::new();

    let finish_block = |values: &mut std::collections::HashMap<String, String>,
                        current_key: &mut Option<String>,
                        block_lines: &mut Vec<String>| {
        if let Some(key) = current_key.take() {
            let value = block_lines
                .iter()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
            values.insert(key, value);
        }
        block_lines.clear();
    };

    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if current_key.is_some() {
            if let Some(stripped) = line.strip_prefix("  ") {
                block_lines.push(stripped.to_string());
                continue;
            }
            finish_block(&mut values, &mut current_key, &mut block_lines);
        }

        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = value.trim();
        if value == "|" {
            current_key = Some(key);
        } else {
            values.insert(key, value.to_string());
        }
    }
    finish_block(&mut values, &mut current_key, &mut block_lines);

    let classification = values.remove("classification").unwrap_or_default();
    Rule {
        pattern: values.remove("pattern").unwrap_or_default(),
        category: values
            .remove("category")
            .unwrap_or_else(|| classification.clone()),
        classification,
        risk: values.remove("risk").unwrap_or_default(),
        explanation: values.remove("explanation").unwrap_or_default(),
        recommendation: values
            .remove("recommendation")
            .unwrap_or_else(|| "None.".to_string()),
    }
}
