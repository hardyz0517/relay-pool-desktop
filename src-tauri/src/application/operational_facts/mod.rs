#![allow(unused_imports)]

pub(crate) mod assembler;
#[cfg(test)]
pub(crate) mod asset_status_projector;
pub(crate) mod balance_projector;
pub(crate) mod candidate_projector;
pub(crate) mod capability_projector;
pub(crate) mod group_projector;
pub(crate) mod health_projector;
pub(crate) mod multiplier_projector;
pub(crate) mod planning_snapshot;
pub(crate) mod pricing_projector;
pub(crate) mod reader;
pub(crate) mod reliability_projector;
pub(crate) mod responsiveness_projector;
pub(crate) mod candidate_projection;
#[cfg(test)]
pub(crate) mod runtime_health_port;
pub(crate) mod target_resolver;

pub(crate) use crate::models::operational::MAX_OPERATIONAL_CANDIDATES;
pub(crate) use crate::models::operational::{
    OperationalFactReadOptions, RawOperationalCandidateRow, RawOperationalFactRows,
    RawOperationalModelAliasRow, RawOperationalSettingRow,
};
pub(crate) use assembler::{
    assemble_operational_fact_bundle, CredentialAvailabilityFact, FactVersionVector,
    ModelAliasFact, OperationalCandidateFact, OperationalFactBundle, OperationalFactSnapshotId,
    SettingFact,
};
pub(crate) use reader::{OperationalFactReadError, OperationalFactReader, OperationalFactSource};
