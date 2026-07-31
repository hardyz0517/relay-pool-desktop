use std::fmt;

use serde::{Deserialize, Serialize};

use super::provenance::FactProvenance;

#[derive(Debug, Clone, PartialEq)]
pub enum EconomicsValidationError {
    InvalidCurrency(String),
    InvalidUnit(String),
    InvalidMoney(f64),
    InvalidMultiplier(f64),
    InvalidConfidence(f64),
}

impl fmt::Display for EconomicsValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCurrency(value) => write!(formatter, "unsupported currency code: {value}"),
            Self::InvalidUnit(value) => write!(formatter, "unsupported pricing unit: {value}"),
            Self::InvalidMoney(value) => write!(
                formatter,
                "money must be finite and non-negative, got {value}"
            ),
            Self::InvalidMultiplier(value) => {
                write!(
                    formatter,
                    "multiplier must be finite and positive, got {value}"
                )
            }
            Self::InvalidConfidence(value) => {
                write!(
                    formatter,
                    "confidence must be finite between 0 and 1, got {value}"
                )
            }
        }
    }
}

impl std::error::Error for EconomicsValidationError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    pub fn new(value: impl Into<String>) -> Result<Self, EconomicsValidationError> {
        let value = value.into();
        let known = matches!(
            value.as_str(),
            "USD" | "CNY" | "EUR" | "GBP" | "JPY" | "KRW" | "HKD" | "TWD" | "SGD" | "AUD" | "CAD"
        );
        if !known {
            return Err(EconomicsValidationError::InvalidCurrency(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PricingUnit {
    InputToken,
    OutputToken,
    Request,
}

impl TryFrom<&str> for PricingUnit {
    type Error = EconomicsValidationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "input_token" => Ok(Self::InputToken),
            "output_token" => Ok(Self::OutputToken),
            "request" => Ok(Self::Request),
            other => Err(EconomicsValidationError::InvalidUnit(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct MoneyAmount(f64);

impl MoneyAmount {
    pub fn new(value: f64) -> Result<Self, EconomicsValidationError> {
        if !value.is_finite() || value < 0.0 {
            return Err(EconomicsValidationError::InvalidMoney(value));
        }
        Ok(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Money {
    amount: MoneyAmount,
    currency: CurrencyCode,
}

impl Money {
    pub fn new(amount: MoneyAmount, currency: CurrencyCode) -> Self {
        Self { amount, currency }
    }

    pub fn amount(&self) -> MoneyAmount {
        self.amount
    }

    pub fn currency(&self) -> &CurrencyCode {
        &self.currency
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct RateMultiplier(f64);

impl RateMultiplier {
    pub fn new(value: f64) -> Result<Self, EconomicsValidationError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(EconomicsValidationError::InvalidMultiplier(value));
        }
        Ok(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PriceConfidence(f64);

impl PriceConfidence {
    pub fn new(value: f64) -> Result<Self, EconomicsValidationError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(EconomicsValidationError::InvalidConfidence(value));
        }
        Ok(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BalanceScope {
    StationAccount,
    StationKey,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceFacts {
    balance: Money,
    low_balance_threshold: Money,
    scope: BalanceScope,
    provenance: FactProvenance,
}

impl BalanceFacts {
    pub fn new(
        balance: Money,
        low_balance_threshold: Money,
        scope: BalanceScope,
        provenance: FactProvenance,
    ) -> Self {
        Self {
            balance,
            low_balance_threshold,
            scope,
            provenance,
        }
    }

    pub fn balance(&self) -> &Money {
        &self.balance
    }

    pub fn low_balance_threshold(&self) -> &Money {
        &self.low_balance_threshold
    }

    pub fn scope(&self) -> BalanceScope {
        self.scope
    }

    pub fn provenance(&self) -> &FactProvenance {
        &self.provenance
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestCostBasis {
    ExactUsagePrice,
    MultiplierProxy,
    Unpriced,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestPricingAssessment {
    basis: RequestCostBasis,
    unit: PricingUnit,
    multiplier: RateMultiplier,
    confidence: PriceConfidence,
    provenance: FactProvenance,
}

impl RequestPricingAssessment {
    pub fn new(
        basis: RequestCostBasis,
        unit: PricingUnit,
        multiplier: RateMultiplier,
        confidence: PriceConfidence,
        provenance: FactProvenance,
    ) -> Self {
        Self {
            basis,
            unit,
            multiplier,
            confidence,
            provenance,
        }
    }

    pub fn basis(&self) -> RequestCostBasis {
        self.basis
    }

    pub fn unit(&self) -> PricingUnit {
        self.unit
    }

    pub fn multiplier(&self) -> RateMultiplier {
        self.multiplier
    }

    pub fn confidence(&self) -> PriceConfidence {
        self.confidence
    }

    pub fn provenance(&self) -> &FactProvenance {
        &self.provenance
    }
}
