//! Bounded, non-regex glob matcher used by model mapping rules.
//!
//! The language is deliberately small: `*` matches zero or more Unicode scalar
//! values, `?` matches one value, and `\\` quotes the next value.  Compilation
//! rejects malformed or over-sized patterns.  Runtime matching uses the
//! standard linear greedy algorithm; overlap checks use a bounded product of
//! the two epsilon NFAs and never enumerate model names.

use std::collections::{BTreeSet, VecDeque};

pub(crate) const MAX_PATTERN_BYTES: usize = 256;
pub(crate) const MAX_PATTERN_TOKENS: usize = 256;
pub(crate) const MAX_INTERSECTION_STATES: usize = 16_384;
pub(crate) const MAX_INTERSECTION_WORK: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GlobCompileError {
    Empty,
    TooLong,
    TooManyTokens,
    TrailingEscape,
    InvalidControl,
}

impl std::fmt::Display for GlobCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "glob pattern must not be empty",
            Self::TooLong => "glob pattern is too long",
            Self::TooManyTokens => "glob pattern has too many tokens",
            Self::TrailingEscape => "glob pattern has a trailing escape",
            Self::InvalidControl => "glob pattern contains a control character",
        })
    }
}

impl std::error::Error for GlobCompileError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GlobToken {
    Literal(char),
    Any,
    Star,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledGlob {
    tokens: Vec<GlobToken>,
    literal_count: usize,
}

impl CompiledGlob {
    pub(crate) fn compile(pattern: &str) -> Result<Self, GlobCompileError> {
        if pattern.is_empty() {
            return Err(GlobCompileError::Empty);
        }
        if pattern.len() > MAX_PATTERN_BYTES {
            return Err(GlobCompileError::TooLong);
        }
        let mut tokens = Vec::new();
        let mut chars = pattern.chars();
        let mut literal_count = 0;
        while let Some(character) = chars.next() {
            let token = match character {
                '\\' => {
                    let escaped = chars.next().ok_or(GlobCompileError::TrailingEscape)?;
                    if escaped == '\0' || escaped.is_control() {
                        return Err(GlobCompileError::InvalidControl);
                    }
                    literal_count += 1;
                    GlobToken::Literal(escaped)
                }
                '*' => GlobToken::Star,
                '?' => GlobToken::Any,
                character if character == '\0' || character.is_control() => {
                    return Err(GlobCompileError::InvalidControl)
                }
                character => {
                    literal_count += 1;
                    GlobToken::Literal(character)
                }
            };
            if !matches!(tokens.last(), Some(GlobToken::Star)) || !matches!(token, GlobToken::Star)
            {
                tokens.push(token);
            }
            if tokens.len() > MAX_PATTERN_TOKENS {
                return Err(GlobCompileError::TooManyTokens);
            }
        }
        if tokens.is_empty() {
            return Err(GlobCompileError::Empty);
        }
        Ok(Self {
            tokens,
            literal_count,
        })
    }

    pub(crate) fn literal_count(&self) -> usize {
        self.literal_count
    }

    pub(crate) fn matches(&self, input: &str) -> bool {
        let input: Vec<char> = input.chars().collect();
        let mut input_index = 0usize;
        let mut token_index = 0usize;
        let mut star_index: Option<usize> = None;
        let mut star_input_index = 0usize;
        while input_index < input.len() {
            match self.tokens.get(token_index) {
                Some(GlobToken::Literal(expected)) if *expected == input[input_index] => {
                    token_index += 1;
                    input_index += 1;
                }
                Some(GlobToken::Any) => {
                    token_index += 1;
                    input_index += 1;
                }
                Some(GlobToken::Star) => {
                    star_index = Some(token_index);
                    star_input_index = input_index;
                    token_index += 1;
                }
                _ => {
                    let Some(star) = star_index else {
                        return false;
                    };
                    token_index = star + 1;
                    star_input_index += 1;
                    input_index = star_input_index;
                }
            }
        }
        while matches!(self.tokens.get(token_index), Some(GlobToken::Star)) {
            token_index += 1;
        }
        token_index == self.tokens.len()
    }

    /// Returns whether the two compiled patterns accept at least one common
    /// string.  `None` means the bounded analysis budget was exceeded.
    pub(crate) fn intersects(&self, other: &Self) -> Result<bool, GlobIntersectionError> {
        let alphabet = representative_alphabet(self, other);
        let start = (0usize, 0usize);
        let mut queue = VecDeque::from([start]);
        let mut seen = BTreeSet::from([start]);
        let mut work = 0usize;
        while let Some((left, right)) = queue.pop_front() {
            work += 1;
            if work > MAX_INTERSECTION_WORK || seen.len() > MAX_INTERSECTION_STATES {
                return Err(GlobIntersectionError::BudgetExceeded);
            }
            let left_closure = epsilon_closure(&self.tokens, left);
            let right_closure = epsilon_closure(&other.tokens, right);
            for &left_state in &left_closure {
                for &right_state in &right_closure {
                    if left_state == self.tokens.len() && right_state == other.tokens.len() {
                        return Ok(true);
                    }
                }
            }
            for representative in &alphabet {
                let left_next = consume_states(&self.tokens, &left_closure, *representative);
                if left_next.is_empty() {
                    continue;
                }
                let right_next = consume_states(&other.tokens, &right_closure, *representative);
                if right_next.is_empty() {
                    continue;
                }
                for left_state in &left_next {
                    for right_state in &right_next {
                        let next = (*left_state, *right_state);
                        if seen.insert(next) {
                            queue.push_back(next);
                        }
                    }
                }
            }
        }
        Ok(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobIntersectionError {
    BudgetExceeded,
}

impl std::fmt::Display for GlobIntersectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("glob intersection analysis exceeded its bounded budget")
    }
}

impl std::error::Error for GlobIntersectionError {}

fn epsilon_closure(tokens: &[GlobToken], start: usize) -> Vec<usize> {
    let mut result = Vec::with_capacity(2);
    let mut current = start;
    result.push(current);
    while matches!(tokens.get(current), Some(GlobToken::Star)) {
        current += 1;
        result.push(current);
    }
    result
}

fn consume_states(tokens: &[GlobToken], closure: &[usize], character: char) -> Vec<usize> {
    let mut states = Vec::new();
    for index in closure {
        let next = match tokens.get(*index) {
            Some(GlobToken::Literal(expected)) if *expected == character => Some(index + 1),
            Some(GlobToken::Any) => Some(index + 1),
            // `*` consumes one value and remains in the same NFA state; its
            // epsilon edge to the next state is handled by `epsilon_closure`.
            Some(GlobToken::Star) => Some(*index),
            _ => None,
        };
        if let Some(next) = next {
            if !states.contains(&next) {
                states.push(next);
            }
        }
    }
    states
}

fn representative_alphabet(left: &CompiledGlob, right: &CompiledGlob) -> Vec<char> {
    let mut literals = BTreeSet::new();
    for token in left.tokens.iter().chain(right.tokens.iter()) {
        if let GlobToken::Literal(character) = token {
            literals.insert(*character);
        }
    }
    // Any character not in the literal set is equivalent for both automata.
    // Use a non-control scalar that cannot collide with the finite set.
    let other = ['\u{10ffff}', '\u{fffd}', 'a', '0', '_']
        .into_iter()
        .find(|character| !literals.contains(character))
        .unwrap_or('\u{10ffff}');
    literals.into_iter().chain([other]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_exact_wildcard_and_escaped_metacharacters() {
        let exact = CompiledGlob::compile("deepseek-v4").unwrap();
        assert!(exact.matches("deepseek-v4"));
        assert!(!exact.matches("deepseek-v4-flash"));
        let wildcard = CompiledGlob::compile("codex-*").unwrap();
        assert!(wildcard.matches("codex-5.4"));
        assert!(wildcard.matches("codex-"));
        assert!(!wildcard.matches("gpt-5"));
        let escaped = CompiledGlob::compile(r"literal-\*\?").unwrap();
        assert!(escaped.matches("literal-*?"));
        assert!(!escaped.matches("literal-any"));
    }

    #[test]
    fn rejects_malformed_or_unsafe_patterns() {
        assert_eq!(
            CompiledGlob::compile("").unwrap_err(),
            GlobCompileError::Empty
        );
        assert_eq!(
            CompiledGlob::compile("foo\\").unwrap_err(),
            GlobCompileError::TrailingEscape
        );
        assert_eq!(
            CompiledGlob::compile("foo\nbar").unwrap_err(),
            GlobCompileError::InvalidControl
        );
    }

    #[test]
    fn intersection_is_exact_for_representative_literal_classes() {
        let left = CompiledGlob::compile("codex-*").unwrap();
        let right = CompiledGlob::compile("*-5.4").unwrap();
        assert!(left.intersects(&right).unwrap());
        let disjoint = CompiledGlob::compile("deepseek-*").unwrap();
        assert!(!left.intersects(&disjoint).unwrap());
        let exact = CompiledGlob::compile("codex-5.4").unwrap();
        assert!(left.intersects(&exact).unwrap());
    }
}
