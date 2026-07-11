use crate::services::proxy::scheduler::types::{
    EffectiveMultiplierFact, MultiplierRejectReason, MultiplierSourceFacts,
};

const ONE_HOUR_MS: i64 = 60 * 60 * 1000;

pub fn resolve_effective_multiplier(
    facts: MultiplierSourceFacts,
    now_ms: i64,
    min_confidence: f64,
    group_rate_interval_ms: i64,
) -> Result<EffectiveMultiplierFact, MultiplierRejectReason> {
    if let Some(value) = facts.manual_rate_multiplier {
        validate_multiplier_value(value)?;
        return Ok(EffectiveMultiplierFact {
            station_key_id: facts.station_key_id,
            value,
            source: "manual".to_string(),
            collected_at_ms: parse_optional_millis(facts.manual_rate_updated_at.as_deref()),
            valid_until_ms: None,
            confidence: 1.0,
            group_binding_id: facts.group_binding_id,
        });
    }

    let value = facts
        .collected_rate_multiplier
        .ok_or(MultiplierRejectReason::Missing)?;
    validate_multiplier_value(value)?;

    let group_binding_id = facts
        .group_binding_id
        .filter(|value| !value.trim().is_empty())
        .ok_or(MultiplierRejectReason::UnboundGroup)?;

    let confidence = facts.collected_rate_confidence.unwrap_or(0.0);
    if confidence < min_confidence {
        return Err(MultiplierRejectReason::LowConfidence);
    }

    let valid_until_ms = match facts.collected_rate_valid_until_ms {
        Some(valid_until_ms) => valid_until_ms,
        None => {
            let collected_at_ms = facts
                .collected_rate_collected_at_ms
                .ok_or(MultiplierRejectReason::Missing)?;
            collected_at_ms + (3 * group_rate_interval_ms).max(ONE_HOUR_MS)
        }
    };
    if now_ms > valid_until_ms {
        return Err(MultiplierRejectReason::Expired);
    }

    Ok(EffectiveMultiplierFact {
        station_key_id: facts.station_key_id,
        value,
        source: facts
            .collected_rate_source
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "group_rate".to_string()),
        collected_at_ms: facts.collected_rate_collected_at_ms,
        valid_until_ms: Some(valid_until_ms),
        confidence,
        group_binding_id: Some(group_binding_id),
    })
}

fn validate_multiplier_value(value: f64) -> Result<(), MultiplierRejectReason> {
    if !value.is_finite() {
        return Err(MultiplierRejectReason::Invalid);
    }
    if value < 0.0 {
        return Err(MultiplierRejectReason::Negative);
    }
    Ok(())
}

fn parse_optional_millis(value: Option<&str>) -> Option<i64> {
    value?.trim().parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::proxy::scheduler::types::{MultiplierRejectReason, MultiplierSourceFacts};

    const NOW_MS: i64 = 1_000_000;
    const MIN_CONFIDENCE: f64 = 0.7;
    const GROUP_RATE_INTERVAL_MS: i64 = 300_000;

    #[test]
    fn manual_override_wins_with_full_confidence() {
        let mut facts = collected_facts();
        facts.manual_rate_multiplier = Some(2.5);
        facts.manual_rate_updated_at = Some("999000".to_string());
        facts.collected_rate_multiplier = Some(0.5);

        let resolved =
            resolve_effective_multiplier(facts, NOW_MS, MIN_CONFIDENCE, GROUP_RATE_INTERVAL_MS)
                .expect("manual override should resolve");

        assert_eq!(resolved.value, 2.5);
        assert_eq!(resolved.source, "manual");
        assert_eq!(resolved.confidence, 1.0);
        assert_eq!(resolved.collected_at_ms, Some(999_000));
        assert_eq!(resolved.valid_until_ms, None);
        assert_eq!(resolved.group_binding_id.as_deref(), Some("binding-pro"));
    }

    #[test]
    fn collected_fact_below_minimum_confidence_rejects_low_confidence() {
        let mut facts = collected_facts();
        facts.collected_rate_confidence = Some(0.69);

        let rejected =
            resolve_effective_multiplier(facts, NOW_MS, MIN_CONFIDENCE, GROUP_RATE_INTERVAL_MS)
                .expect_err("low confidence should reject");

        assert_eq!(rejected, MultiplierRejectReason::LowConfidence);
    }

    #[test]
    fn expired_collected_fact_rejects_expired() {
        let mut facts = collected_facts();
        facts.collected_rate_valid_until_ms = Some(NOW_MS - 1);

        let rejected =
            resolve_effective_multiplier(facts, NOW_MS, MIN_CONFIDENCE, GROUP_RATE_INTERVAL_MS)
                .expect_err("expired fact should reject");

        assert_eq!(rejected, MultiplierRejectReason::Expired);
    }

    #[test]
    fn missing_group_binding_rejects_unbound_group() {
        let mut facts = collected_facts();
        facts.group_binding_id = None;

        let rejected =
            resolve_effective_multiplier(facts, NOW_MS, MIN_CONFIDENCE, GROUP_RATE_INTERVAL_MS)
                .expect_err("unbound collected fact should reject");

        assert_eq!(rejected, MultiplierRejectReason::UnboundGroup);
    }

    #[test]
    fn non_finite_values_reject_invalid() {
        let mut facts = collected_facts();
        facts.collected_rate_multiplier = Some(f64::INFINITY);

        let rejected =
            resolve_effective_multiplier(facts, NOW_MS, MIN_CONFIDENCE, GROUP_RATE_INTERVAL_MS)
                .expect_err("infinite value should reject");

        assert_eq!(rejected, MultiplierRejectReason::Invalid);
    }

    #[test]
    fn negative_values_reject_negative() {
        let mut facts = collected_facts();
        facts.collected_rate_multiplier = Some(-0.1);

        let rejected =
            resolve_effective_multiplier(facts, NOW_MS, MIN_CONFIDENCE, GROUP_RATE_INTERVAL_MS)
                .expect_err("negative value should reject");

        assert_eq!(rejected, MultiplierRejectReason::Negative);
    }

    fn collected_facts() -> MultiplierSourceFacts {
        MultiplierSourceFacts {
            station_key_id: "key-1".to_string(),
            manual_rate_multiplier: None,
            manual_rate_updated_at: None,
            group_binding_id: Some("binding-pro".to_string()),
            group_id_hash: Some("hash-pro".to_string()),
            group_name: Some("Pro".to_string()),
            collected_rate_multiplier: Some(1.25),
            collected_rate_source: Some("rate_api".to_string()),
            collected_rate_confidence: Some(0.8),
            collected_rate_collected_at_ms: Some(NOW_MS - 1_000),
            collected_rate_valid_until_ms: Some(NOW_MS + 1_000),
        }
    }
}
