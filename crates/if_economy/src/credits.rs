// credits.rs: Fixed-point currency.
//
// *** DO NOT USE FLOATS FOR MONEY. ***
//
// Floating-point is fundamentally wrong for currency: 0.1 + 0.2 != 0.3
// in IEEE-754, and rounding drift across millions of transactions will
// either mint or destroy credits.
//
// We store every credit as an i64 number of cents (1/100 of a credit).
// The `i64` range handles ~92 quadrillion credits, which is more than
// enough headroom for any sensible game economy. We use signed integers
// so that debts, refunds, and accounting deltas can be expressed
// naturally without a separate Debit/Credit distinction.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Credits — the in-game currency.
///
/// Internally stored as 1/100 units (cents). A value of `Credits(123)`
/// means "1.23 credits". Always use the constructors (`from_whole`,
/// `from_cents`, `ZERO`) to make the unit clear at the call site.
///
/// Arithmetic is saturating on overflow (clamps to i64::MAX/MIN) rather
/// than panicking — a catastrophic clamp is still preferable to a
/// panic in a networked economy.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct Credits(i64);

impl Credits {
    /// Zero credits — the additive identity.
    pub const ZERO: Self = Self(0);

    /// Construct from a whole number of credits, e.g. `from_whole(10)` = "10.00 cr".
    pub const fn from_whole(n: i64) -> Self {
        // Saturating on construction: a caller passing i64::MAX would
        // otherwise silently wrap to a huge negative number.
        Self(n.saturating_mul(100))
    }

    /// Construct from a raw cent count, e.g. `from_cents(150)` = "1.50 cr".
    pub const fn from_cents(n: i64) -> Self {
        Self(n)
    }

    /// Raw cents (the internal representation).
    pub const fn cents(self) -> i64 {
        self.0
    }

    /// Whole credits, truncated toward zero.
    pub const fn whole(self) -> i64 {
        self.0 / 100
    }

    /// Is this value negative (a debt / deficit)?
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// Is this value positive?
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    /// Absolute value.
    pub const fn abs(self) -> Self {
        Self(self.0.saturating_abs())
    }
}

impl Add for Credits {
    type Output = Credits;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Credits {
    type Output = Credits;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl Neg for Credits {
    type Output = Credits;
    fn neg(self) -> Self {
        Self(self.0.saturating_neg())
    }
}

impl AddAssign for Credits {
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0.saturating_add(rhs.0);
    }
}

impl SubAssign for Credits {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0.saturating_sub(rhs.0);
    }
}

// Scalar multiplication: Credits × quantity.
// This is the "price × quantity = total" path — extremely common.
impl Mul<i64> for Credits {
    type Output = Credits;
    fn mul(self, rhs: i64) -> Self {
        Self(self.0.saturating_mul(rhs))
    }
}

impl Mul<Credits> for i64 {
    type Output = Credits;
    fn mul(self, rhs: Credits) -> Credits {
        Credits(self.saturating_mul(rhs.0))
    }
}

// u32 is the inventory quantity type; supporting it avoids `as i64` noise everywhere.
impl Mul<u32> for Credits {
    type Output = Credits;
    fn mul(self, rhs: u32) -> Self {
        Self(self.0.saturating_mul(rhs as i64))
    }
}

impl Mul<Credits> for u32 {
    type Output = Credits;
    fn mul(self, rhs: Credits) -> Credits {
        Credits(rhs.0.saturating_mul(self as i64))
    }
}

// Integer division, truncating toward zero. Used for averages.
impl Div<i64> for Credits {
    type Output = Credits;
    fn div(self, rhs: i64) -> Self {
        // Division by zero would panic; callers should check.
        // We deliberately let it panic rather than return ZERO silently,
        // because silently dividing by zero in money code is worse.
        Self(self.0 / rhs)
    }
}

impl fmt::Display for Credits {
    /// Format as "1234.56 cr" (negative: "-1234.56 cr").
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cents = self.0;
        let sign = if cents < 0 { "-" } else { "" };
        let abs = cents.unsigned_abs();
        let whole = abs / 100;
        let frac = abs % 100;
        write!(f, "{sign}{whole}.{frac:02} cr")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_whole_and_cents() {
        assert_eq!(Credits::from_whole(1).cents(), 100);
        assert_eq!(Credits::from_whole(0).cents(), 0);
        assert_eq!(Credits::from_cents(250).whole(), 2);
        assert_eq!(Credits::from_cents(99).whole(), 0);
    }

    #[test]
    fn zero_is_additive_identity() {
        let a = Credits::from_whole(42);
        assert_eq!(a + Credits::ZERO, a);
        assert_eq!(Credits::ZERO + a, a);
    }

    #[test]
    fn add_and_sub() {
        let a = Credits::from_whole(10);
        let b = Credits::from_cents(50);
        assert_eq!((a + b).cents(), 1050);
        assert_eq!((a - b).cents(), 950);
    }

    #[test]
    fn neg_and_abs() {
        let a = Credits::from_whole(5);
        assert_eq!(-a, Credits::from_cents(-500));
        assert_eq!((-a).abs(), a);
        assert_eq!(a.abs(), a);
    }

    #[test]
    fn add_assign_sub_assign() {
        let mut a = Credits::from_whole(10);
        a += Credits::from_whole(5);
        assert_eq!(a, Credits::from_whole(15));
        a -= Credits::from_whole(3);
        assert_eq!(a, Credits::from_whole(12));
    }

    #[test]
    fn mul_by_scalar() {
        let price = Credits::from_cents(250); // 2.50 cr
        let total = price * 10i64;
        assert_eq!(total, Credits::from_whole(25));
        // commutative
        let total2 = 10i64 * price;
        assert_eq!(total, total2);
    }

    #[test]
    fn mul_by_u32_quantity() {
        let price = Credits::from_cents(125); // 1.25 cr
        let total = price * 4u32;
        assert_eq!(total, Credits::from_whole(5));
        let total2 = 4u32 * price;
        assert_eq!(total, total2);
    }

    #[test]
    fn div_truncates() {
        // 10.00 / 3 = 3.33
        let a = Credits::from_whole(10);
        assert_eq!((a / 3).cents(), 333);
        // 10.00 / 4 = 2.50
        assert_eq!((Credits::from_whole(10) / 4).cents(), 250);
    }

    #[test]
    fn display_format() {
        assert_eq!(format!("{}", Credits::from_whole(1234)), "1234.00 cr");
        assert_eq!(format!("{}", Credits::from_cents(123456)), "1234.56 cr");
        assert_eq!(format!("{}", Credits::from_cents(5)), "0.05 cr");
        assert_eq!(format!("{}", Credits::from_cents(-123456)), "-1234.56 cr");
        assert_eq!(format!("{}", Credits::ZERO), "0.00 cr");
    }

    #[test]
    fn ordering() {
        let a = Credits::from_whole(5);
        let b = Credits::from_whole(10);
        assert!(a < b);
        assert!(b > a);
        assert!(a <= a);
    }

    #[test]
    fn overflow_saturates_not_panics() {
        let huge = Credits::from_cents(i64::MAX);
        let more = huge + Credits::from_cents(1);
        // Saturated at i64::MAX, did not panic
        assert_eq!(more.cents(), i64::MAX);
        let neg = Credits::from_cents(i64::MIN);
        let less = neg - Credits::from_cents(1);
        assert_eq!(less.cents(), i64::MIN);
    }

    #[test]
    fn reasonable_game_values_do_not_overflow() {
        // Price of 1 million credits, quantity of 1 million units.
        // Total: 10^12 credits = 10^14 cents. i64::MAX ≈ 9.2 * 10^18.
        let price = Credits::from_whole(1_000_000);
        let qty = 1_000_000u32;
        let total = price * qty;
        assert!(total.cents() > 0);
        assert_eq!(total.whole(), 1_000_000_000_000);
    }

    #[test]
    fn sign_predicates() {
        assert!(Credits::from_whole(1).is_positive());
        assert!(!Credits::from_whole(1).is_negative());
        assert!(Credits::from_whole(-1).is_negative());
        assert!(!Credits::from_whole(-1).is_positive());
        assert!(!Credits::ZERO.is_positive());
        assert!(!Credits::ZERO.is_negative());
    }

    #[test]
    fn serde_roundtrip() {
        let a = Credits::from_cents(12345);
        let bytes = bincode::serialize(&a).unwrap();
        let b: Credits = bincode::deserialize(&bytes).unwrap();
        assert_eq!(a, b);
    }
}
