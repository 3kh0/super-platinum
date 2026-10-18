//! Ommm.TU fixed-ten-digit/base62 key generation, on a closed supported domain.
//! The captured helper permits malformed input through coercion. We do not:
//! canonical ASCII digits/fractions only, bounded to 1024 bytes locally.
use super::ListError;

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
pub(super) const MAX_KEY_BYTES: usize = 1024;
const LIMIT: u64 = 10_000_000_000;

pub(in crate::slack_lists) fn valid(key: &str) -> bool {
    let bytes = key.as_bytes();
    (10..=MAX_KEY_BYTES).contains(&bytes.len())
        && bytes[..10].iter().all(u8::is_ascii_digit)
        && (bytes.len() == 10
            || bytes.len() > 11
                && bytes[10] == b'.'
                && bytes[11..].iter().all(u8::is_ascii_alphanumeric)
                && bytes.last() != Some(&b'0'))
}
fn invalid() -> ListError {
    ListError("Saved view positions cannot be reordered safely.")
}
fn integer(key: &str) -> u64 {
    // All callers validate canonical ASCII before slicing.
    key[..10].parse().unwrap()
}
fn fraction(key: &str) -> &str {
    key.get(11..).unwrap_or("")
}
fn format_integer(value: u64) -> Result<String, ListError> {
    (value < LIMIT)
        .then(|| format!("{value:010}"))
        .ok_or_else(invalid)
}
fn midpoint(left: &[u8], right: Option<&[u8]>) -> Result<Vec<u8>, ListError> {
    if right.is_some_and(|right| left >= right)
        || left.last() == Some(&b'0')
        || right.is_some_and(|right| right.last() == Some(&b'0'))
    {
        return Err(invalid());
    }
    if let Some(right) = right.filter(|right| !right.is_empty()) {
        let common = right
            .iter()
            .enumerate()
            .take_while(|(index, value)| left.get(*index).copied().unwrap_or(b'0') == **value)
            .count();
        if common > 0 {
            let mut out = right[..common].to_vec();
            out.extend(midpoint(
                left.get(common..).unwrap_or(&[]),
                Some(&right[common..]),
            )?);
            return Ok(out);
        }
    }
    let low = left
        .first()
        .map_or(0, |c| DIGITS.iter().position(|d| d == c).unwrap());
    let high = match right {
        None => DIGITS.len(),
        Some(right) => right
            .first()
            .and_then(|c| DIGITS.iter().position(|d| d == c))
            .ok_or_else(invalid)?,
    };
    if high > low + 1 {
        return Ok(vec![DIGITS[(low + high).div_ceil(2)]]);
    }
    if let Some(right) = right.filter(|right| right.len() > 1) {
        return Ok(vec![right[0]]);
    }
    let mut out = vec![DIGITS[low]];
    out.extend(midpoint(left.get(1..).unwrap_or(&[]), None)?);
    Ok(out)
}

pub(in crate::slack_lists) fn between(
    left: Option<&str>,
    right: Option<&str>,
) -> Result<String, ListError> {
    if left.is_some_and(|key| !valid(key))
        || right.is_some_and(|key| !valid(key))
        || left.zip(right).is_some_and(|(left, right)| left >= right)
    {
        return Err(invalid());
    }
    let out = match (left, right) {
        (None, None) => format_integer(5_000_000_000)?,
        (None, Some(right)) => {
            let value = integer(right);
            // Preserve the source's unpadded string comparison exactly.
            if value.to_string().as_str() < right && !fraction(right).is_empty() {
                format_integer(value)?
            } else {
                format_integer(value.checked_sub(1).ok_or_else(invalid)?)?
            }
        }
        (Some(left), None) => format_integer(integer(left) + 1)?,
        (Some(left), Some(right)) => {
            let low = integer(left);
            let high = integer(right);
            let middle = (low + high) / 2;
            if low != high && middle > low && middle < high {
                format_integer(middle)?
            } else {
                let fraction = midpoint(
                    fraction(left).as_bytes(),
                    (low == high).then(|| fraction(right).as_bytes()),
                )?;
                format!(
                    "{}.{fraction}",
                    format_integer(low)?,
                    fraction = String::from_utf8(fraction).unwrap()
                )
            }
        }
    };
    // No truncation or normalization when density exceeds the native bound.
    if !valid(&out)
        || left.is_some_and(|left| left >= out.as_str())
        || right.is_some_and(|right| out.as_str() >= right)
    {
        return Err(invalid());
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
