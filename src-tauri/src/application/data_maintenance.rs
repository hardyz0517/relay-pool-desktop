use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    background_tasks::OperationRegistry,
    persistence::runtime::{ActivationFreezeEvidence, PersistenceRuntime, RuntimeTransitionError},
    services::{
        proxy::runtime::ProxyRuntimeState, station_collectors::StationCollectorRunnerState,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataMaintenanceState {
    Normal,
    Exporting,
    InspectingImport,
    PreparingImport,
    ActivationPending,
    Recovering,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataMaintenanceActivity {
    Export,
    InspectImport,
    PrepareImport,
}

impl DataMaintenanceActivity {
    fn state(self) -> DataMaintenanceState {
        match self {
            Self::Export => DataMaintenanceState::Exporting,
            Self::InspectImport => DataMaintenanceState::InspectingImport,
            Self::PrepareImport => DataMaintenanceState::PreparingImport,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataCommandAdmission {
    Read,
    Mutation,
    MaintenanceRead,
    MaintenanceActivity(DataMaintenanceActivity),
    ActivationCommit,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum DataMaintenanceError {
    #[error("data maintenance activity is already active")]
    Busy,
    #[error("data maintenance state transition is invalid")]
    InvalidTransition,
    #[error("data maintenance activation is pending restart")]
    ActivationPending,
    #[error("data maintenance recovery is active")]
    Recovering,
    #[error("mutation is rejected during data maintenance activation")]
    MutationRejected,
    #[error("background operations did not stop before the maintenance deadline")]
    OperationDrainTimedOut,
    #[error("background runner did not stop before the maintenance deadline")]
    RunnerDrainTimedOut,
    #[error("local proxy did not stop before the maintenance deadline")]
    ProxyDrainFailed,
    #[error("persistence runtime freeze failed")]
    PersistenceFreezeFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CoordinatorInner {
    state: DataMaintenanceState,
    lease_id: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct DataMaintenanceCoordinator {
    inner: Arc<Mutex<CoordinatorInner>>,
}

impl Default for DataMaintenanceCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl DataMaintenanceCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(CoordinatorInner {
                state: DataMaintenanceState::Normal,
                lease_id: 0,
            })),
        }
    }

    pub(crate) fn state(&self) -> DataMaintenanceState {
        self.inner.lock().expect("maintenance mutex").state
    }

    pub(crate) fn admit_command(
        &self,
        admission: DataCommandAdmission,
    ) -> Result<(), DataMaintenanceError> {
        let state = self.state();
        match (state, admission) {
            (_, DataCommandAdmission::Read | DataCommandAdmission::MaintenanceRead) => Ok(()),
            (DataMaintenanceState::Normal, DataCommandAdmission::Mutation) => Ok(()),
            (
                DataMaintenanceState::Exporting
                | DataMaintenanceState::InspectingImport
                | DataMaintenanceState::PreparingImport,
                DataCommandAdmission::Mutation,
            ) => Ok(()),
            (DataMaintenanceState::Normal, DataCommandAdmission::MaintenanceActivity(_)) => Ok(()),
            (DataMaintenanceState::PreparingImport, DataCommandAdmission::ActivationCommit) => {
                Ok(())
            }
            (DataMaintenanceState::ActivationPending, _) => {
                Err(DataMaintenanceError::ActivationPending)
            }
            (DataMaintenanceState::Recovering, _) => Err(DataMaintenanceError::Recovering),
            _ => Err(DataMaintenanceError::Busy),
        }
    }

    pub(crate) fn begin(
        &self,
        activity: DataMaintenanceActivity,
    ) -> Result<DataMaintenanceLease, DataMaintenanceError> {
        let mut inner = self.inner.lock().expect("maintenance mutex");
        if inner.state != DataMaintenanceState::Normal {
            return Err(match inner.state {
                DataMaintenanceState::ActivationPending => DataMaintenanceError::ActivationPending,
                DataMaintenanceState::Recovering => DataMaintenanceError::Recovering,
                _ => DataMaintenanceError::Busy,
            });
        }
        inner.lease_id = inner.lease_id.saturating_add(1);
        inner.state = activity.state();
        Ok(DataMaintenanceLease {
            coordinator: self.clone(),
            activity,
            lease_id: inner.lease_id,
            active: true,
        })
    }

    pub(crate) fn enter_recovery(&self) -> Result<(), DataMaintenanceError> {
        let mut inner = self.inner.lock().expect("maintenance mutex");
        if inner.state != DataMaintenanceState::Normal {
            return Err(DataMaintenanceError::Busy);
        }
        inner.state = DataMaintenanceState::Recovering;
        Ok(())
    }

    pub(crate) fn finish_recovery(&self) -> Result<(), DataMaintenanceError> {
        let mut inner = self.inner.lock().expect("maintenance mutex");
        if inner.state != DataMaintenanceState::Recovering {
            return Err(DataMaintenanceError::InvalidTransition);
        }
        inner.state = DataMaintenanceState::Normal;
        Ok(())
    }

    fn release(&self, activity: DataMaintenanceActivity, lease_id: u64) {
        let mut inner = self.inner.lock().expect("maintenance mutex");
        if inner.lease_id == lease_id && inner.state == activity.state() {
            inner.state = DataMaintenanceState::Normal;
        }
    }

    fn commit_activation(&self, lease_id: u64) -> Result<(), DataMaintenanceError> {
        let mut inner = self.inner.lock().expect("maintenance mutex");
        if inner.lease_id != lease_id || inner.state != DataMaintenanceState::PreparingImport {
            return Err(DataMaintenanceError::InvalidTransition);
        }
        inner.state = DataMaintenanceState::ActivationPending;
        Ok(())
    }

    pub(crate) async fn freeze_for_activation(
        &self,
        lease: &mut DataMaintenanceLease,
        runtime: &PersistenceRuntime,
        operations: &OperationRegistry,
        runner: Option<&StationCollectorRunnerState>,
        proxy: Option<&ProxyRuntimeState>,
        deadline: Duration,
    ) -> Result<ActivationFreezeEvidence, DataMaintenanceError> {
        if lease.activity != DataMaintenanceActivity::PrepareImport || !lease.active {
            return Err(DataMaintenanceError::InvalidTransition);
        }

        let operation_report = operations.stop_admission_and_cancel(deadline).await;
        if !operation_report.timed_out.is_empty() {
            return Err(DataMaintenanceError::OperationDrainTimedOut);
        }
        if let Some(runner) = runner {
            runner
                .stop_and_join(deadline)
                .await
                .map_err(|_| DataMaintenanceError::RunnerDrainTimedOut)?;
        }
        if let Some(proxy) = proxy {
            proxy
                .drain_for_data_maintenance(deadline)
                .await
                .map_err(|_| DataMaintenanceError::ProxyDrainFailed)?;
        }
        let evidence = runtime
            .freeze_for_activation(deadline)
            .await
            .map_err(map_freeze_error)?;
        self.commit_activation(lease.lease_id)?;
        lease.active = false;
        Ok(evidence)
    }
}

fn map_freeze_error(_error: RuntimeTransitionError) -> DataMaintenanceError {
    DataMaintenanceError::PersistenceFreezeFailed
}

#[derive(Debug)]
pub(crate) struct DataMaintenanceLease {
    coordinator: DataMaintenanceCoordinator,
    activity: DataMaintenanceActivity,
    lease_id: u64,
    active: bool,
}

impl DataMaintenanceLease {
    pub(crate) fn activity(&self) -> DataMaintenanceActivity {
        self.activity
    }
}

impl Drop for DataMaintenanceLease {
    fn drop(&mut self) {
        if self.active {
            self.coordinator.release(self.activity, self.lease_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        application::data_maintenance::{
            DataCommandAdmission, DataMaintenanceActivity, DataMaintenanceCoordinator,
            DataMaintenanceError, DataMaintenanceState,
        },
        background_tasks::{
            OperationOwner, OperationRegistry, OperationRegistryConfig, OperationStartRequest,
            OperationTerminal,
        },
        persistence::{error::PersistenceError, runtime::PersistenceRuntime},
    };

    #[test]
    fn maintenance_leases_are_exclusive_and_raii_return_to_normal() {
        let coordinator = DataMaintenanceCoordinator::new();
        let export = coordinator
            .begin(DataMaintenanceActivity::Export)
            .expect("export lease");

        assert_eq!(coordinator.state(), DataMaintenanceState::Exporting);
        assert!(matches!(
            coordinator.begin(DataMaintenanceActivity::InspectImport),
            Err(DataMaintenanceError::Busy)
        ));

        drop(export);
        assert_eq!(coordinator.state(), DataMaintenanceState::Normal);
        let inspect = coordinator
            .begin(DataMaintenanceActivity::InspectImport)
            .expect("inspect lease after release");
        assert_eq!(inspect.activity(), DataMaintenanceActivity::InspectImport);
    }

    #[test]
    fn export_and_inspection_do_not_block_business_mutations() {
        for activity in [
            DataMaintenanceActivity::Export,
            DataMaintenanceActivity::InspectImport,
        ] {
            let coordinator = DataMaintenanceCoordinator::new();
            let _lease = coordinator.begin(activity).expect("lease");
            assert_eq!(
                coordinator.admit_command(DataCommandAdmission::Mutation),
                Ok(())
            );
        }
    }

    #[test]
    fn activation_pending_rejects_mutations_and_new_maintenance() {
        let coordinator = DataMaintenanceCoordinator::new();
        let lease = coordinator
            .begin(DataMaintenanceActivity::PrepareImport)
            .expect("prepare lease");

        coordinator
            .commit_activation(lease.lease_id)
            .expect("commit activation");
        drop(lease);

        assert_eq!(coordinator.state(), DataMaintenanceState::ActivationPending);
        assert_eq!(
            coordinator.admit_command(DataCommandAdmission::Read),
            Ok(())
        );
        assert_eq!(
            coordinator.admit_command(DataCommandAdmission::Mutation),
            Err(DataMaintenanceError::ActivationPending)
        );
        assert!(matches!(
            coordinator.begin(DataMaintenanceActivity::Export),
            Err(DataMaintenanceError::ActivationPending)
        ));
    }

    #[tokio::test]
    async fn activation_freeze_blocks_command_and_persistence_admission_together() {
        let root = tempfile::tempdir().expect("temp directory");
        let path = root.path().join("runtime.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("runtime");
        let coordinator = DataMaintenanceCoordinator::new();
        let mut lease = coordinator
            .begin(DataMaintenanceActivity::PrepareImport)
            .expect("prepare lease");
        let operations = OperationRegistry::new(OperationRegistryConfig::architecture_budget());

        coordinator
            .freeze_for_activation(
                &mut lease,
                &runtime,
                &operations,
                None,
                None,
                Duration::from_secs(1),
            )
            .await
            .expect("freeze");

        assert_eq!(coordinator.state(), DataMaintenanceState::ActivationPending);
        assert!(matches!(
            runtime.handle().begin_write().await,
            Err(PersistenceError::RuntimeUnavailable)
        ));
        assert_eq!(
            coordinator.admit_command(DataCommandAdmission::Mutation),
            Err(DataMaintenanceError::ActivationPending)
        );
    }

    #[tokio::test]
    async fn activation_freeze_cancels_and_joins_running_operations() {
        let root = tempfile::tempdir().expect("temp directory");
        let path = root.path().join("runtime.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("runtime");
        let coordinator = DataMaintenanceCoordinator::new();
        let mut lease = coordinator
            .begin(DataMaintenanceActivity::PrepareImport)
            .expect("prepare lease");
        let operations = OperationRegistry::new(OperationRegistryConfig::architecture_budget());
        let id = operations
            .start(OperationStartRequest::new(
                "maintenance-test",
                OperationOwner::new("test"),
                |context| {
                    Box::pin(async move {
                        context.cancellation_token.cancelled().await;
                        OperationTerminal::Cancelled
                    })
                },
            ))
            .expect("operation starts");

        coordinator
            .freeze_for_activation(
                &mut lease,
                &runtime,
                &operations,
                None,
                None,
                Duration::from_secs(1),
            )
            .await
            .expect("freeze");

        let status = operations.status(id).expect("status retained");
        assert_eq!(status.terminal, Some(OperationTerminal::Cancelled));
        assert!(operations
            .start(OperationStartRequest::new(
                "after-freeze",
                OperationOwner::new("test"),
                |_| Box::pin(async { OperationTerminal::Completed }),
            ))
            .is_err());
    }
}
