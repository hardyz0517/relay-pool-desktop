#![allow(dead_code)]

#[path = "../src/models/operational/mod.rs"]
mod operational_model;

mod models {
    pub(crate) mod operational {
        pub(crate) use crate::operational_model::*;
    }
}

#[path = "../src/application/operational_facts/balance_projector.rs"]
mod balance_projector;
#[path = "../src/application/operational_facts/group_projector.rs"]
mod group_projector;
#[path = "../src/application/operational_facts/multiplier_projector.rs"]
mod multiplier_projector;

use balance_projector::{
    project_balance, BalanceEvidenceStatus, BalanceObservation, BalanceProjectionStatus,
};
use group_projector::{
    project_group, GroupIdentity, GroupProjectionInput, GroupStatus, ProjectionTrace,
};
use multiplier_projector::{
    project_multiplier, MultiplierEvidence, MultiplierEvidenceKind, MultiplierProjectionInput,
    MultiplierResolutionStatus,
};
use operational_model::{
    BalanceScope, CurrencyCode, Money, MoneyAmount, PriceConfidence, RateMultiplier,
    RecordRevision, UnixMillis,
};

fn revision(value: i64) -> RecordRevision {
    RecordRevision::new(value).expect("revision")
}

fn now() -> UnixMillis {
    UnixMillis::new(1_000).expect("now")
}

fn confidence(value: f64) -> PriceConfidence {
    PriceConfidence::new(value).expect("confidence")
}

fn trace(reason: &'static str) -> ProjectionTrace {
    ProjectionTrace::new(
        vec!["test"],
        confidence(1.0),
        now(),
        reason,
        vec![revision(1)],
    )
}

fn multiplier(kind: MultiplierEvidenceKind, value: f64, fresh: bool) -> MultiplierEvidence {
    MultiplierEvidence {
        kind,
        multiplier: RateMultiplier::new(value).expect("multiplier"),
        authoritative: true,
        fresh,
        revision: revision(10),
    }
}

fn money(value: f64) -> Money {
    Money::new(
        MoneyAmount::new(value).expect("money"),
        CurrencyCode::new("USD").expect("currency"),
    )
}

fn balance(
    scope: BalanceScope,
    status: BalanceEvidenceStatus,
    value: Option<f64>,
    threshold: Option<f64>,
    fresh: bool,
    revision_value: i64,
) -> BalanceObservation {
    BalanceObservation {
        scope,
        status,
        balance: value.map(money),
        low_balance_threshold: threshold.map(money),
        authoritative: true,
        fresh,
        revision: revision(revision_value),
    }
}

#[test]
fn group_identity_prefers_binding_then_group_key_then_group_id_then_legacy_name() {
    let with_all = project_group(GroupProjectionInput {
        group_binding_id: Some("binding-a".to_string()),
        group_key_hash: Some("local-hash".to_string()),
        group_id_hash: Some("remote-hash".to_string()),
        group_name: Some("Shared Name".to_string()),
        status: GroupStatus::Available,
        trace: trace("binding"),
    })
    .expect("group");
    assert_eq!(
        with_all.identity,
        GroupIdentity::BindingId("binding-a".to_string())
    );
    assert_eq!(with_all.identity.stable_key(), "binding:binding-a");

    let key_hash = project_group(GroupProjectionInput {
        group_binding_id: None,
        group_key_hash: Some("local-hash".to_string()),
        group_id_hash: Some("remote-hash".to_string()),
        group_name: Some("Shared Name".to_string()),
        status: GroupStatus::Available,
        trace: trace("key-hash"),
    })
    .expect("group");
    assert_eq!(key_hash.identity.stable_key(), "group-key:local-hash");

    let id_hash = project_group(GroupProjectionInput {
        group_binding_id: None,
        group_key_hash: None,
        group_id_hash: Some("remote-hash".to_string()),
        group_name: Some("Shared Name".to_string()),
        status: GroupStatus::Available,
        trace: trace("id-hash"),
    })
    .expect("group");
    assert_eq!(id_hash.identity.stable_key(), "group-id:remote-hash");

    let legacy = project_group(GroupProjectionInput {
        group_binding_id: None,
        group_key_hash: None,
        group_id_hash: None,
        group_name: Some(" Shared   Name ".to_string()),
        status: GroupStatus::Available,
        trace: trace("legacy"),
    })
    .expect("group");
    assert_eq!(legacy.identity.stable_key(), "legacy-name:shared name");
}

#[test]
fn group_hashes_are_not_interchangeable_even_when_values_match_display_names() {
    let local = project_group(GroupProjectionInput {
        group_binding_id: None,
        group_key_hash: Some("same-value".to_string()),
        group_id_hash: None,
        group_name: Some("same-value".to_string()),
        status: GroupStatus::Available,
        trace: trace("local"),
    })
    .expect("group");
    let remote = project_group(GroupProjectionInput {
        group_binding_id: None,
        group_key_hash: None,
        group_id_hash: Some("same-value".to_string()),
        group_name: Some("same-value".to_string()),
        status: GroupStatus::Available,
        trace: trace("remote"),
    })
    .expect("group");

    assert_ne!(local.identity, remote.identity);
    assert_eq!(local.identity.stable_key(), "group-key:same-value");
    assert_eq!(remote.identity.stable_key(), "group-id:same-value");
}

#[test]
fn multiplier_manual_override_beats_latest_and_current_values() {
    let projection = project_multiplier(MultiplierProjectionInput {
        disabled: false,
        ambiguous: false,
        manual_override: Some(multiplier(
            MultiplierEvidenceKind::ManualOverride,
            0.5,
            true,
        )),
        binding_latest_user: Some(multiplier(
            MultiplierEvidenceKind::BindingLatestUser,
            0.6,
            true,
        )),
        binding_latest_effective: Some(multiplier(
            MultiplierEvidenceKind::BindingLatestEffective,
            0.7,
            true,
        )),
        current_user: Some(multiplier(MultiplierEvidenceKind::CurrentUser, 0.8, true)),
        current_effective: Some(multiplier(
            MultiplierEvidenceKind::CurrentEffective,
            0.9,
            true,
        )),
        current_default: Some(multiplier(
            MultiplierEvidenceKind::CurrentDefault,
            1.0,
            true,
        )),
        resolved_at: now(),
    });

    assert_eq!(projection.status, MultiplierResolutionStatus::Resolved);
    assert_eq!(
        projection.selected_kind,
        Some(MultiplierEvidenceKind::ManualOverride)
    );
    assert_eq!(projection.multiplier.expect("value").get(), 0.5);
}

#[test]
fn multiplier_uses_documented_latest_user_effective_default_fallback_order() {
    let projection = project_multiplier(MultiplierProjectionInput {
        disabled: false,
        ambiguous: false,
        manual_override: None,
        binding_latest_user: None,
        binding_latest_effective: Some(multiplier(
            MultiplierEvidenceKind::BindingLatestEffective,
            0.7,
            true,
        )),
        current_user: Some(multiplier(MultiplierEvidenceKind::CurrentUser, 0.8, true)),
        current_effective: Some(multiplier(
            MultiplierEvidenceKind::CurrentEffective,
            0.9,
            true,
        )),
        current_default: Some(multiplier(
            MultiplierEvidenceKind::CurrentDefault,
            1.0,
            true,
        )),
        resolved_at: now(),
    });

    assert_eq!(
        projection.selected_kind,
        Some(MultiplierEvidenceKind::BindingLatestEffective)
    );
    assert_eq!(projection.multiplier.expect("value").get(), 0.7);
}

#[test]
fn multiplier_missing_stale_untrusted_disabled_and_ambiguous_fail_closed() {
    let missing = project_multiplier(MultiplierProjectionInput {
        disabled: false,
        ambiguous: false,
        manual_override: None,
        binding_latest_user: None,
        binding_latest_effective: None,
        current_user: None,
        current_effective: None,
        current_default: None,
        resolved_at: now(),
    });
    assert_eq!(missing.status, MultiplierResolutionStatus::Missing);
    assert!(missing.multiplier.is_none());

    let stale = project_multiplier(MultiplierProjectionInput {
        disabled: false,
        ambiguous: false,
        manual_override: None,
        binding_latest_user: Some(multiplier(
            MultiplierEvidenceKind::BindingLatestUser,
            1.0,
            false,
        )),
        binding_latest_effective: None,
        current_user: None,
        current_effective: None,
        current_default: None,
        resolved_at: now(),
    });
    assert_eq!(stale.status, MultiplierResolutionStatus::Stale);
    assert!(stale.multiplier.is_none());

    let mut untrusted = multiplier(MultiplierEvidenceKind::CurrentDefault, 1.0, true);
    untrusted.authoritative = false;
    let untrusted_projection = project_multiplier(MultiplierProjectionInput {
        disabled: false,
        ambiguous: false,
        manual_override: None,
        binding_latest_user: None,
        binding_latest_effective: None,
        current_user: None,
        current_effective: None,
        current_default: Some(untrusted),
        resolved_at: now(),
    });
    assert_eq!(
        untrusted_projection.status,
        MultiplierResolutionStatus::Untrusted
    );
    assert!(untrusted_projection.multiplier.is_none());

    let disabled = project_multiplier(MultiplierProjectionInput {
        disabled: true,
        ambiguous: false,
        manual_override: Some(multiplier(
            MultiplierEvidenceKind::ManualOverride,
            0.5,
            true,
        )),
        binding_latest_user: None,
        binding_latest_effective: None,
        current_user: None,
        current_effective: None,
        current_default: None,
        resolved_at: now(),
    });
    assert_eq!(disabled.status, MultiplierResolutionStatus::Disabled);
    assert!(disabled.multiplier.is_none());

    let ambiguous = project_multiplier(MultiplierProjectionInput {
        disabled: false,
        ambiguous: true,
        manual_override: Some(multiplier(
            MultiplierEvidenceKind::ManualOverride,
            0.5,
            true,
        )),
        binding_latest_user: None,
        binding_latest_effective: None,
        current_user: None,
        current_effective: None,
        current_default: None,
        resolved_at: now(),
    });
    assert_eq!(ambiguous.status, MultiplierResolutionStatus::Ambiguous);
    assert!(ambiguous.multiplier.is_none());
}

#[test]
fn balance_key_scope_and_station_scope_do_not_override_each_other_by_timestamp() {
    let projection = project_balance(
        Some(balance(
            BalanceScope::StationKey,
            BalanceEvidenceStatus::Available,
            Some(100.0),
            Some(10.0),
            true,
            1,
        )),
        Some(balance(
            BalanceScope::StationAccount,
            BalanceEvidenceStatus::Available,
            Some(1.0),
            Some(10.0),
            true,
            99,
        )),
        now(),
    );

    assert_eq!(projection.status, BalanceProjectionStatus::Healthy);
    assert_eq!(projection.selected_scope, Some(BalanceScope::StationKey));
    assert_eq!(projection.trace.revision_refs, vec![revision(1)]);
}

#[test]
fn balance_unknown_not_supported_and_not_applicable_are_not_depleted() {
    for (status, expected) in [
        (
            BalanceEvidenceStatus::Unknown,
            BalanceProjectionStatus::Unknown,
        ),
        (
            BalanceEvidenceStatus::NotSupported,
            BalanceProjectionStatus::NotSupported,
        ),
        (
            BalanceEvidenceStatus::NotApplicable,
            BalanceProjectionStatus::NotApplicable,
        ),
    ] {
        let projection = project_balance(
            Some(balance(
                BalanceScope::StationKey,
                status,
                Some(0.0),
                Some(10.0),
                true,
                1,
            )),
            None,
            now(),
        );
        assert_eq!(projection.status, expected);
    }
}

#[test]
fn balance_depleted_emergency_requires_authoritative_scope_matched_fresh_evidence() {
    let depleted = project_balance(
        Some(balance(
            BalanceScope::StationKey,
            BalanceEvidenceStatus::Available,
            Some(1.0),
            Some(10.0),
            true,
            1,
        )),
        None,
        now(),
    );
    assert_eq!(depleted.status, BalanceProjectionStatus::DepletedEmergency);

    let stale = project_balance(
        Some(balance(
            BalanceScope::StationKey,
            BalanceEvidenceStatus::Available,
            Some(1.0),
            Some(10.0),
            false,
            1,
        )),
        None,
        now(),
    );
    assert_eq!(stale.status, BalanceProjectionStatus::Stale);

    let mut untrusted = balance(
        BalanceScope::StationKey,
        BalanceEvidenceStatus::Available,
        Some(1.0),
        Some(10.0),
        true,
        1,
    );
    untrusted.authoritative = false;
    let untrusted_projection = project_balance(Some(untrusted), None, now());
    assert_eq!(
        untrusted_projection.status,
        BalanceProjectionStatus::Untrusted
    );
}
