#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DispatchDecision {
    pub(crate) selected_id: String,
    pub(crate) band_size: usize,
    pub(crate) explored: bool,
    pub(crate) seed_commitment: String,
}
