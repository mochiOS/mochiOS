//! Checked rational arithmetic for decimal input; transcendental functions use f64.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    Exact(i128, i128),
    Approx(f64),
}
pub type Result<T> = std::result::Result<T, String>;
fn overflow() -> String {
    "This calculation exceeds the supported precision".into()
}
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}
impl Number {
    pub const ZERO: Self = Self::Exact(0, 1);
    pub const ONE: Self = Self::Exact(1, 1);
    pub fn fraction(n: i128, d: i128) -> Result<Self> {
        if d == 0 {
            return Err("Cannot divide by zero".into());
        }
        // Reserve the signed minimum so negation and normalization remain checked.
        if n == i128::MIN || d == i128::MIN {
            return Err(overflow());
        }
        let common = gcd(n.unsigned_abs(), d.unsigned_abs()) as i128;
        let (n, d) = (n / common, d / common);
        Ok(if d < 0 {
            Self::Exact(-n, -d)
        } else {
            Self::Exact(n, d)
        })
    }
    pub fn decimal(text: &str) -> Result<Self> {
        let (digits, exponent) = if let Some((base, e)) = text.split_once(['e', 'E']) {
            (
                base,
                e.parse::<i32>()
                    .map_err(|_| "Invalid exponent".to_owned())?,
            )
        } else {
            (text, 0)
        };
        if exponent.unsigned_abs() > 38 {
            return Err(overflow());
        }
        let negative = digits.starts_with('-');
        let digits = digits.strip_prefix(['-', '+']).unwrap_or(digits);
        let mut n = 0i128;
        let mut scale = 0u32;
        let mut dot = false;
        let mut count = 0;
        for b in digits.bytes() {
            match b {
                b'.' if !dot => dot = true,
                b'0'..=b'9' => {
                    n = n
                        .checked_mul(10)
                        .and_then(|n| n.checked_add((b - b'0') as i128))
                        .ok_or_else(overflow)?;
                    if dot {
                        scale += 1;
                    }
                    count += 1;
                }
                _ => return Err("Invalid number".into()),
            }
        }
        if count == 0 {
            return Err("Enter a number".into());
        }
        let power = i32::try_from(scale).map_err(|_| overflow())? - exponent;
        let (n, d) = if power >= 0 {
            (n, 10i128.checked_pow(power as u32).ok_or_else(overflow)?)
        } else {
            (
                n.checked_mul(10i128.checked_pow((-power) as u32).ok_or_else(overflow)?)
                    .ok_or_else(overflow)?,
                1,
            )
        };
        Self::fraction(if negative { -n } else { n }, d)
    }
    pub fn float(self) -> f64 {
        match self {
            Self::Exact(n, d) => n as f64 / d as f64,
            Self::Approx(v) => v,
        }
    }
    pub fn approximate(value: f64) -> Result<Self> {
        if !value.is_finite() {
            Err("The result is outside the real-number range".into())
        } else {
            Ok(Self::Approx(if value == 0.0 { 0.0 } else { value }))
        }
    }
    pub fn neg(self) -> Result<Self> {
        match self {
            Self::Exact(n, d) => Self::fraction(n.checked_neg().ok_or_else(overflow)?, d),
            Self::Approx(v) => Self::approximate(-v),
        }
    }
    pub fn add(self, other: Self) -> Result<Self> {
        match (self, other) {
            (Self::Exact(a, b), Self::Exact(c, d)) => {
                let g = gcd(b as u128, d as u128) as i128;
                Self::fraction(
                    a.checked_mul(d / g)
                        .and_then(|x| c.checked_mul(b / g).and_then(|y| x.checked_add(y)))
                        .ok_or_else(overflow)?,
                    (b / g).checked_mul(d).ok_or_else(overflow)?,
                )
            }
            _ => Self::approximate(self.float() + other.float()),
        }
    }
    pub fn sub(self, other: Self) -> Result<Self> {
        self.add(other.neg()?)
    }
    pub fn mul(self, other: Self) -> Result<Self> {
        match (self, other) {
            (Self::Exact(a, b), Self::Exact(c, d)) => {
                let g = gcd(a.unsigned_abs(), d as u128) as i128;
                let h = gcd(c.unsigned_abs(), b as u128) as i128;
                Self::fraction(
                    (a / g).checked_mul(c / h).ok_or_else(overflow)?,
                    (b / h).checked_mul(d / g).ok_or_else(overflow)?,
                )
            }
            _ => Self::approximate(self.float() * other.float()),
        }
    }
    pub fn div(self, other: Self) -> Result<Self> {
        if other.float() == 0.0 {
            return Err("Cannot divide by zero".into());
        }
        match other {
            Self::Exact(n, d) => self.mul(Self::fraction(d, n)?),
            _ => Self::approximate(self.float() / other.float()),
        }
    }
    pub fn pow(self, other: Self) -> Result<Self> {
        if let Self::Exact(n, 1) = other {
            if n.unsigned_abs() <= 128 {
                let mut result = Self::ONE;
                let mut factor = self;
                let mut power = n.unsigned_abs();
                while power > 0 {
                    if power & 1 != 0 {
                        result = result.mul(factor)?;
                    }
                    power >>= 1;
                    if power > 0 {
                        factor = factor.mul(factor)?;
                    }
                }
                return if n < 0 {
                    Self::ONE.div(result)
                } else {
                    Ok(result)
                };
            }
        }
        Self::approximate(self.float().powf(other.float()))
    }
    pub fn factorial(self) -> Result<Self> {
        let Self::Exact(n, 1) = self else {
            return Err("Factorial requires a whole number".into());
        };
        if !(0..=33).contains(&n) {
            return Err("Factorial supports whole numbers from 0 to 33".into());
        }
        Ok(Self::Exact((1..=n).product(), 1))
    }
    pub fn display(self) -> String {
        match self {
            Self::Exact(n, 1) => n.to_string(),
            Self::Exact(n, d) => {
                let Some(places) = decimal_places(d) else {
                    return approximate_label(self.float());
                };
                let sign = if n < 0 { "-" } else { "" };
                let n = n.unsigned_abs();
                let d = d as u128;
                let remainder = n % d;
                if remainder == 0 {
                    return format!("{sign}{}", n / d);
                }
                // d divides 10^places. Scaling only the fractional remainder
                // avoids overflowing even when n and d approach i128::MAX.
                let fraction = remainder * (10u128.pow(places) / d);
                let digits = format!("{fraction:0width$}", width = places as usize);
                format!("{sign}{}.{}", n / d, digits.trim_end_matches('0'))
            }
            Self::Approx(value) => approximate_label(value),
        }
    }

    pub fn is_approximate(self) -> bool {
        match self {
            Self::Approx(_) => true,
            Self::Exact(_, d) => decimal_places(d).is_none(),
        }
    }
}
fn decimal_places(mut denominator: i128) -> Option<u32> {
    let (mut twos, mut fives) = (0u32, 0u32);
    while denominator % 2 == 0 {
        denominator /= 2;
        twos += 1;
    }
    while denominator % 5 == 0 {
        denominator /= 5;
        fives += 1;
    }
    let places = twos.max(fives);
    (denominator == 1 && places <= 38).then_some(places)
}
fn approximate_label(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    if value.abs() >= 1e15 || value.abs() < 1e-9 {
        let text = format!("{value:.14e}");
        let (base, exponent) = text.split_once('e').unwrap();
        format!(
            "{}e{}",
            base.trim_end_matches('0').trim_end_matches('.'),
            exponent
        )
    } else {
        format!("{value:.14}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimal_arithmetic_is_exact() {
        let a = Number::decimal("0.1").unwrap();
        let b = Number::decimal("0.2").unwrap();
        assert_eq!(a.add(b).unwrap().display(), "0.3");
        assert_eq!(
            Number::ONE
                .div(Number::Exact(3, 1))
                .unwrap()
                .mul(Number::Exact(3, 1))
                .unwrap(),
            Number::ONE
        );
        assert_eq!(
            Number::decimal("1e-12").unwrap().display(),
            "0.000000000001"
        );
    }
    #[test]
    fn checked_limits_and_domains() {
        assert!(Number::ONE.div(Number::ZERO).is_err());
        assert!(Number::decimal("1e1000000").is_err());
        assert!(Number::Exact(i128::MAX, 1).add(Number::ONE).is_err());
        assert!(Number::Exact(-1, 1).factorial().is_err());
        assert!(Number::Exact(34, 1).factorial().is_err());
        assert_eq!(Number::Exact(5, 1).factorial().unwrap().display(), "120");
        assert!(Number::fraction(i128::MIN, 1).is_err());
    }
    #[test]
    fn signs_powers_and_rounded_display() {
        assert_eq!(Number::decimal("-1.25").unwrap().display(), "-1.25");
        assert_eq!(
            Number::Exact(2, 1)
                .pow(Number::Exact(-3, 1))
                .unwrap()
                .display(),
            "0.125"
        );
        assert!(
            Number::ONE
                .div(Number::Exact(3, 1))
                .unwrap()
                .is_approximate()
        );
        assert_eq!(Number::Approx(1e-16).display(), "1e-16");
        let scale = 10i128.pow(38);
        let near_one = Number::fraction(scale - 1, scale).unwrap();
        assert_eq!(near_one.display(), format!("0.{}", "9".repeat(38)));
        assert!(!near_one.is_approximate());
        assert!(Number::fraction(1, 1i128 << 100).unwrap().is_approximate());
    }
}
