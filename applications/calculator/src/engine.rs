use crate::number::{Number, Result};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Standard,
    Scientific,
    Convert,
    Programmer,
}
impl Mode {
    pub const ALL: [Self; 4] = [
        Self::Standard,
        Self::Scientific,
        Self::Convert,
        Self::Programmer,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Scientific => "Scientific",
            Self::Convert => "Convert",
            Self::Programmer => "Programmer",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|m| m.label().eq_ignore_ascii_case(text))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub mode: Mode,
    pub degrees: bool,
    pub base: u32,
    pub bits: u32,
    pub signed: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Standard,
            degrees: true,
            base: 10,
            bits: 64,
            signed: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Real(Number),
    Word(u64),
}
impl Value {
    pub fn display(self, settings: Settings) -> String {
        match self {
            Self::Real(n) => n.display(),
            Self::Word(n) => format_word(n, settings.base, settings.bits, settings.signed),
        }
    }

    pub fn approximate(self) -> bool {
        matches!(self,Self::Real(n)if n.is_approximate())
    }
}
pub fn mask(bits: u32) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}
pub fn signed_word(n: u64, bits: u32) -> i128 {
    let n = n & mask(bits);
    if n & (1u64 << (bits - 1)) != 0 {
        i128::from(n) - (1i128 << bits)
    } else {
        i128::from(n)
    }
}
pub fn format_word(n: u64, base: u32, bits: u32, signed: bool) -> String {
    let n = n & mask(bits);
    match base {
        2 => format!("0b{n:b}"),
        8 => format!("0o{n:o}"),
        16 => format!("0x{n:X}"),
        _ if signed => signed_word(n, bits).to_string(),
        _ => n.to_string(),
    }
}
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Literal(String),
    Name(String),
    Op(&'static str),
    Left,
    Right,
    End,
}
fn lex(text: &str, program: bool) -> Result<Vec<Token>> {
    if text.len() > 2048 {
        return Err("The expression is too long".into());
    }
    let normalized = text
        .replace('×', "*")
        .replace('÷', "/")
        .replace('−', "-")
        .replace('π', "pi");
    let b = normalized.as_bytes();
    let mut i = 0;
    let mut tokens = Vec::new();
    while i < b.len() {
        if tokens.len() >= 512 {
            return Err("The expression is too complex".into());
        }
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let token = if c.is_ascii_digit() || (!program && c == b'.') {
            let start = i;
            i += 1;
            if program {
                while i < b.len() && b[i].is_ascii_alphanumeric() {
                    i += 1;
                }
            } else {
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                if i < b.len() && matches!(b[i], b'e' | b'E') {
                    i += 1;
                    if i < b.len() && matches!(b[i], b'+' | b'-') {
                        i += 1;
                    }
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            Token::Literal(normalized[start..i].to_owned())
        } else if c.is_ascii_alphabetic() {
            let start = i;
            while i < b.len() && b[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let name = normalized[start..i].to_ascii_lowercase();
            match name.as_str() {
                "and" => Token::Op("&"),
                "or" => Token::Op("|"),
                "xor" => Token::Op("xor"),
                "mod" => Token::Op("mod"),
                "not" => Token::Op("~"),
                _ => Token::Name(name),
            }
        } else {
            i += 1;
            match c {
                b'(' => Token::Left,
                b')' => Token::Right,
                b'+' => Token::Op("+"),
                b'-' => Token::Op("-"),
                b'*' => Token::Op("*"),
                b'/' => Token::Op("/"),
                b'^' => Token::Op("^"),
                b'%' => Token::Op("%"),
                b'!' => Token::Op("!"),
                b'&' => Token::Op("&"),
                b'|' => Token::Op("|"),
                b'~' => Token::Op("~"),
                b'<' if b.get(i) == Some(&b'<') => {
                    i += 1;
                    Token::Op("<<")
                }
                b'>' if b.get(i) == Some(&b'>') => {
                    i += 1;
                    Token::Op(">>")
                }
                _ => return Err("The expression contains an unsupported character".into()),
            }
        };
        tokens.push(token);
    }
    tokens.push(Token::End);
    Ok(tokens)
}
#[derive(Clone, Copy)]
struct Eval {
    value: Value,
    percent: bool,
}
impl Eval {
    fn new(value: Value) -> Self {
        Self {
            value,
            percent: false,
        }
    }
}
struct Parser {
    tokens: Vec<Token>,
    at: usize,
    settings: Settings,
    answer: Option<Value>,
}
pub fn evaluate(text: &str, settings: Settings, answer: Option<Value>) -> Result<Value> {
    if ![8, 16, 32, 64].contains(&settings.bits) || ![2, 8, 10, 16].contains(&settings.base) {
        return Err("Invalid programmer settings".into());
    }
    let mut parser = Parser {
        tokens: lex(text, settings.mode == Mode::Programmer)?,
        at: 0,
        settings,
        answer,
    };
    let result = parser.expression(0, 0)?.value;
    if parser.tokens[parser.at] != Token::End {
        return Err("Check the expression and parentheses".into());
    }
    Ok(result)
}
impl Parser {
    fn expression(&mut self, min: u8, depth: usize) -> Result<Eval> {
        if depth > 64 {
            return Err("The expression is too deeply nested".into());
        }
        let token = self.tokens[self.at].clone();
        self.at += 1;
        let mut left = match token {
            Token::Literal(text) => Eval::new(self.literal(&text)?),
            Token::Name(name) if name == "ans" => {
                Eval::new(self.answer.ok_or("There is no previous result")?)
            }
            Token::Name(name) if self.settings.mode == Mode::Programmer => {
                Eval::new(self.literal(&name)?)
            }
            Token::Name(name) if name == "pi" || name == "e" => {
                self.scientific()?;
                Eval::new(Value::Real(Number::Approx(if name == "pi" {
                    std::f64::consts::PI
                } else {
                    std::f64::consts::E
                })))
            }
            Token::Name(name) => {
                self.scientific()?;
                if self.tokens[self.at] != Token::Left {
                    return Err("A function requires parentheses".into());
                }
                self.at += 1;
                let argument = self.expression(0, depth + 1)?.value;
                self.close()?;
                Eval::new(self.function(&name, argument)?)
            }
            Token::Left => {
                let v = self.expression(0, depth + 1)?;
                self.close()?;
                v
            }
            Token::Op(op @ ("+" | "-" | "~")) => {
                let operand = self.expression(25, depth + 1)?;
                let value = match (op, operand.value) {
                    ("+", v) => v,
                    ("-", Value::Real(n)) => Value::Real(n.neg()?),
                    ("-", Value::Word(n)) => {
                        Value::Word(n.wrapping_neg() & mask(self.settings.bits))
                    }
                    ("~", Value::Word(n)) => Value::Word(!n & mask(self.settings.bits)),
                    _ => return Err("NOT is available in Programmer mode".into()),
                };
                Eval {
                    value,
                    percent: operand.percent && op != "~",
                }
            }
            _ => return Err("Enter a complete expression".into()),
        };
        loop {
            let token = self.tokens[self.at].clone();
            if matches!(token, Token::Op("!" | "%")) && self.settings.mode != Mode::Programmer {
                if 40 < min {
                    break;
                }
                self.at += 1;
                let Value::Real(n) = left.value else {
                    return Err("Clear the expression after changing mode".into());
                };
                left = if token == Token::Op("%") {
                    Eval {
                        value: Value::Real(n.div(Number::Exact(100, 1))?),
                        percent: true,
                    }
                } else {
                    self.scientific()?;
                    Eval::new(Value::Real(n.factorial()?))
                };
                continue;
            }
            let implicit = self.settings.mode != Mode::Programmer
                && matches!(token, Token::Left | Token::Name(_));
            let op = if implicit {
                "*"
            } else if let Token::Op(op) = token {
                op
            } else {
                break;
            };
            let (precedence, right) = match op {
                "|" => (5, false),
                "xor" => (6, false),
                "&" => (7, false),
                "<<" | ">>" => (9, false),
                "+" | "-" => (10, false),
                "*" | "/" | "mod" | "%" => (20, false),
                "^" if self.settings.mode == Mode::Programmer => (6, false),
                "^" => (30, true),
                _ => return Err("Unsupported operation".into()),
            };
            if precedence < min {
                break;
            }
            if !implicit {
                self.at += 1;
            }
            let rhs =
                self.expression(if right { precedence } else { precedence + 1 }, depth + 1)?;
            left = Eval::new(self.binary(op, left.value, rhs)?);
        }
        Ok(left)
    }
    fn close(&mut self) -> Result<()> {
        if self.tokens[self.at] != Token::Right {
            return Err("A closing parenthesis is missing".into());
        }
        self.at += 1;
        Ok(())
    }
    fn scientific(&self) -> Result<()> {
        if self.settings.mode != Mode::Scientific {
            Err("Switch to Scientific mode for this function".into())
        } else {
            Ok(())
        }
    }
    fn literal(&self, text: &str) -> Result<Value> {
        if self.settings.mode != Mode::Programmer {
            let number = Number::decimal(text).or_else(|error| {
                if self.settings.mode != Mode::Scientific {
                    return Err(error);
                }
                let value = text.parse::<f64>().map_err(|_| error)?;
                Number::approximate(value)
            })?;
            return Ok(Value::Real(number));
        }
        let (digits, base) =
            if let Some(d) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                (d, 16)
            } else if let Some(d) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
                (d, 2)
            } else if let Some(d) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
                (d, 8)
            } else {
                (text, self.settings.base)
            };
        let n = u64::from_str_radix(digits, base).map_err(|_| {
            "This number is invalid for the selected base or exceeds 64 bits".to_owned()
        })?;
        if n > mask(self.settings.bits) {
            return Err("This number does not fit the selected word size".into());
        }
        Ok(Value::Word(n))
    }
    fn binary(&self, op: &str, left: Value, right: Eval) -> Result<Value> {
        match (left, right.value) {
            (Value::Real(a), Value::Real(mut b)) => {
                if right.percent && matches!(op, "+" | "-") {
                    b = a.mul(b).or_else(|error| {
                        if self.settings.mode == Mode::Scientific {
                            Number::approximate(a.float() * b.float())
                        } else {
                            Err(error)
                        }
                    })?;
                }
                let value = match op {
                    "+" => a.add(b),
                    "-" => a.sub(b),
                    "*" => a.mul(b),
                    "/" => a.div(b),
                    "^" => {
                        self.scientific()?;
                        a.pow(b)
                    }
                    _ => Err("Bit operations are available in Programmer mode".into()),
                }
                .or_else(|error| {
                    if self.settings.mode != Mode::Scientific
                        || !matches!(op, "+" | "-" | "*" | "/" | "^")
                        || (op == "/" && b.float() == 0.0)
                    {
                        return Err(error);
                    }
                    let (a, b) = (a.float(), b.float());
                    Number::approximate(match op {
                        "+" => a + b,
                        "-" => a - b,
                        "*" => a * b,
                        "/" => a / b,
                        _ => a.powf(b),
                    })
                })?;
                Ok(Value::Real(value))
            }
            (Value::Word(a), Value::Word(b)) => {
                let bits = self.settings.bits;
                let result = match op {
                    "+" => a.wrapping_add(b),
                    "-" => a.wrapping_sub(b),
                    "*" => a.wrapping_mul(b),
                    "/" | "mod" | "%" => {
                        if b == 0 {
                            return Err("Cannot divide by zero".into());
                        }
                        if self.settings.signed {
                            let a = signed_word(a, bits);
                            let b = signed_word(b, bits);
                            if op == "/" {
                                (a / b) as u64
                            } else {
                                (a % b) as u64
                            }
                        } else if op == "/" {
                            a / b
                        } else {
                            a % b
                        }
                    }
                    "&" => a & b,
                    "|" => a | b,
                    "^" | "xor" => a ^ b,
                    "<<" | ">>" => {
                        if b >= u64::from(bits) {
                            return Err("The shift count must be smaller than the word size".into());
                        }
                        if op == "<<" {
                            a << b
                        } else if self.settings.signed {
                            (signed_word(a, bits) >> b) as u64
                        } else {
                            a >> b
                        }
                    }
                    _ => return Err("Unsupported programmer operation".into()),
                };
                Ok(Value::Word(result & mask(bits)))
            }
            _ => Err("Clear the expression after changing mode".into()),
        }
    }
    fn function(&self, name: &str, value: Value) -> Result<Value> {
        let Value::Real(n) = value else {
            return Err("A real number is required".into());
        };
        let x = n.float();
        let radians = if self.settings.degrees {
            x.to_radians()
        } else {
            x
        };
        if name == "abs" {
            return Ok(Value::Real(if x < 0.0 { n.neg()? } else { n }));
        }
        if name == "sqrt" {
            if x < 0.0 {
                return Err("The square root requires a nonnegative number".into());
            }
            if let Number::Exact(a, b) = n {
                let r = (a as u128).isqrt();
                let s = (b as u128).isqrt();
                if r * r == a as u128 && s * s == b as u128 {
                    return Ok(Value::Real(Number::fraction(r as i128, s as i128)?));
                }
            }
        }
        let y = match name {
            "sin" if self.settings.degrees && x.rem_euclid(180.0) == 0.0 => 0.0,
            "cos" if self.settings.degrees && x.rem_euclid(180.0) == 90.0 => 0.0,
            "sin" => radians.sin(),
            "cos" => radians.cos(),
            "tan" => {
                if radians.cos().abs() < f64::EPSILON * 4.0 {
                    return Err("Tangent is undefined at this angle".into());
                }
                radians.tan()
            }
            "asin" => {
                let n = x.asin();
                if self.settings.degrees {
                    n.to_degrees()
                } else {
                    n
                }
            }
            "acos" => {
                let n = x.acos();
                if self.settings.degrees {
                    n.to_degrees()
                } else {
                    n
                }
            }
            "atan" => {
                let n = x.atan();
                if self.settings.degrees {
                    n.to_degrees()
                } else {
                    n
                }
            }
            "ln" => {
                if x <= 0.0 {
                    return Err("The logarithm requires a positive number".into());
                }
                x.ln()
            }
            "log" => {
                if x <= 0.0 {
                    return Err("The logarithm requires a positive number".into());
                }
                x.log10()
            }
            "sqrt" => x.sqrt(),
            "exp" => x.exp(),
            _ => return Err("Unknown scientific function".into()),
        };
        Number::approximate(y).map(Value::Real)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn calc(text: &str) -> String {
        evaluate(text, Settings::default(), None)
            .unwrap()
            .display(Settings::default())
    }
    #[test]
    fn basic_precedence_parentheses_and_percent() {
        assert_eq!(calc("0.1+0.2"), "0.3");
        assert_eq!(calc("2*(3+4)"), "14");
        assert_eq!(calc("2(3+4)"), "14");
        assert_eq!(calc("200+10%"), "220");
        assert_eq!(calc("200-10%"), "180");
        assert_eq!(calc("200+-10%"), "180");
        assert_eq!(calc("200*10%"), "20");
        assert_eq!(calc("50%"), "0.5");
        assert_eq!(calc("6÷2×3"), "9");
    }
    #[test]
    fn scientific_functions_and_domains() {
        let s = Settings {
            mode: Mode::Scientific,
            ..Settings::default()
        };
        let e = |text| evaluate(text, s, None).unwrap().display(s);
        assert_eq!(e("-2^2"), "-4");
        assert_eq!(e("2^3^2"), "512");
        assert_eq!(e("2^-3"), "0.125");
        assert_eq!(e("sqrt(4)+5!"), "122");
        assert_eq!(e("sin(30)"), "0.5");
        assert_eq!(e("cos(90)"), "0");
        assert_eq!(e("ln(e)"), "1");
        assert_eq!(e("10^100"), "1e100");
        assert_eq!(e("1e38*1e38"), "1e76");
        assert_eq!(e("1e-100+1e-100"), "2e-100");
        for text in ["1/0", "sqrt(-1)", "ln(0)", "tan(90)", "asin(2)", "34!"] {
            assert!(evaluate(text, s, None).is_err(), "{text}");
        }
    }
    #[test]
    fn programmer_bases_masks_shifts_and_signed_arithmetic() {
        let s = Settings {
            mode: Mode::Programmer,
            bits: 8,
            base: 16,
            ..Settings::default()
        };
        let e = |text| evaluate(text, s, None).unwrap();
        assert_eq!(e("FF+1"), Value::Word(0));
        assert_eq!(e("~0"), Value::Word(255));
        assert_eq!(e("F0 and 0F"), Value::Word(0));
        assert_eq!(e("80 >> 1"), Value::Word(64));
        assert_eq!(e("1<<7"), Value::Word(128));
        assert!(evaluate("1<<8", s, None).is_err());
        assert!(evaluate("100", s, None).is_err());
        let signed = Settings { signed: true, ..s };
        assert_eq!(evaluate("FE/2", signed, None).unwrap(), Value::Word(255));
        assert_eq!(format_word(255, 10, 8, true), "-1");
    }
    #[test]
    fn invalid_input_is_bounded_and_never_panics() {
        for text in ["", "(", "2+", "1/0", "1..2", "2**3", "foo(2)", "1)", "2,3"] {
            assert!(evaluate(text, Settings::default(), None).is_err(), "{text}");
        }
        assert!(evaluate(&"(".repeat(300), Settings::default(), None).is_err());
        assert!(evaluate(&"1+".repeat(2000), Settings::default(), None).is_err());
        assert!(evaluate("sin(30)", Settings::default(), None).is_err());
    }
}
