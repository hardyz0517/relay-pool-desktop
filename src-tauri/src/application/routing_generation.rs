use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::models::routing_generation::NewRoutingRuntimeGeneration;

pub(crate) const ROUTING_GENERATION_ALGORITHM_VERSION: &str = "routing-generation-v3";
pub(crate) const ROUTING_CUTOVER_FENCE_PROTOCOL_REVISION: u64 = 1;
const GENERATION_ID_DOMAIN: &str = "routing-generation-id/v1";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum RoutingGenerationIdentityError {
    #[error("generation identity input is invalid")]
    InvalidInput,
    #[error("generation hash is invalid")]
    InvalidHash,
    #[error("generation payload is not serializable")]
    Serialization,
}

pub(crate) fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

pub(crate) fn canonical_json_sha256(
    value: &Value,
) -> Result<String, RoutingGenerationIdentityError> {
    Ok(sha256_hex(&canonical_json_bytes(value)?))
}

pub(crate) fn canonical_json_bytes(
    value: &Value,
) -> Result<Vec<u8>, RoutingGenerationIdentityError> {
    serde_json::to_vec(&canonicalize_json(value))
        .map_err(|_| RoutingGenerationIdentityError::Serialization)
}

pub(crate) fn policy_generation_id(
    scope: &str,
    source_policy_revision: u64,
    target_policy_version: &str,
    canonical_policy_hash: &str,
    policy_algorithm_version: &str,
) -> Result<String, RoutingGenerationIdentityError> {
    if scope.is_empty()
        || source_policy_revision == 0
        || target_policy_version.is_empty()
        || !is_sha256_hex(canonical_policy_hash)
        || !valid_algorithm_version(policy_algorithm_version)
    {
        return Err(RoutingGenerationIdentityError::InvalidInput);
    }
    generation_id(
        "pg1_",
        &[
            "routing-policy-v3",
            scope,
            &source_policy_revision.to_string(),
            target_policy_version,
            canonical_policy_hash,
            policy_algorithm_version,
        ],
    )
}

pub(crate) fn quality_generation_id(
    evaluation_at_ms: i64,
    input_observation_watermark: u64,
    quality_policy_revision: u64,
    algorithm_version: &str,
    input_observation_hash: &str,
) -> Result<String, RoutingGenerationIdentityError> {
    if evaluation_at_ms < 0
        || quality_policy_revision == 0
        || !valid_algorithm_version(algorithm_version)
        || !is_sha256_hex(input_observation_hash)
    {
        return Err(RoutingGenerationIdentityError::InvalidInput);
    }
    generation_id(
        "qg1_",
        &[
            "routing-quality-v3",
            "station_key",
            &quality_policy_revision.to_string(),
            &evaluation_at_ms.to_string(),
            &input_observation_watermark.to_string(),
            input_observation_hash,
            algorithm_version,
        ],
    )
}

pub(crate) fn circuit_generation_id(
    input_circuit_event_watermark: u64,
    circuit_policy_revision: u64,
    algorithm_version: &str,
    input_circuit_event_hash: &str,
) -> Result<String, RoutingGenerationIdentityError> {
    if circuit_policy_revision == 0
        || !valid_algorithm_version(algorithm_version)
        || !is_sha256_hex(input_circuit_event_hash)
    {
        return Err(RoutingGenerationIdentityError::InvalidInput);
    }
    generation_id(
        "cg1_",
        &[
            "routing-circuit-v3",
            "station_key",
            &circuit_policy_revision.to_string(),
            &input_circuit_event_watermark.to_string(),
            input_circuit_event_hash,
            algorithm_version,
        ],
    )
}

pub(crate) fn runtime_generation_id(
    generation: &NewRoutingRuntimeGeneration,
) -> Result<String, RoutingGenerationIdentityError> {
    validate_new_runtime_generation(generation, false)?;
    generation_id(
        "rg1_",
        &[
            "routing-runtime-v3",
            "global",
            &generation.policy_generation_id,
            &generation.quality_generation_id,
            &generation.circuit_generation_id,
            &ROUTING_CUTOVER_FENCE_PROTOCOL_REVISION.to_string(),
        ],
    )
}

pub(crate) fn validate_new_runtime_generation(
    generation: &NewRoutingRuntimeGeneration,
    require_identity: bool,
) -> Result<(), RoutingGenerationIdentityError> {
    let ids_are_valid = [
        generation.policy_generation_id.as_str(),
        generation.quality_generation_id.as_str(),
        generation.circuit_generation_id.as_str(),
        generation.checkpoint_ref.as_str(),
        generation.policy_checkpoint_ref.as_str(),
        generation.quality_checkpoint_ref.as_str(),
        generation.circuit_checkpoint_ref.as_str(),
    ]
    .into_iter()
    .all(|value| !value.is_empty() && value.len() <= 192 && !value.chars().any(char::is_control));
    let hashes_are_valid = [
        generation.policy_input_hash.as_str(),
        generation.quality_input_hash.as_str(),
        generation.circuit_input_hash.as_str(),
        generation.policy_content_hash.as_str(),
        generation.quality_content_hash.as_str(),
        generation.circuit_content_hash.as_str(),
    ]
    .into_iter()
    .all(is_sha256_hex);
    if !ids_are_valid
        || !hashes_are_valid
        || generation.policy_revision == 0
        || generation.quality_policy_revision == 0
        || generation.circuit_policy_revision == 0
        || generation.algorithm_version.is_empty()
        || generation.algorithm_version.len() > 96
        || generation.created_at_ms < 0
    {
        return Err(RoutingGenerationIdentityError::InvalidInput);
    }
    if require_identity {
        let expected = runtime_generation_id(generation)?;
        if generation.runtime_generation_id != expected {
            return Err(RoutingGenerationIdentityError::InvalidHash);
        }
    }
    Ok(())
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn generation_id(prefix: &str, fields: &[&str]) -> Result<String, RoutingGenerationIdentityError> {
    if fields
        .iter()
        .any(|field| field.chars().any(char::is_control))
    {
        return Err(RoutingGenerationIdentityError::InvalidInput);
    }
    let mut preimage = Vec::from(GENERATION_ID_DOMAIN.as_bytes());
    for field in fields {
        preimage.push(0x1f);
        preimage.extend_from_slice(field.len().to_string().as_bytes());
        preimage.push(b':');
        preimage.extend_from_slice(field.as_bytes());
    }
    Ok(format!("{prefix}{}", sha256_hex(&preimage)))
}

fn valid_algorithm_version(value: &str) -> bool {
    !value.is_empty() && value.len() <= 96 && !value.chars().any(char::is_control)
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonicalize_json).collect()),
        Value::Object(values) => {
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort();
            let mut canonical = Map::new();
            for key in keys {
                canonical.insert(key.clone(), canonicalize_json(&values[key]));
            }
            Value::Object(canonical)
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_hash_ignores_object_insertion_order() {
        let left = serde_json::json!({"b": 2, "a": {"d": 4, "c": 3}});
        let right = serde_json::json!({"a": {"c": 3, "d": 4}, "b": 2});
        assert_eq!(
            canonical_json_sha256(&left).unwrap(),
            canonical_json_sha256(&right).unwrap()
        );
    }

    #[test]
    fn quality_identity_is_bound_to_watermark_policy_and_input() {
        let hash = "a".repeat(64);
        let baseline = quality_generation_id(10, 20, 3, "routing_quality_v3", &hash).unwrap();
        assert_ne!(
            baseline,
            quality_generation_id(10, 21, 3, "routing_quality_v3", &hash).unwrap()
        );
        assert_ne!(
            baseline,
            quality_generation_id(10, 20, 4, "routing_quality_v3", &hash).unwrap()
        );
        assert!(baseline.starts_with("qg1_"));
    }

    #[test]
    fn generation_ids_use_the_frozen_length_prefixed_domain() {
        let hash = "a".repeat(64);
        let policy =
            policy_generation_id("active", 7, "routing-policy-v3", &hash, "routing-policy-v3")
                .unwrap();
        assert_eq!(
            policy,
            "pg1_26b9d4135dc95408ab4e089b631c16bac2bb5b553e92720ff5e32bb5bd8c86c6"
        );
        assert_eq!(
            quality_generation_id(10, 20, 7, "routing_quality_v3", &hash).unwrap(),
            "qg1_78d6f8f687e96a268ea78973613c86fae4a41f9dae583fcc0d8e0b8e83c424c5"
        );
        assert_eq!(
            circuit_generation_id(0, 7, "station-key-circuit-v3", &hash).unwrap(),
            "cg1_d0874ed78d62ef7eef1ab21a9786a26442fabdd1127bc2ca9b4d7aa29fe48a87"
        );
        let runtime = NewRoutingRuntimeGeneration {
            runtime_generation_id: String::new(),
            policy_generation_id: "pg1_test".into(),
            quality_generation_id: "qg1_test".into(),
            circuit_generation_id: "cg1_test".into(),
            policy_revision: 7,
            quality_policy_revision: 7,
            circuit_policy_revision: 7,
            algorithm_version: ROUTING_GENERATION_ALGORITHM_VERSION.into(),
            input_observation_watermark: 0,
            input_circuit_event_watermark: 0,
            policy_input_hash: hash.clone(),
            quality_input_hash: hash.clone(),
            circuit_input_hash: hash.clone(),
            policy_content_hash: hash.clone(),
            quality_content_hash: hash.clone(),
            circuit_content_hash: hash,
            checkpoint_ref: "runtime-checkpoint".into(),
            policy_checkpoint_ref: "policy-checkpoint".into(),
            quality_checkpoint_ref: "quality-checkpoint".into(),
            circuit_checkpoint_ref: "circuit-checkpoint".into(),
            created_at_ms: 10,
        };
        assert_eq!(
            runtime_generation_id(&runtime).unwrap(),
            "rg1_b08f75e8a59e47dc9dec5f38ade745a5b59fe3d177864d4349df617214eafd61"
        );
    }
}
