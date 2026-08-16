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
        answer_markers(candidate).any(|answer| {
            let candidate_hash = hash_normalized(answer);
            candidate_hash
                .as_slice()
                .ct_eq(self.expected_answer_hash.as_slice())
                .into()
        })
    }
}

fn answer_markers(candidate: &str) -> impl Iterator<Item = &str> {
    const PREFIX: &str = "RP_ANSWER=";

    candidate.match_indices(PREFIX).filter_map(|(start, _)| {
        let valid_left_boundary = candidate[..start]
            .chars()
            .next_back()
            .map_or(true, |character| {
                !character.is_ascii_alphanumeric() && character != '_'
            });
        if !valid_left_boundary {
            return None;
        }

        let answer_start = start + PREFIX.len();
        let tail = &candidate[answer_start..];
        let remainder = tail.trim_start_matches(char::is_whitespace);
        let whitespace_bytes = tail.len() - remainder.len();
        let digit_count = remainder
            .as_bytes()
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        let valid_right_boundary = remainder[digit_count..]
            .chars()
            .next()
            .map_or(true, |character| {
                !character.is_ascii_alphanumeric() && character != '_'
            });
        (digit_count > 0 && valid_right_boundary)
            .then(|| &candidate[start..answer_start + whitespace_bytes + digit_count])
    })
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
        let validator = ChallengeValidator::from_expected_answer_for_tests("RP_ANSWER=42");
        assert!(validator.validate("The result is `RP_ANSWER=42`."));
        assert!(!validator.validate("The result is RP_ANSWER=41."));
        assert!(!validator.validate("NOT_RP_ANSWER=42"));
        assert!(!validator.validate("RP_ANSWER=42extra"));
    }
}
