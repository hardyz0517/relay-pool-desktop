#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortableActivationStep {
    TargetValidated,
    BackupVerified,
    BeforeFreeze,
    AfterFreeze,
    BeforeJournalPublish,
    AfterJournalPublish,
}

pub(crate) trait PortableActivationFaults: Send + Sync {
    fn check(&self, step: PortableActivationStep) -> Result<(), PortableActivationFault>;
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NoPortableActivationFaults;

impl PortableActivationFaults for NoPortableActivationFaults {
    fn check(&self, _step: PortableActivationStep) -> Result<(), PortableActivationFault> {
        Ok(())
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct InjectPortableActivationFault {
    step: PortableActivationStep,
}

#[cfg(test)]
impl InjectPortableActivationFault {
    pub(crate) fn at(step: PortableActivationStep) -> Self {
        Self { step }
    }
}

#[cfg(test)]
impl PortableActivationFaults for InjectPortableActivationFault {
    fn check(&self, step: PortableActivationStep) -> Result<(), PortableActivationFault> {
        if self.step == step {
            Err(PortableActivationFault::Injected(step))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum PortableActivationFault {
    #[error("injected portable activation fault at {0:?}")]
    Injected(PortableActivationStep),
}
