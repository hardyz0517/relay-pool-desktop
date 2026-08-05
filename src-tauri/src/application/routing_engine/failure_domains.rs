use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct FailureDomainSet(BTreeSet<String>);
impl FailureDomainSet { pub(crate) fn insert(&mut self, domain: impl Into<String>) { self.0.insert(domain.into()); } pub(crate) fn contains(&self, domain: &str) -> bool { self.0.contains(domain) } pub(crate) fn iter(&self) -> impl Iterator<Item = &String> { self.0.iter() } }

pub(crate) fn correlated_with(excluded: &FailureDomainSet, candidate: &FailureDomainSet) -> bool { candidate.iter().any(|domain| excluded.contains(domain)) }
