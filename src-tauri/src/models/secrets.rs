use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretScope {
    Station,
    StationKey,
    Collector,
    Proxy,
    Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretKind {
    ApiKey,
    LoginPassword,
    Token,
    Cookie,
    Session,
    Authorization,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretRef {
    pub id: String,
    pub scope: SecretScope,
    pub owner_id: String,
    pub kind: SecretKind,
    pub masked_value: String,
    pub encryption_version: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretMigrationReport {
    pub migrated_count: i64,
    pub skipped_count: i64,
    pub failed_count: i64,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretScanFinding {
    pub table_name: String,
    pub column_name: String,
    pub evidence: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_ref_serializes_camel_case() {
        let value = serde_json::to_value(SecretRef {
            id: "secret-1".to_string(),
            scope: SecretScope::StationKey,
            owner_id: "key-1".to_string(),
            kind: SecretKind::ApiKey,
            masked_value: "sk-...abcd".to_string(),
            encryption_version: 1,
            updated_at: "1000".to_string(),
        })
        .expect("json");

        assert_eq!(value["ownerId"], "key-1");
        assert_eq!(value["maskedValue"], "sk-...abcd");
        assert_eq!(value["encryptionVersion"], 1);
    }
}
