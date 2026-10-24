//! Operand-aware keypad edits use the native field's current byte selection.
use std::ops::Range;

fn boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}
fn token(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '.' || c == 'π'
}
fn exponent_sign(text: &str, index: usize) -> bool {
    matches!(text.as_bytes()[index], b'+' | b'-')
        && index > 0
        && matches!(text.as_bytes()[index - 1], b'e' | b'E')
        && text[..index - 1]
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_digit() || c == '.')
}
pub fn operand(text: &str, selection: Range<usize>) -> Range<usize> {
    let start = boundary(text, selection.start);
    let mut end = boundary(text, selection.end);
    if start != end {
        return start.min(end)..start.max(end);
    }
    while end > 0 && text[..end].chars().last().unwrap().is_whitespace() {
        end -= text[..end].chars().last().unwrap().len_utf8();
    }
    let mut begin = end;
    if text[..end].ends_with(')') {
        let mut depth = 0;
        for (i, c) in text[..end].char_indices().rev() {
            if c == ')' {
                depth += 1;
            } else if c == '(' {
                depth -= 1;
                if depth == 0 {
                    begin = i;
                    break;
                }
            }
        }
        if depth != 0 {
            return end..end;
        }
        while begin > 0 && text[..begin].chars().last().unwrap().is_ascii_alphabetic() {
            begin -= 1;
        }
    } else {
        while begin > 0 {
            let c = text[..begin].chars().last().unwrap();
            let i = begin - c.len_utf8();
            if token(c) || exponent_sign(text, i) {
                begin = i;
            } else {
                break;
            }
        }
        while end < text.len() {
            let c = text[end..].chars().next().unwrap();
            if token(c) || exponent_sign(text, end) {
                end += c.len_utf8();
            } else {
                break;
            }
        }
        if begin == end {
            return end..end;
        }
    }
    if begin > 0 && text[..begin].ends_with('-') {
        let prefix = text[..begin - 1].trim_end();
        if prefix.is_empty() || prefix.ends_with(['(', '+', '-', '*', '/', '^', '×', '÷']) {
            begin -= 1;
        }
    }
    begin..end
}
pub fn negate(value: &str) -> String {
    if let Some(inner) = value.strip_prefix("-(").and_then(|s| s.strip_suffix(')')) {
        inner.into()
    } else if let Some(inner) = value.strip_prefix('-') {
        inner.into()
    } else {
        format!("-({value})")
    }
}
pub fn complete(text: &str) -> String {
    let balance = text.chars().fold(0i32, |n, c| {
        n + if c == '(' {
            1
        } else if c == ')' {
            -1
        } else {
            0
        }
    });
    if balance > 0 && balance <= 64 {
        format!("{text}{}", ")".repeat(balance as usize))
    } else {
        text.into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_the_operand_under_the_caret_or_selection() {
        for (text, pos, expected) in [
            ("12+345*2", 5, "345"),
            ("12+3", 4, "3"),
            ("12+-3", 5, "-3"),
            ("sin(30)", 7, "sin(30)"),
            ("2+sqrt(9)", 9, "sqrt(9)"),
            ("1e-3+2", 4, "1e-3"),
        ] {
            let range = operand(text, pos..pos);
            assert_eq!(&text[range], expected, "{text}");
        }
        assert_eq!(operand("12+3", 4..4), 3..4);
        assert_eq!(operand("12+", 3..3), 3..3);
        assert_eq!(operand("12+34", 0..2), 0..2);
    }
    #[test]
    fn toggling_sign_and_completing_parentheses_are_reversible() {
        assert_eq!(negate("3"), "-(3)");
        assert_eq!(negate("-(3)"), "3");
        assert_eq!(complete("2+sin(30"), "2+sin(30)");
    }
}
