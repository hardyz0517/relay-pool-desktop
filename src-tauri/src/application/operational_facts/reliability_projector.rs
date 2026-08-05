use super::super::quality_projection::QualitySummary;

pub(crate) const RELIABILITY_PROJECTOR_VERSION: &str = "reliability_axis_v1";

pub(crate) fn reliability_basis_points(summary: &QualitySummary) -> u16 {
    summary.reliability_basis_points
}
