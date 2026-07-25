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

#[allow(
    dead_code,
    reason = "legacy five-slot bundle remains for source-included persistence fault tests until every ready-service publisher moves to facade-aware bundles"
)]
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
    #[allow(
        dead_code,
        reason = "source-included fault tests use the legacy five-slot constructor while production publishes the first facade slot through the six-slot constructor"
    )]
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

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "source-included persistence tests do not publish the facade-aware production bundle"
    )
)]
pub(crate) struct ReadyServiceBundleWithCommandFacades<
    Startup,
    Persistence,
    Application,
    SettingsStations,
    KeyPool,
    Routing,
    RequestLogs,
    ChannelMonitoring,
    Pricing,
    ChangeEvents,
    Credentials,
    Monitor,
    Collector,
> {
    startup: Startup,
    persistence: Persistence,
    application: Application,
    settings_stations: SettingsStations,
    key_pool: KeyPool,
    routing: Routing,
    request_logs: RequestLogs,
    channel_monitoring: ChannelMonitoring,
    pricing: Pricing,
    change_events: ChangeEvents,
    credentials: Credentials,
    monitor: Monitor,
    collector: Collector,
}

impl<
        Startup,
        Persistence,
        Application,
        SettingsStations,
        KeyPool,
        Routing,
        RequestLogs,
        ChannelMonitoring,
        Pricing,
        ChangeEvents,
        Credentials,
        Monitor,
        Collector,
    >
    ReadyServiceBundleWithCommandFacades<
        Startup,
        Persistence,
        Application,
        SettingsStations,
        KeyPool,
        Routing,
        RequestLogs,
        ChannelMonitoring,
        Pricing,
        ChangeEvents,
        Credentials,
        Monitor,
        Collector,
    >
{
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "source-included persistence tests do not publish the facade-aware production bundle"
        )
    )]
    pub(crate) fn new(
        startup: Startup,
        persistence: Persistence,
        application: Application,
        settings_stations: SettingsStations,
        key_pool: KeyPool,
        routing: Routing,
        request_logs: RequestLogs,
        channel_monitoring: ChannelMonitoring,
        pricing: Pricing,
        change_events: ChangeEvents,
        credentials: Credentials,
        monitor: Monitor,
        collector: Collector,
    ) -> Self {
        Self {
            startup,
            persistence,
            application,
            settings_stations,
            key_pool,
            routing,
            request_logs,
            channel_monitoring,
            pricing,
            change_events,
            credentials,
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

#[allow(
    dead_code,
    reason = "legacy five-slot registration remains for source-included persistence fault tests until facade migration fully replaces AppServices command state"
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

#[allow(
    dead_code,
    reason = "source-included persistence fault tests exercise this five-slot registration path"
)]
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

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "source-included persistence tests do not publish the facade-aware production bundle"
    )
)]
pub(crate) fn register_ready_services_with_command_facades<
    R,
    Startup,
    Persistence,
    Application,
    SettingsStations,
    KeyPool,
    Routing,
    RequestLogs,
    ChannelMonitoring,
    Pricing,
    ChangeEvents,
    Credentials,
    Monitor,
    Collector,
>(
    faults: &dyn UpgradeFaultInjector,
    app: &mut tauri::App<R>,
    services: ReadyServiceBundleWithCommandFacades<
        Startup,
        Persistence,
        Application,
        SettingsStations,
        KeyPool,
        Routing,
        RequestLogs,
        ChannelMonitoring,
        Pricing,
        ChangeEvents,
        Credentials,
        Monitor,
        Collector,
    >,
) -> Result<(), RuntimeCompositionError>
where
    R: Runtime,
    Startup: Send + Sync + 'static,
    Persistence: Send + Sync + 'static,
    Application: Send + Sync + 'static,
    SettingsStations: Send + Sync + 'static,
    KeyPool: Send + Sync + 'static,
    Routing: Send + Sync + 'static,
    RequestLogs: Send + Sync + 'static,
    ChannelMonitoring: Send + Sync + 'static,
    Pricing: Send + Sync + 'static,
    ChangeEvents: Send + Sync + 'static,
    Credentials: Send + Sync + 'static,
    Monitor: Send + Sync + 'static,
    Collector: Send + Sync + 'static,
{
    let mut registry = TauriReadyServiceRegistry(app);
    register_ready_services_with_command_facades_in(faults, &mut registry, services)
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "source-included persistence tests do not publish the facade-aware production bundle"
    )
)]
pub(crate) fn register_ready_services_with_command_facades_in<
    Registry,
    Startup,
    Persistence,
    Application,
    SettingsStations,
    KeyPool,
    Routing,
    RequestLogs,
    ChannelMonitoring,
    Pricing,
    ChangeEvents,
    Credentials,
    Monitor,
    Collector,
>(
    faults: &dyn UpgradeFaultInjector,
    registry: &mut Registry,
    services: ReadyServiceBundleWithCommandFacades<
        Startup,
        Persistence,
        Application,
        SettingsStations,
        KeyPool,
        Routing,
        RequestLogs,
        ChannelMonitoring,
        Pricing,
        ChangeEvents,
        Credentials,
        Monitor,
        Collector,
    >,
) -> Result<(), RuntimeCompositionError>
where
    Registry: ReadyServiceRegistry,
    Startup: Send + Sync + 'static,
    Persistence: Send + Sync + 'static,
    Application: Send + Sync + 'static,
    SettingsStations: Send + Sync + 'static,
    KeyPool: Send + Sync + 'static,
    Routing: Send + Sync + 'static,
    RequestLogs: Send + Sync + 'static,
    ChannelMonitoring: Send + Sync + 'static,
    Pricing: Send + Sync + 'static,
    ChangeEvents: Send + Sync + 'static,
    Credentials: Send + Sync + 'static,
    Monitor: Send + Sync + 'static,
    Collector: Send + Sync + 'static,
{
    faults.check(UpgradeFailpoint::ServiceRegistration)?;
    if registry.contains::<Startup>()
        || registry.contains::<Persistence>()
        || registry.contains::<Application>()
        || registry.contains::<SettingsStations>()
        || registry.contains::<KeyPool>()
        || registry.contains::<Routing>()
        || registry.contains::<RequestLogs>()
        || registry.contains::<ChannelMonitoring>()
        || registry.contains::<Pricing>()
        || registry.contains::<ChangeEvents>()
        || registry.contains::<Credentials>()
        || registry.contains::<Monitor>()
        || registry.contains::<Collector>()
    {
        return Err(RuntimeCompositionError::StateSlotOccupied);
    }

    let ReadyServiceBundleWithCommandFacades {
        startup,
        persistence,
        application,
        settings_stations,
        key_pool,
        routing,
        request_logs,
        channel_monitoring,
        pricing,
        change_events,
        credentials,
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
    if !registry.manage(settings_stations) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    if !registry.manage(key_pool) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    if !registry.manage(routing) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    if !registry.manage(request_logs) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    if !registry.manage(channel_monitoring) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    if !registry.manage(pricing) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    if !registry.manage(change_events) {
        return Err(RuntimeCompositionError::ServiceRegistration);
    }
    if !registry.manage(credentials) {
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

#[cfg(test)]
mod tests {
    use std::{
        any::{Any, TypeId},
        collections::HashMap,
    };

    use crate::persistence::upgrade_fault::NoUpgradeFaults;

    use super::{
        register_ready_services_with_command_facades_in, ReadyServiceBundleWithCommandFacades,
        ReadyServiceRegistry, RuntimeCompositionError,
    };

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotOne(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotTwo(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotThree(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotFour(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotFive(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotSix(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotSeven(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotEight(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotNine(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotTen(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotEleven(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotTwelve(u8);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SlotThirteen(u8);

    #[derive(Default)]
    struct TestRegistry {
        states: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    }

    impl TestRegistry {
        fn manage_direct<T: Send + Sync + 'static>(&mut self, state: T) -> bool {
            let type_id = TypeId::of::<T>();
            if self.states.contains_key(&type_id) {
                return false;
            }
            self.states.insert(type_id, Box::new(state));
            true
        }

        fn try_state<T: Copy + Send + Sync + 'static>(&self) -> Option<T> {
            self.states
                .get(&TypeId::of::<T>())
                .and_then(|state| state.downcast_ref::<T>())
                .copied()
        }
    }

    impl ReadyServiceRegistry for TestRegistry {
        fn contains<T: Send + Sync + 'static>(&self) -> bool {
            self.states.contains_key(&TypeId::of::<T>())
        }

        fn manage<T: Send + Sync + 'static>(&mut self, state: T) -> bool {
            self.manage_direct(state)
        }
    }

    #[test]
    fn command_facade_ready_services_preflight_every_concrete_slot() {
        for occupied_slot in 0..13 {
            let mut registry = TestRegistry::default();
            match occupied_slot {
                0 => assert!(registry.manage_direct(SlotOne(99))),
                1 => assert!(registry.manage_direct(SlotTwo(99))),
                2 => assert!(registry.manage_direct(SlotThree(99))),
                3 => assert!(registry.manage_direct(SlotFour(99))),
                4 => assert!(registry.manage_direct(SlotFive(99))),
                5 => assert!(registry.manage_direct(SlotSix(99))),
                6 => assert!(registry.manage_direct(SlotSeven(99))),
                7 => assert!(registry.manage_direct(SlotEight(99))),
                8 => assert!(registry.manage_direct(SlotNine(99))),
                9 => assert!(registry.manage_direct(SlotTen(99))),
                10 => assert!(registry.manage_direct(SlotEleven(99))),
                11 => assert!(registry.manage_direct(SlotTwelve(99))),
                12 => assert!(registry.manage_direct(SlotThirteen(99))),
                _ => unreachable!(),
            }

            let error = register_ready_services_with_command_facades_in(
                &NoUpgradeFaults,
                &mut registry,
                ReadyServiceBundleWithCommandFacades::new(
                    SlotOne(1),
                    SlotTwo(1),
                    SlotThree(1),
                    SlotFour(1),
                    SlotFive(1),
                    SlotSix(1),
                    SlotSeven(1),
                    SlotEight(1),
                    SlotNine(1),
                    SlotTen(1),
                    SlotEleven(1),
                    SlotTwelve(1),
                    SlotThirteen(1),
                ),
            )
            .expect_err("occupied slots must fail before publishing any new ready state");

            assert_eq!(error, RuntimeCompositionError::StateSlotOccupied);
            let observed = [
                registry.try_state::<SlotOne>().map(|state| state.0),
                registry.try_state::<SlotTwo>().map(|state| state.0),
                registry.try_state::<SlotThree>().map(|state| state.0),
                registry.try_state::<SlotFour>().map(|state| state.0),
                registry.try_state::<SlotFive>().map(|state| state.0),
                registry.try_state::<SlotSix>().map(|state| state.0),
                registry.try_state::<SlotSeven>().map(|state| state.0),
                registry.try_state::<SlotEight>().map(|state| state.0),
                registry.try_state::<SlotNine>().map(|state| state.0),
                registry.try_state::<SlotTen>().map(|state| state.0),
                registry.try_state::<SlotEleven>().map(|state| state.0),
                registry.try_state::<SlotTwelve>().map(|state| state.0),
                registry.try_state::<SlotThirteen>().map(|state| state.0),
            ];
            let expected = std::array::from_fn(|index| (index == occupied_slot).then_some(99));
            assert_eq!(observed, expected);
        }
    }

    #[test]
    fn command_facade_ready_services_publish_complete_bundle() {
        let mut registry = TestRegistry::default();

        register_ready_services_with_command_facades_in(
            &NoUpgradeFaults,
            &mut registry,
            ReadyServiceBundleWithCommandFacades::new(
                SlotOne(1),
                SlotTwo(2),
                SlotThree(3),
                SlotFour(4),
                SlotFive(5),
                SlotSix(6),
                SlotSeven(7),
                SlotEight(8),
                SlotNine(9),
                SlotTen(10),
                SlotEleven(11),
                SlotTwelve(12),
                SlotThirteen(13),
            ),
        )
        .expect("vacant registry must publish every ready state");

        assert_eq!(registry.try_state::<SlotOne>(), Some(SlotOne(1)));
        assert_eq!(registry.try_state::<SlotTwo>(), Some(SlotTwo(2)));
        assert_eq!(registry.try_state::<SlotThree>(), Some(SlotThree(3)));
        assert_eq!(registry.try_state::<SlotFour>(), Some(SlotFour(4)));
        assert_eq!(registry.try_state::<SlotFive>(), Some(SlotFive(5)));
        assert_eq!(registry.try_state::<SlotSix>(), Some(SlotSix(6)));
        assert_eq!(registry.try_state::<SlotSeven>(), Some(SlotSeven(7)));
        assert_eq!(registry.try_state::<SlotEight>(), Some(SlotEight(8)));
        assert_eq!(registry.try_state::<SlotNine>(), Some(SlotNine(9)));
        assert_eq!(registry.try_state::<SlotTen>(), Some(SlotTen(10)));
        assert_eq!(registry.try_state::<SlotEleven>(), Some(SlotEleven(11)));
        assert_eq!(registry.try_state::<SlotTwelve>(), Some(SlotTwelve(12)));
        assert_eq!(registry.try_state::<SlotThirteen>(), Some(SlotThirteen(13)));
    }
}
