//! Scenario contracts for the production runtime-composition boundary.

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Mutex,
};

use crate::{
    persistence::upgrade_fault::{
        NoUpgradeFaults, UpgradeFailpoint, UpgradeFaultInjector, UpgradeInjectedFailure,
    },
    runtime_composition::{
        drain_finalization, register_ready_services_with_command_facades_in,
        ReadyServiceBundleWithCommandFacades, ReadyServiceRegistry, RuntimeCompositionError,
    },
};

macro_rules! define_slots {
    ($($name:ident),+ $(,)?) => {
        $(#[derive(Clone, Copy, Debug, PartialEq, Eq)] struct $name(u8);)+
    };
}

define_slots!(
    Slot01, Slot02, Slot03, Slot04, Slot05, Slot06, Slot07, Slot08, Slot09, Slot10, Slot11, Slot12,
    Slot13, Slot14, Slot15, Slot16, Slot17, Slot18, Slot19, Slot20, Slot21,
);

#[derive(Default)]
struct TestRegistry {
    states: Mutex<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl TestRegistry {
    fn occupy<T: Send + Sync + 'static>(&self, state: T) -> bool {
        let mut states = self.states.lock().expect("state registry poisoned");
        let type_id = TypeId::of::<T>();
        if states.contains_key(&type_id) {
            return false;
        }
        states.insert(type_id, Box::new(state));
        true
    }

    fn value<T: Copy + Send + Sync + 'static>(&self) -> Option<T> {
        self.states
            .lock()
            .expect("state registry poisoned")
            .get(&TypeId::of::<T>())
            .and_then(|state| state.downcast_ref::<T>())
            .copied()
    }
}

impl ReadyServiceRegistry for TestRegistry {
    fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.states
            .lock()
            .expect("state registry poisoned")
            .contains_key(&TypeId::of::<T>())
    }

    fn manage<T: Send + Sync + 'static>(&mut self, state: T) -> bool {
        self.occupy(state)
    }
}

struct OneShotFault {
    target: UpgradeFailpoint,
    fired: Mutex<bool>,
}

impl OneShotFault {
    fn new(target: UpgradeFailpoint) -> Self {
        Self {
            target,
            fired: Mutex::new(false),
        }
    }
}

impl UpgradeFaultInjector for OneShotFault {
    fn check(&self, failpoint: UpgradeFailpoint) -> Result<(), UpgradeInjectedFailure> {
        let mut fired = self.fired.lock().expect("fault mutex poisoned");
        if !*fired && failpoint == self.target {
            *fired = true;
            return Err(UpgradeInjectedFailure::new(failpoint));
        }
        Ok(())
    }
}

pub fn registration_fault_publishes_nothing() {
    let mut registry = TestRegistry::default();
    let error = register_ready_services_with_command_facades_in(
        &OneShotFault::new(UpgradeFailpoint::ServiceRegistration),
        &mut registry,
        bundle(1),
    )
    .expect_err("injected registration must fail closed");

    assert_eq!(
        error,
        RuntimeCompositionError::Injected(UpgradeInjectedFailure::new(
            UpgradeFailpoint::ServiceRegistration,
        ))
    );
    assert_all_slots(&registry, |_| None);
    assert_bounded_diagnostic(&error);
}

pub fn occupied_slot_never_causes_partial_publication() {
    for occupied in 0..21 {
        let mut registry = TestRegistry::default();
        occupy_slot(&registry, occupied, 99);

        let error = register_ready_services_with_command_facades_in(
            &NoUpgradeFaults,
            &mut registry,
            bundle(1),
        )
        .expect_err("occupied slot must fail closed");

        assert_eq!(error, RuntimeCompositionError::StateSlotOccupied);
        assert_all_slots(&registry, |index| (index == occupied).then_some(99));
    }
}

pub fn vacant_registry_publishes_complete_bundle() {
    let mut registry = TestRegistry::default();
    register_ready_services_with_command_facades_in(&NoUpgradeFaults, &mut registry, bundle(1))
        .expect("vacant registry must publish the complete bundle");
    assert_all_slots(&registry, |_| Some(1));
}

pub async fn finalization_drain_fault_does_not_poll_work() {
    let polled = std::cell::Cell::new(false);
    let error = drain_finalization(
        &OneShotFault::new(UpgradeFailpoint::FinalizationDrain),
        async {
            polled.set(true);
            Ok(())
        },
    )
    .await
    .expect_err("injected drain must fail closed");

    assert_eq!(
        error,
        RuntimeCompositionError::Injected(UpgradeInjectedFailure::new(
            UpgradeFailpoint::FinalizationDrain,
        ))
    );
    assert!(
        !polled.get(),
        "injected drain must not poll the work future"
    );
    assert_bounded_diagnostic(&error);
}

fn bundle(
    value: u8,
) -> ReadyServiceBundleWithCommandFacades<
    Slot01,
    Slot02,
    Slot03,
    Slot04,
    Slot05,
    Slot06,
    Slot07,
    Slot08,
    Slot09,
    Slot10,
    Slot11,
    Slot12,
    Slot13,
    Slot14,
    Slot15,
    Slot16,
    Slot17,
    Slot18,
    Slot19,
    Slot20,
    Slot21,
> {
    ReadyServiceBundleWithCommandFacades::new(
        Slot01(value),
        Slot02(value),
        Slot03(value),
        Slot04(value),
        Slot05(value),
        Slot06(value),
        Slot07(value),
        Slot08(value),
        Slot09(value),
        Slot10(value),
        Slot11(value),
        Slot12(value),
        Slot13(value),
        Slot14(value),
        Slot15(value),
        Slot16(value),
        Slot17(value),
        Slot18(value),
        Slot19(value),
        Slot20(value),
        Slot21(value),
    )
}

fn occupy_slot(registry: &TestRegistry, index: usize, value: u8) {
    let occupied = match index {
        0 => registry.occupy(Slot01(value)),
        1 => registry.occupy(Slot02(value)),
        2 => registry.occupy(Slot03(value)),
        3 => registry.occupy(Slot04(value)),
        4 => registry.occupy(Slot05(value)),
        5 => registry.occupy(Slot06(value)),
        6 => registry.occupy(Slot07(value)),
        7 => registry.occupy(Slot08(value)),
        8 => registry.occupy(Slot09(value)),
        9 => registry.occupy(Slot10(value)),
        10 => registry.occupy(Slot11(value)),
        11 => registry.occupy(Slot12(value)),
        12 => registry.occupy(Slot13(value)),
        13 => registry.occupy(Slot14(value)),
        14 => registry.occupy(Slot15(value)),
        15 => registry.occupy(Slot16(value)),
        16 => registry.occupy(Slot17(value)),
        17 => registry.occupy(Slot18(value)),
        18 => registry.occupy(Slot19(value)),
        19 => registry.occupy(Slot20(value)),
        20 => registry.occupy(Slot21(value)),
        _ => unreachable!(),
    };
    assert!(occupied);
}

fn assert_all_slots(registry: &TestRegistry, expected: impl Fn(usize) -> Option<u8>) {
    let observed = [
        registry.value::<Slot01>().map(|v| v.0),
        registry.value::<Slot02>().map(|v| v.0),
        registry.value::<Slot03>().map(|v| v.0),
        registry.value::<Slot04>().map(|v| v.0),
        registry.value::<Slot05>().map(|v| v.0),
        registry.value::<Slot06>().map(|v| v.0),
        registry.value::<Slot07>().map(|v| v.0),
        registry.value::<Slot08>().map(|v| v.0),
        registry.value::<Slot09>().map(|v| v.0),
        registry.value::<Slot10>().map(|v| v.0),
        registry.value::<Slot11>().map(|v| v.0),
        registry.value::<Slot12>().map(|v| v.0),
        registry.value::<Slot13>().map(|v| v.0),
        registry.value::<Slot14>().map(|v| v.0),
        registry.value::<Slot15>().map(|v| v.0),
        registry.value::<Slot16>().map(|v| v.0),
        registry.value::<Slot17>().map(|v| v.0),
        registry.value::<Slot18>().map(|v| v.0),
        registry.value::<Slot19>().map(|v| v.0),
        registry.value::<Slot20>().map(|v| v.0),
        registry.value::<Slot21>().map(|v| v.0),
    ];
    assert_eq!(observed, std::array::from_fn(expected));
}

fn assert_bounded_diagnostic(error: &RuntimeCompositionError) {
    let diagnostic = error.to_string();
    assert!(!diagnostic.contains("sk-"), "secret-like value leaked");
    assert!(
        !diagnostic.contains(':') && !diagnostic.contains('\\'),
        "path leaked"
    );
    assert!(diagnostic.starts_with("persistence_upgrade_fault_injected at runtime."));
}
