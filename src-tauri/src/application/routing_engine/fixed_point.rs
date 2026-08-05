use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct BasisPoints(u16);
impl BasisPoints { pub(crate) const ZERO: Self = Self(0); pub(crate) const FULL: Self = Self(10_000); pub(crate) const fn new(value: u16) -> Option<Self> { if value <= 10_000 { Some(Self(value)) } else { None } } pub(crate) const fn get(self) -> u16 { self.0 } pub(crate) fn checked_add(self, other: Self) -> Option<Self> { Self::new(self.0.checked_add(other.0)?) } pub(crate) fn checked_mul(self, other: Self) -> Option<Self> { let value = u32::from(self.0).checked_mul(u32::from(other.0))? / 10_000; Self::new(u16::try_from(value).ok()?) } pub(crate) fn weighted_average(values: impl IntoIterator<Item = (Self, Self)>) -> Option<Self> { let mut sum = 0_u64; let mut weights = 0_u64; for (value, weight) in values { sum = sum.checked_add(u64::from(value.0) * u64::from(weight.0))?; weights = weights.checked_add(u64::from(weight.0))?; } if weights == 0 { return Some(Self::ZERO); } Self::new(u16::try_from((sum + weights / 2) / weights).ok()?) } }
impl fmt::Display for BasisPoints { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}bp", self.0) } }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UtilityScore(BasisPoints);
impl UtilityScore { pub(crate) fn new(value: BasisPoints) -> Self { Self(value) } pub(crate) fn value(self) -> BasisPoints { self.0 } }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FactorScore { pub(crate) value: BasisPoints, pub(crate) confidence: BasisPoints }
impl FactorScore { pub(crate) fn effective(self) -> BasisPoints { self.value.checked_mul(self.confidence).unwrap_or(BasisPoints::ZERO) } }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FactorContribution { pub(crate) weight: BasisPoints, pub(crate) score: BasisPoints, pub(crate) contribution: BasisPoints }
#[cfg(test)]
mod tests { use super::*; #[test] fn fixed_point_is_bounded() { assert!(BasisPoints::new(10_001).is_none()); assert!(BasisPoints::new(10_000).unwrap().checked_add(BasisPoints::new(1).unwrap()).is_none()); assert_eq!(BasisPoints::weighted_average([(BasisPoints::new(1).unwrap(), BasisPoints::new(2).unwrap()), (BasisPoints::new(2).unwrap(), BasisPoints::new(1).unwrap())]).unwrap().get(), 1); } }
