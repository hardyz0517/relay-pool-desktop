use sha2::{Digest, Sha256};

use crate::models::alerting::{AlertEventType, ConditionKey};

#[derive(Debug, Default, Clone, Copy)]
#[expect(
    dead_code,
    reason = "contract=alerting.condition-key-factory; owner=application/alerting; remove_when=all producer adapters construct keys through the shared factory"
)]
pub(crate) struct ConditionKeyFactory;

impl ConditionKeyFactory {
    #[expect(
        dead_code,
        reason = "contract=alerting.condition-key-object; owner=application/alerting; remove_when=all producer adapters construct keys through the shared factory"
    )]
    pub(crate) fn for_object(
        &self,
        event_type: AlertEventType,
        object_type: &str,
        object_id: Option<&str>,
    ) -> Result<ConditionKey, String> {
        let mut hasher = Sha256::new();
        hasher.update(object_type.as_bytes());
        hasher.update([0]);
        hasher.update(object_id.unwrap_or_default().as_bytes());
        let digest = hasher.finalize();
        let digest = digest
            .iter()
            .take(16)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        ConditionKey::new(format!("event:{}:object:{}", event_type.as_str(), digest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_keys_are_stable_and_do_not_include_object_values() {
        let factory = ConditionKeyFactory;
        let left = factory
            .for_object(
                AlertEventType::StationDown,
                "station",
                Some("secret-ish-id"),
            )
            .unwrap();
        let right = factory
            .for_object(
                AlertEventType::StationDown,
                "station",
                Some("secret-ish-id"),
            )
            .unwrap();
        assert_eq!(left, right);
        assert!(!left.as_str().contains("secret-ish-id"));
    }
}
