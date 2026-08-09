//! Delivery persistence is intentionally kept behind the alerting store module.
//! The worker owns claim/retry semantics; this file is the stable boundary for
//! the eventual SQL implementation.
pub(crate) use super::DeliveryStore;
