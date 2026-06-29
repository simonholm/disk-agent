use crate::rules::{load_rules, Classification, Rule, UNKNOWN};

pub fn classify_path(path: &str, rules: Option<&[Rule]>) -> Classification {
    let loaded;
    let rules = match rules {
        Some(rules) => rules,
        None => {
            loaded = load_rules();
            &loaded
        }
    };

    for rule in rules {
        if path == rule.pattern || path.starts_with(&format!("{}/", rule.pattern)) {
            return Classification {
                path: path.to_string(),
                classification: rule.classification.clone(),
                risk: rule.risk.clone(),
                explanation: rule.explanation.clone(),
                recommendation: rule.recommendation.clone(),
                known: true,
            };
        }
    }

    Classification {
        path: path.to_string(),
        classification: UNKNOWN.classification.to_string(),
        risk: UNKNOWN.risk.to_string(),
        explanation: UNKNOWN.explanation.to_string(),
        recommendation: UNKNOWN.recommendation.to_string(),
        known: UNKNOWN.known,
    }
}

pub fn is_child_path(child: &str, parent: &str) -> bool {
    child.starts_with(&format!("{parent}/"))
}
