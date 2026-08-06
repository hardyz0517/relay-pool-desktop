use super::super::quality_projection::QualitySummary;

#[cfg(test)]
pub(crate) const RESPONSIVENESS_PROJECTOR_VERSION: &str = "responsiveness_axis_v1";

#[cfg(test)]
pub(crate) fn responsiveness_basis_points(summary: &QualitySummary, cap_ms: u32) -> u16 {
    let Some(latency) = summary.p95_latency_ms else {
        return 0;
    };
    if cap_ms == 0 {
        return 0;
    }
    let latency = u64::from(latency).min(u64::from(cap_ms));
    ((u64::from(cap_ms).saturating_sub(latency) * 10_000) / u64::from(cap_ms)) as u16
}
