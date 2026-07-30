use std::{future::Future, pin::Pin};

use super::assembler::{
    assemble_operational_fact_bundle, OperationalFactAssemblyError, OperationalFactBundle,
    OperationalFactReadOptions, RawOperationalFactRows,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum OperationalFactReadError {
    #[error("{0}")]
    Assembly(#[from] OperationalFactAssemblyError),
    #[error("operational fact source failed: {0}")]
    Source(String),
}

pub(crate) trait OperationalFactSource {
    fn load_raw_operational_facts<'a>(
        &'a self,
        options: &'a OperationalFactReadOptions,
    ) -> Pin<Box<dyn Future<Output = Result<RawOperationalFactRows, OperationalFactReadError>> + Send + 'a>>;
}

#[derive(Debug, Clone)]
pub(crate) struct OperationalFactReader<S> {
    source: S,
}

impl<S> OperationalFactReader<S>
where
    S: OperationalFactSource,
{
    pub(crate) fn new(source: S) -> Self {
        Self { source }
    }

    pub(crate) async fn load_bundle(
        &self,
        options: &OperationalFactReadOptions,
    ) -> Result<OperationalFactBundle, OperationalFactReadError> {
        let raw = self.source.load_raw_operational_facts(options).await?;
        Ok(assemble_operational_fact_bundle(raw, options)?)
    }
}
