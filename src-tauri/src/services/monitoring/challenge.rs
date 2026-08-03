use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

#[derive(Debug, Clone)]
pub struct ProbeChallenge {
    prompt: String,
    token: String,
    expected_answer_hash: [u8; 32],
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeChallengeSnapshot {
    pub prompt: String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct ChallengeValidator {
    expected_answer_hash: [u8; 32],
}

impl ProbeChallenge {
    pub fn generate_arithmetic() -> Self {
        let mut rng = OsRng;
        let a = 10 + (rng.next_u32() % 90);
        let b = 10 + (rng.next_u32() % 90);
        let mut token_bytes = [0_u8; 12];
        rng.fill_bytes(&mut token_bytes);
        let token = hex_token(&token_bytes);
        let expected_answer = format!("RP_ANSWER={}", a + b);
        let prompt = format!(
            "Compute {a} + {b}. Reply only with RP_ANSWER= immediately followed by the total in digits. Probe token: {token}"
        );
        Self {
            prompt,
            token,
            expected_answer_hash: hash_normalized(&expected_answer),
        }
    }

    pub fn snapshot(&self) -> ProbeChallengeSnapshot {
        ProbeChallengeSnapshot {
            prompt: self.prompt.clone(),
            token: self.token.clone(),
        }
    }

    pub fn validator(&self) -> ChallengeValidator {
        ChallengeValidator {
            expected_answer_hash: self.expected_answer_hash,
        }
    }
}

impl ChallengeValidator {
    #[cfg(test)]
    pub fn from_expected_answer_for_tests(expected_answer: &str) -> Self {
        Self {
            expected_answer_hash: hash_normalized(expected_answer),
        }
    }

    pub fn validate(&self, candidate: &str) -> bool {
        let candidate_hash = hash_normalized(candidate);
        candidate_hash
            .as_slice()
            .ct_eq(self.expected_answer_hash.as_slice())
            .into()
    }
}

fn hash_normalized(value: &str) -> [u8; 32] {
    let normalized = normalize_answer(value);
    Sha256::digest(normalized.as_bytes()).into()
}

fn normalize_answer(value: &str) -> String {
    value.split_whitespace().collect::<String>()
}

fn hex_token(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::{ChallengeValidator, ProbeChallenge};

    #[test]
    fn services_monitoring_challenge_validator_normalizes_without_serializing_answer() {
        let challenge = ProbeChallenge::generate_arithmetic();
        let serialized = serde_json::to_string(&challenge.snapshot()).expect("snapshot");

        assert!(!serialized.contains("expected"));
        assert!(!serialized.contains("answer_hash"));
        assert!(
            ChallengeValidator::from_expected_answer_for_tests("RP_ANSWER=42")
                .validate("RP_ANSWER=42")
        );
        assert!(
            ChallengeValidator::from_expected_answer_for_tests("RP_ANSWER=42")
                .validate("RP_ANSWER= 42")
        );
    }
}
