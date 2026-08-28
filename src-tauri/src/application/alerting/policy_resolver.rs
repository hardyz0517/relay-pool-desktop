use sha2::{Digest, Sha256};

use crate::models::alerting::{resolve_policy, AlertPolicy, PolicyMatchContext, Severity};

/// The result of policy resolution is immutable for the lifetime of one
/// observation/episode.  The fingerprint is persisted with the incident so a
/// later policy edit cannot silently reinterpret old observations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedPolicy {
    pub policy: AlertPolicy,
    pub effective_severity: Severity,
    pub fingerprint: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PolicyResolver;

impl PolicyResolver {
    pub(crate) fn resolve(
        &self,
        policies: &[AlertPolicy],
        context: &PolicyMatchContext<'_>,
    ) -> ResolvedPolicy {
        let policy = resolve_policy(policies.iter(), context);
        self.resolve_with_policy(policy, context.base_severity)
    }

    pub(crate) fn resolve_with_policy(
        &self,
        policy: AlertPolicy,
        base_severity: Severity,
    ) -> ResolvedPolicy {
        let effective_severity = policy.effective_severity(base_severity);
        let fingerprint = policy_fingerprint(&policy);
        ResolvedPolicy {
            policy,
            effective_severity,
            fingerprint,
        }
    }
}

pub(crate) fn policy_fingerprint(policy: &AlertPolicy) -> String {
    let encoded = serde_json::to_vec(policy).unwrap_or_default();
    let digest = Sha256::digest(encoded);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::alerting::{AlertEventType, ConditionKey};

    #[test]
    fn resolver_returns_stable_fallback_and_fingerprint() {
        let context = PolicyMatchContext {
            event_type: AlertEventType::StationDown,
            base_severity: Severity::Warning,
            station_id: None,
            station_key_id: None,
        };
        let resolver = PolicyResolver;
        let first = resolver.resolve(&[], &context);
        let second = resolver.resolve(&[], &context);
        assert_eq!(first.policy.id, "system_default");
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(first.effective_severity, Severity::Warning);
        let _ = ConditionKey::new("station:1").expect("fixture key");
    }
}
