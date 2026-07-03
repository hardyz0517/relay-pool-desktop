use crate::models::secrets::SecretScanFinding;

pub fn canary_patterns() -> Vec<&'static str> {
    vec![
        "sk-p8-secret-plaintext-canary",
        "p8-password-canary",
        "rpd_session=p8-cookie-canary",
        "Bearer sk-p8-secret",
        "token=p8-token-canary",
    ]
}

pub fn evidence_for_value(value: &str) -> String {
    let mut chars = value.chars();
    let preview: String = chars.by_ref().take(24).collect();
    if value.chars().count() > 24 {
        format!("{preview}...")
    } else {
        preview
    }
}

pub fn finding(table_name: &str, column_name: &str, value: &str) -> SecretScanFinding {
    SecretScanFinding {
        table_name: table_name.to_string(),
        column_name: column_name.to_string(),
        evidence: evidence_for_value(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_is_short() {
        let evidence = evidence_for_value("sk-p8-secret-plaintext-canary-extra");

        assert!(evidence.ends_with("..."));
        assert!(evidence.chars().count() <= 27);
    }
}
