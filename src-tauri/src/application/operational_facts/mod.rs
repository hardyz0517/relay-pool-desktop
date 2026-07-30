#![allow(unused_imports)]
#![allow(dead_code)]

pub(crate) mod assembler;
pub(crate) mod reader;

pub(crate) use assembler::{
    assemble_operational_fact_bundle, CredentialAvailabilityFact, FactVersionVector,
    ModelAliasFact, OperationalCandidateFact, OperationalFactBundle, OperationalFactReadOptions,
    OperationalFactSnapshotId, RawOperationalCandidateRow, RawOperationalFactRows,
    RawOperationalModelAliasRow, RawOperationalSettingRow, SettingFact, MAX_OPERATIONAL_CANDIDATES,
};
pub(crate) use reader::{OperationalFactReadError, OperationalFactReader, OperationalFactSource};
