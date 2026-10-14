use super::{ListError, ordering_error};

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const MAX_KEY_LENGTH: usize = 1024;

// S Ommm TU (4545426), default integer length 10 and alphabet 0-9A-Za-z.
// Keep malformed input out of its permissive JavaScript number coercions.
pub(super) fn validate(key: &str) -> Result<(), ListError> {
    let bytes = key.as_bytes();
    if bytes.len() < 10
        || bytes.len() > MAX_KEY_LENGTH
        || !bytes[..10].iter().all(u8::is_ascii_digit)
        || (bytes.len() > 10
            && (bytes[10] != b'.'
                || bytes[11..].iter().any(|byte| !DIGITS.contains(byte))
                || bytes.last() == Some(&b'0')))
    {
        return Err(ordering_error());
    }
    Ok(())
}

fn integer(key: &str) -> u64 {
    key[..10].parse().expect("validated decimal order key")
}

fn format(number: u64) -> Result<String, ListError> {
    if number >= 10_000_000_000 {
        return Err(ordering_error());
    }
    Ok(format!("{number:010}"))
}

fn fraction(key: &str) -> &str {
    key.get(11..).unwrap_or("")
}

pub(super) fn between(lower: Option<&str>, upper: Option<&str>) -> Result<String, ListError> {
    for key in [lower, upper].into_iter().flatten() {
        validate(key)?;
    }
    if lower
        .zip(upper)
        .is_some_and(|(lower, upper)| lower >= upper)
    {
        return Err(ordering_error());
    }
    match (lower, upper) {
        (None, None) => format(5_000_000_000),
        (None, Some(upper)) => {
            let number = integer(upper);
            if number.to_string().as_str() < upper && !fraction(upper).is_empty() {
                format(number)
            } else {
                format(number.checked_sub(1).ok_or_else(ordering_error)?)
            }
        }
        (Some(lower), None) => format(integer(lower) + 1),
        (Some(lower), Some(upper)) => {
            let left = integer(lower);
            let right = integer(upper);
            if left == right {
                return Ok(format!(
                    "{}.{}",
                    format(left)?,
                    midpoint(fraction(lower), Some(fraction(upper)))?
                ));
            }
            let middle = (left + right) / 2;
            if middle > left && middle < right {
                return format(middle);
            }
            Ok(format!(
                "{}.{}",
                format(left)?,
                midpoint(fraction(lower), None)?
            ))
        }
    }
}

fn midpoint(mut lower: &str, mut upper: Option<&str>) -> Result<String, ListError> {
    let mut prefix = String::new();
    loop {
        if prefix.len() >= MAX_KEY_LENGTH - 11 {
            return Err(ordering_error());
        }
        if upper.is_some_and(|upper| lower >= upper)
            || lower.ends_with('0')
            || upper.is_some_and(|upper| upper.ends_with('0'))
        {
            return Err(ordering_error());
        }
        if let Some(end) = upper {
            let shared = end
                .bytes()
                .enumerate()
                .take_while(|(index, byte)| {
                    lower.as_bytes().get(*index).copied().unwrap_or(b'0') == *byte
                })
                .count();
            if shared > 0 {
                prefix.push_str(&end[..shared]);
                lower = lower.get(shared..).unwrap_or("");
                upper = Some(&end[shared..]);
                continue;
            }
        }
        let left = lower.as_bytes().first().map_or(0, |byte| {
            DIGITS.iter().position(|digit| digit == byte).unwrap()
        });
        let right = upper.map_or(DIGITS.len(), |upper| {
            upper
                .as_bytes()
                .first()
                .and_then(|byte| DIGITS.iter().position(|digit| digit == byte))
                .unwrap_or(0)
        });
        if right > left + 1 {
            // JavaScript Math.round for a nonnegative half integer.
            prefix.push(DIGITS[(left + right).div_ceil(2)] as char);
            return Ok(prefix);
        }
        if let Some(upper) = upper.filter(|upper| upper.len() > 1) {
            prefix.push(upper.as_bytes()[0] as char);
            return Ok(prefix);
        }
        prefix.push(DIGITS[left] as char);
        lower = lower.get(1..).unwrap_or("");
        upper = None;
    }
}
