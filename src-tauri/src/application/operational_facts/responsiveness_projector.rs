use super::super::quality_projection::QualitySummary;

#[cfg(test)]
pub(crate) const RESPONSIVENESS_PROJECTOR_VERSION: &str = "responsiveness_axis_v1";

#[cfg(test)]
pub(crate) fn responsiveness_basis_points(summary: &QualitySummary, _cap_ms: u32) -> u16 {
    // The quality projector owns the recent/history blend. Keep this legacy
    // test-facing adapter aligned with that canonical output instead of
    // reintroducing a single-window P95 calculation.
    summary.responsiveness_basis_points
}
