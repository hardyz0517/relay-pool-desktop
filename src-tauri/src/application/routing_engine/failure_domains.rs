use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct FailureDomainSet(BTreeSet<String>);
impl FailureDomainSet {
    pub(crate) fn insert(&mut self, domain: impl Into<String>) {
        self.0.insert(domain.into());
    }
    pub(crate) fn contains(&self, domain: &str) -> bool {
        self.0.contains(domain)
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &String> {
        self.0.iter()
    }
}

pub(crate) fn correlated_with(excluded: &FailureDomainSet, candidate: &FailureDomainSet) -> bool {
    candidate.iter().any(|domain| excluded.contains(domain))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FailureDomain {
    Endpoint(String),
    ProviderAccount(String),
    StationAccount(String),
    StationKey(String),
    ModelFamily(String),
}

impl FailureDomain {
    pub(crate) fn canonical(&self) -> String {
        match self {
            Self::Endpoint(value) => format!("endpoint:{value}"),
            Self::ProviderAccount(value) => format!("provider_account:{value}"),
            Self::StationAccount(value) => format!("station_account:{value}"),
            Self::StationKey(value) => format!("station_key:{value}"),
            Self::ModelFamily(value) => format!("model_family:{value}"),
        }
    }
}

pub(crate) fn max_ejection_count(candidate_count: usize, max_percent: u8) -> usize {
    if candidate_count == 0 { return 0; }
    ((candidate_count.saturating_mul(usize::from(max_percent))).saturating_add(99) / 100)
        .min(candidate_count.saturating_sub(1).max(1))
}
