//! Scalar StringNumericLiteral used only by the reached temp-mute predicate.
//! Decimal parse uses Rust's correctly rounded str -> f64 path, not a parsed
//! serde_json Number. Binary/octal/hex integers round once from their exact bits.
pub(super) fn parse(text: &str) -> f64 {
    let s = text.trim_matches(white);
    if s.is_empty() {
        return 0.0;
    }
    match s {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefix, bits) in [
        ("0x", 4),
        ("0X", 4),
        ("0o", 3),
        ("0O", 3),
        ("0b", 1),
        ("0B", 1),
    ] {
        if let Some(digits) = s.strip_prefix(prefix) {
            return integer(digits, bits);
        }
    }
    if decimal(s.as_bytes()) {
        s.parse::<f64>().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}
fn white(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn decimal(s: &[u8]) -> bool {
    let mut i = usize::from(s.first().is_some_and(|c| matches!(c, b'+' | b'-')));
    let mut digits = 0;
    while s.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
        digits += 1;
    }
    if s.get(i) == Some(&b'.') {
        i += 1;
        while s.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return false;
    }
    if s.get(i).is_some_and(|c| matches!(c, b'e' | b'E')) {
        i += 1;
        if s.get(i).is_some_and(|c| matches!(c, b'+' | b'-')) {
            i += 1;
        }
        let start = i;
        while s.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == s.len()
}
fn integer(digits: &str, width: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    let mut bits = Vec::new();
    for c in digits.chars() {
        let Some(d) = c.to_digit(1 << width) else {
            return f64::NAN;
        };
        // char::to_digit permits only ASCII hexadecimal digits for this radix.
        for shift in (0..width).rev() {
            let b = (d >> shift) & 1;
            if b != 0 || !bits.is_empty() {
                bits.push(b as u64);
            }
        }
    }
    if bits.is_empty() {
        return 0.0;
    }
    let mut exponent = bits.len() - 1;
    if exponent > 1023 {
        return f64::INFINITY;
    }
    let count = bits.len().min(53);
    let mut mantissa = bits[..count].iter().fold(0_u64, |m, b| (m << 1) | b) << (53 - count);
    if bits.len() > 53 && bits[53] == 1 && (bits[54..].contains(&1) || mantissa & 1 == 1) {
        mantissa += 1;
    }
    if mantissa == (1_u64 << 53) {
        mantissa >>= 1;
        exponent += 1;
    }
    if exponent > 1023 {
        return f64::INFINITY;
    }
    f64::from_bits(((exponent as u64 + 1023) << 52) | (mantissa & ((1_u64 << 52) - 1)))
}
