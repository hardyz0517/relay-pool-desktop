#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "path-included integration contracts exercise disjoint delivery terminal variants"
    )
)]
pub(crate) enum DeliveryTerminal {
    BodyCompleted,
    DownstreamDropped,
    #[cfg(test)]
    DownstreamWriteFailed,
    NotStarted,
}
