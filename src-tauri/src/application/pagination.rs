use crate::application::error::ApplicationError;

pub(crate) const MAX_PAGE_LIMIT: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageLimit(u32);

impl PageLimit {
    pub(crate) fn new(value: u32) -> Result<Self, ApplicationError> {
        if !(1..=MAX_PAGE_LIMIT).contains(&value) {
            return Err(ApplicationError::ConstraintViolation);
        }
        Ok(Self(value))
    }

    pub(crate) fn get(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_limit_rejects_unbounded_values() {
        assert!(PageLimit::new(0).is_err());
        assert_eq!(PageLimit::new(1).expect("minimum").get(), 1);
        assert_eq!(PageLimit::new(500).expect("maximum").get(), 500);
        assert!(PageLimit::new(501).is_err());
    }
}
