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

impl DeliveryTerminal {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::BodyCompleted => "body_completed",
            Self::DownstreamDropped => "downstream_dropped",
            #[cfg(test)]
            Self::DownstreamWriteFailed => "downstream_write_failed",
            Self::NotStarted => "not_started",
        }
    }
}
