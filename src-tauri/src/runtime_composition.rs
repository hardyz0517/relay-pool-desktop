use std::future::Future;

use crate::persistence::upgrade_fault::{
    UpgradeFailpoint, UpgradeFaultInjector, UpgradeInjectedFailure,
};
use tauri::{Manager, Runtime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum RuntimeCompositionError {
    #[error(transparent)]
    Injected(#[from] UpgradeInjectedFailure),
    #[error("runtime service state slot is already occupied")]
    StateSlotOccupied,
    #[error("runtime service registration failed")]
    ServiceRegistration,
    #[error("proxy finalization drain failed")]
    FinalizationDrain,
}

pub(crate) struct ReadyServiceBundle<Startup, Persistence, Application, Monitor, Collector> {
    startup: Startup,
    persistence: Persistence,
    application: Application,
    monitor: Monitor,
    collector: Collector,
}

impl<Startup, Persistence, Application, Monitor, Collector>
    ReadyServiceBundle<Startup, Persistence, Application, Monitor, Collector>
{
    pub(crate) fn new(
        startup: Startup,
        persistence: Persistence,
        application: Application,
        monitor: Monitor,
        collector: Collector,
    ) -> Self {
        Self {
            startup,
            persistence,
            application,
            monitor,
            collector,
        }
    }
}

// Atomicity relies on `manage` returning `false` only for an already occupied
// concrete TypeId and on every publisher of these private ready-service types
// going through this exclusive registration path.
pub(crate) trait ReadyServiceRegistry {
    fn contains<T: Send + Sync + 'static>(&self) -> bool;
    fn manage<T: Send + Sync + 'static>(&mut self, state: T) -> bool;
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "source-included fault tests exercise the registry contract without constructing a Tauri app"
    )
)]
struct TauriReadyServiceRegistry<'app, R: Runtime>(&'app mut tauri::App<R>);

impl<R: Runtime> ReadyServiceRegistry for TauriReadyServiceRegistry<'_, R> {
    fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.0.try_state::<T>().is_some()
    }

    fn manage<T: Send + Sync + 'static>(&mut self, state: T) -> bool {
        self.0.manage(state)
    }
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "source-included fault tests use the in-memory registry; production startup owns this Tauri adapter"
    )
)]
pub(crate) fn register_ready_services<R, Startup, Persistence, Application, Monitor, Collector>(
    faults: &dyn UpgradeFaultInjector,
    app: &mut tauri::App<R>,
    services: ReadyServiceBundle<Startup, Persistence, Application, Monitor, Collector>,
) -> Result<(), RuntimeCompositionError>
where
    R: Runtime,
    Startup: Send + Sync + 'static,
    Persistence: Send + Sync + 'static,
    Application: Send + Sync + 'static,
    Monitor: Send + Sync + 'static,
    Collector: Send + Sync + 'static,
{
    let mut registry = TauriReadyServiceRegistry(app);
    register_ready_services_in(faults, &mut registry, services)
}

pub(crate) fn register_ready_services_in<
    Registry,
    Startup,
    Persistence,
    Application,
    Monitor,
    Collector,
>(
    faults: &dyn UpgradeFaultInjector,
    registry: &mut Registry,
    services: ReadyServiceBundle<Startup, Persistence, Application, Monitor, Collector>,
) -> Result<(), RuntimeCompositionError>
where
    Registry: ReadyServiceRegistry,
    Startup: Send + Sync + 'static,
    Persistence: Send + Sync + 'static,
    Application: Send + Sync + 'static,
    Monitor: Send + Sync + 'static,
    Collector: Send + Sync + 'static,
{
    faults.check(UpgradeFailpoint::ServiceRegistration)?;
    if registry.contains::<Startup>()
        || registry.contains::<Persistence>()
        || registry.contains::<Application>()
        || registry.contains::<Monitor>()
        || registry.contains::<Collector>()
    {
        return Err(RuntimeCompositionError::StateSlotOccupied);
    }

    let ReadyServiceBundle {
        startup,
        persistence,
        application,
        monitor,
        collector,
    } = services;
    if !registry.manage(startup) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    if !registry.manage(persistence) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    if !registry.manage(application) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    if !registry.manage(monitor) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    if !registry.manage(collector) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    Ok(())
}

pub(crate) async fn drain_finalization<F>(
    faults: &dyn UpgradeFaultInjector,
    drain: F,
) -> Result<(), RuntimeCompositionError>
where
    F: Future<Output = Result<(), ()>>,
{
    faults.check(UpgradeFailpoint::FinalizationDrain)?;
    drain
        .await
        .map_err(|_| RuntimeCompositionError::FinalizationDrain)
}
