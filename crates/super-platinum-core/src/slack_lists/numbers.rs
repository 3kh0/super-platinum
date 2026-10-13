//! Lists-only decimal display (captured LlTF/H0Nw). Never changes cell values.
//! Work from the JSON number's decimal spelling: converting an exact u64/i64
//! through f64 would corrupt both its displayed digits and rounding boundary.
use serde_json::{Number, Value};

pub const LOCALES: [&str; 12] = [
    "de-DE", "en-GB", "en-US", "es-ES", "es-LA", "fr-FR", "it-IT", "ja-JP", "ko-KR", "pt-BR",
    "zh-CN", "zh-TW",
];

pub fn format(value: &Number, options: Option<&Value>, locale: Option<&str>) -> String {
    let raw = value.to_string();
    let precision = match options.and_then(|options| options.get("precision")) {
        None | Some(Value::Null) => 0,
        Some(value) => match value.as_u64().filter(|value| *value <= 5) {
            Some(value) => value as usize,
            // Captured configuration exposes 0..5. Preserve an unsupported
            // schema's raw value instead of inventing a clamp or panicking.
            None => return raw,
        },
    };
    let Some(decimal) = Decimal::parse(&raw) else {
        return raw;
    };
    let locale = locale
        .filter(|locale| LOCALES.contains(locale))
        .unwrap_or("en-US");
    let rounded = decimal.rounded(precision, true);
    let integer_end = rounded.len() - precision;
    let integer = &rounded[..integer_end];
    let spanish = matches!(locale, "es-ES" | "es-LA");
    // H0Nw counts Math.round(value).toString(), including '-' except for -0.
    // Math.round resolves negative ties toward +infinity; Intl resolves ties
    // away from zero. These are deliberately separate rounding operations.
    let suppress_grouping = spanish && {
        let whole = decimal.rounded(0, !decimal.negative);
        let sign = usize::from(decimal.negative && whole.iter().any(|digit| *digit != 0));
        whole.len() + sign == 4
    };
    let (group, point) = match locale {
        "de-DE" | "it-IT" | "pt-BR" => (".", ','),
        "es-ES" | "es-LA" | "fr-FR" => ("\u{202f}", ','),
        _ => (",", '.'),
    };
    // es-ES Intl defaults to a minimum of two grouping digits; the other
    // captured locales group from four digits (it-IT explicitly enables it).
    let grouping = !suppress_grouping && integer.len() >= if locale == "es-ES" { 5 } else { 4 };
    let mut result = String::with_capacity(rounded.len() + rounded.len() / 3 + precision + 2);
    if decimal.negative {
        result.push('-');
    }
    for (index, digit) in integer.iter().enumerate() {
        if grouping && index != 0 && (integer.len() - index) % 3 == 0 {
            result.push_str(group);
        }
        result.push(char::from(b'0' + digit));
    }
    if precision > 0 {
        result.push(point);
        for digit in &rounded[integer_end..] {
            result.push(char::from(b'0' + digit));
        }
    }
    result
}

struct Decimal {
    negative: bool,
    digits: Vec<u8>,
    point: i32,
}
impl Decimal {
    fn parse(raw: &str) -> Option<Self> {
        let negative = raw.starts_with('-');
        let unsigned = raw.strip_prefix('-').unwrap_or(raw);
        let (mantissa, exponent) = unsigned
            .split_once(['e', 'E'])
            .map_or(Some((unsigned, 0)), |(mantissa, exponent)| {
                Some((mantissa, exponent.parse::<i32>().ok()?))
            })?;
        let point = i32::try_from(mantissa.find('.').unwrap_or(mantissa.len()))
            .ok()?
            .checked_add(exponent)?;
        // serde_json's finite Number spelling needs at most 309 integer digits
        // and 324 leading fractional zeros. Keep expansion explicitly bounded.
        if !(-512..=512).contains(&point) || mantissa.len() > 512 {
            return None;
        }
        let digits = mantissa
            .bytes()
            .filter(|byte| *byte != b'.')
            .map(|byte| byte.checked_sub(b'0').filter(|digit| *digit < 10))
            .collect::<Option<Vec<_>>>()?;
        (!digits.is_empty()).then_some(Self {
            negative,
            digits,
            point,
        })
    }
    fn rounded(&self, precision: usize, ties_away: bool) -> Vec<u8> {
        let keep = self.point + precision as i32;
        let mut digits = if keep <= 0 {
            vec![0]
        } else {
            let mut digits = self.digits[..(keep as usize).min(self.digits.len())].to_vec();
            digits.resize(keep as usize, 0);
            digits
        };
        let round_up = if keep < 0 {
            false
        } else {
            let rest = self.digits.get(keep as usize..).unwrap_or_default();
            rest.first().is_some_and(|first| {
                *first > 5
                    || (*first == 5 && (ties_away || rest[1..].iter().any(|digit| *digit != 0)))
            })
        };
        if round_up {
            let mut carry = true;
            for digit in digits.iter_mut().rev() {
                if *digit == 9 {
                    *digit = 0;
                } else {
                    *digit += 1;
                    carry = false;
                    break;
                }
            }
            if carry {
                digits.insert(0, 1);
            }
        }
        if digits.len() < precision + 1 {
            let mut prefix = vec![0; precision + 1 - digits.len()];
            prefix.extend(digits);
            digits = prefix;
        }
        let leading = digits
            .iter()
            .take(digits.len() - precision - 1)
            .take_while(|digit| **digit == 0)
            .count();
        digits.drain(..leading);
        digits
    }
}

#[cfg(test)]
mod tests;
