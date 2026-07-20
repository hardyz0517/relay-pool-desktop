#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryTerminal {
    BodyCompleted,
    DownstreamDropped,
    DownstreamWriteFailed,
    CancelledByShutdown,
    NotStarted,
}
