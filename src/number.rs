//! Arbitrary precision number representation for HCL.
use bigdecimal::BigDecimal;
use bigdecimal::ToPrimitive;
use std::str::FromStr;
/// HCL Number type using arbitrary precision to avoid f64 loss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Number(pub BigDecimal);
impl Number {
    /// Create a new `Number` from a `BigDecimal`.
    #[must_use]
    pub fn new(val: BigDecimal) -> Self {
        Self(val)
    }
    /// Convert to f64 for testing purposes.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        self.0.to_f64()
    }
    /// Converts the number to an `i64` if it can be represented as such.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        self.0.to_i64()
    }
    /// Computes `self / rhs`, returning `None` if `rhs` is zero.
    ///
    /// # Arguments
    /// * `rhs` - The divisor to divide by.
    #[must_use]
    pub fn checked_div(&self, rhs: &Self) -> Option<Self> {
        if rhs.0 == 0 {
            None
        } else {
            Some(Self(&self.0 / &rhs.0))
        }
    }
    /// Computes `self % rhs`, returning `None` if `rhs` is zero.
    ///
    /// # Arguments
    /// * `rhs` - The divisor for the remainder operation.
    #[must_use]
    pub fn checked_rem(&self, rhs: &Self) -> Option<Self> {
        if rhs.0 == 0 {
            None
        } else {
            Some(Self(&self.0 % &rhs.0))
        }
    }
}
impl std::ops::Add for Number {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}
impl std::ops::Sub for Number {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}
impl std::ops::Mul for Number {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}
impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Number {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}
impl FromStr for Number {
    type Err = bigdecimal::ParseBigDecimalError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BigDecimal::from_str(s).map(Self)
    }
}
impl std::fmt::Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::ops::Neg for Number {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Number(-self.0)
    }
}
impl From<i64> for Number {
    fn from(val: i64) -> Self {
        Self(BigDecimal::from(val))
    }
}
impl From<i32> for Number {
    fn from(val: i32) -> Self {
        Self(BigDecimal::from(val))
    }
}
impl From<u64> for Number {
    fn from(val: u64) -> Self {
        Self(BigDecimal::from(val))
    }
}
impl From<u32> for Number {
    fn from(val: u32) -> Self {
        Self(BigDecimal::from(val))
    }
}
#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::pedantic,
        clippy::nursery
    )]
    use super::*;
    use std::str::FromStr;
    #[test]
    fn test_number_new() {
        let val = BigDecimal::from_str("123.456").unwrap();
        let num = Number::new(val.clone());
        assert_eq!(num.0, val);
    }
    #[test]
    fn test_number_conversions() {
        let val_i32: Number = 42_i32.into();
        assert_eq!(val_i32.as_f64(), Some(42.0));
        let val_i64: Number = (-100_i64).into();
        assert_eq!(val_i64.as_f64(), Some(-100.0));
        let unsigned_val: Number = 10_u32.into();
        assert_eq!(unsigned_val.as_f64(), Some(10.0));
        let big_unsigned: Number = 20_u64.into();
        assert_eq!(big_unsigned.as_f64(), Some(20.0));
        let neg = -val_i32;
        assert_eq!(neg.as_f64(), Some(-42.0));
    }
    #[test]
    fn test_number_arithmetic_and_cmp() {
        let n10: Number = 10_i32.into();
        let n3: Number = 3_i32.into();
        let n0: Number = 0_i32.into();
        assert_eq!((n10.clone() + n3.clone()).as_f64(), Some(13.0));
        assert_eq!((n10.clone() - n3.clone()).as_f64(), Some(7.0));
        assert_eq!((n10.clone() * n3.clone()).as_f64(), Some(30.0));
        let div = n10.checked_div(&n3);
        assert!(div.is_some());
        assert_eq!(n10.checked_div(&n0), None);
        let rem = n10.checked_rem(&n3);
        assert!(rem.is_some());
        assert_eq!(n10.checked_rem(&n0), None);
        assert!(n3 < n10);
        assert!(n10 > n3);
        assert!(n3 <= n3);
        assert!(n3 >= n3);
        assert_eq!(n3.cmp(&n10), std::cmp::Ordering::Less);
        assert_eq!(n3.partial_cmp(&n10), Some(std::cmp::Ordering::Less));
    }
}
