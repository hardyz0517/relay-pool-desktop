#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RequestSendPhase {
    NotConnected,
    #[cfg(test)]
    ConnectedNoHeaders,
    #[cfg(test)]
    HeadersSent,
    #[cfg(test)]
    BodyPartiallySent,
    #[cfg(test)]
    BodyFullySent,
    ResponseStarted,
    Unknown,
}

impl RequestSendPhase {
    pub(crate) const fn definitely_no_request_bytes_sent(self) -> bool {
        if matches!(self, Self::NotConnected) {
            return true;
        }
        #[cfg(test)]
        {
            return matches!(self, Self::ConnectedNoHeaders);
        }
        #[cfg(not(test))]
        false
    }
}
