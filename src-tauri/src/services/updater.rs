use semver::Version;
use serde::Serialize;
use std::time::Duration;

use super::outbound::{agent_builder_for_proxy, current_system_proxy_url, ProxyConfig};

const UPDATE_MANIFEST_URL: &str =
    "https://github.com/hardyz0517/relay-pool-desktop/releases/latest/download/latest.json";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterNetworkConfig {
    pub proxy_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishedVersionRelation {
    CurrentOrOlder,
    Newer,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedUpdateInspection {
    pub relation: PublishedVersionRelation,
    pub version: String,
    pub notes: Option<String>,
}

pub fn network_config() -> UpdaterNetworkConfig {
    UpdaterNetworkConfig {
        proxy_url: current_system_proxy_url(),
    }
}

pub fn inspect_latest_update_manifest(
    current_version: &str,
) -> Result<PublishedUpdateInspection, String> {
    let proxy = ProxyConfig {
        mode: "system".to_string(),
        url: None,
    };
    let agent = agent_builder_for_proxy(&proxy)?
        .timeout(Duration::from_secs(10))
        .build();
    let response = match agent
        .get(UPDATE_MANIFEST_URL)
        .set("Accept", "application/json")
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(status, _)) => {
            return Err(format!("Failed to read updater latest.json: HTTP {status}"));
        }
        Err(error) => {
            return Err(format!("Failed to read updater latest.json: {error}"));
        }
    };
    let body = response
        .into_string()
        .map_err(|error| format!("Failed to read updater latest.json body: {error}"))?;
    inspect_manifest_body(&body, current_version)
}

fn inspect_manifest_body(
    body: &str,
    current_version: &str,
) -> Result<PublishedUpdateInspection, String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|error| format!("Invalid updater manifest JSON: {error}"))?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Updater manifest does not contain a version".to_string())?;
    let published = Version::parse(normalize_version(version))
        .map_err(|error| format!("Invalid published updater version: {error}"))?;
    let current = Version::parse(normalize_version(current_version))
        .map_err(|error| format!("Invalid current application version: {error}"))?;

    Ok(PublishedUpdateInspection {
        relation: if published > current {
            PublishedVersionRelation::Newer
        } else {
            PublishedVersionRelation::CurrentOrOlder
        },
        version: version.to_string(),
        notes: value
            .get("notes")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|notes| !notes.is_empty())
            .map(str::to_string),
    })
}

fn normalize_version(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix('v')
        .or_else(|| value.strip_prefix('V'))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::{inspect_manifest_body, PublishedVersionRelation};

    #[test]
    fn classifies_equal_and_older_manifests_as_current_or_older() {
        assert_eq!(
            inspect_manifest_body(r#"{"version":"0.2.2","notes":""}"#, "0.2.2")
                .unwrap()
                .relation,
            PublishedVersionRelation::CurrentOrOlder,
        );
        assert_eq!(
            inspect_manifest_body(r#"{"version":"0.2.1"}"#, "0.2.2")
                .unwrap()
                .relation,
            PublishedVersionRelation::CurrentOrOlder,
        );
    }

    #[test]
    fn classifies_newer_and_prefixed_versions_with_semver_rules() {
        assert_eq!(
            inspect_manifest_body(r#"{"version":"v0.2.3"}"#, "V0.2.2")
                .unwrap()
                .relation,
            PublishedVersionRelation::Newer,
        );
        assert_eq!(
            inspect_manifest_body(r#"{"version":"0.2.3-beta.1"}"#, "0.2.3")
                .unwrap()
                .relation,
            PublishedVersionRelation::CurrentOrOlder,
        );
    }

    #[test]
    fn normalizes_empty_release_notes_to_none() {
        let inspection =
            inspect_manifest_body(r#"{"version":"0.2.3","notes":""}"#, "0.2.2").unwrap();

        assert_eq!(inspection.notes, None);
    }

    #[test]
    fn rejects_missing_or_invalid_manifest_versions() {
        assert!(inspect_manifest_body("{}", "0.2.2").is_err());
        assert!(inspect_manifest_body(r#"{"version":"not-semver"}"#, "0.2.2").is_err());
        assert!(inspect_manifest_body(r#"{"version":"0.2.3"}"#, "not-semver").is_err());
    }
}
