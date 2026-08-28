pub mod attention;
pub mod delivery;
pub mod event;
pub mod incident;
pub mod occurrence;
pub mod policy;

pub use attention::IncidentAttention;
pub use delivery::{
    make_delivery_key, DeliveryKind, NotificationChannel, NotificationDelivery, SuppressionReason,
};
pub use event::{AlertEventType, ConditionKey, EventCategory, ObservationKind, Severity};
pub use incident::{Incident, IncidentObservation, LifecycleState, StateTransition};
pub use policy::{
    resolve_policy, AlertPolicy, PolicyMatchContext, PolicyState, QuietHoursPolicy, RecoveryMode,
    RepeatMode, ScopeKind, TriggerMode,
};
