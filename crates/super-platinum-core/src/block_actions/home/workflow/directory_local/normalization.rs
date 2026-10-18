//! XbX7.computeDerivedNames differs deliberately from hNDQ query normalization.
//! JS lowercase operations are caller-owned, so no host-locale guess is made.
use serde_json::Value;
mod table;

fn whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
fn deburr(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if matches!(c, '\u{0300}'..='\u{036f}' | '\u{fe20}'..='\u{fe2f}' | '\u{20d0}'..='\u{20ff}')
        {
            continue;
        }
        if let Some(mapped) = table::map(c) {
            out.push_str(mapped);
        } else {
            out.push(c);
        }
    }
    out
}
/// Input is exactly renderer `raw.trim().toLocaleLowerCase()`. The original raw
/// string must still be retained separately for hHTo's case-sensitive D-ID path.
/// This performs only the remaining J3F/ transform and JS /\s+/g collapse.
pub fn normalize_lowered_query(trimmed_locale_lowered: &str) -> String {
    let mut result = String::new();
    let mut in_space = false;
    for c in deburr(trimmed_locale_lowered).chars() {
        if whitespace(c) {
            if !in_space {
                result.push(' ');
            }
            in_space = true;
        } else {
            result.push(c);
            in_space = false;
        }
    }
    result
}
/// `locale_lowercase` must match the owning renderer's JS toLocaleLowerCase.
/// Called after ECMAScript trim; no locale-independent default is provided.
pub fn normalize_query_with_lowercase(
    raw: &str,
    locale_lowercase: impl FnOnce(&str) -> String,
) -> String {
    normalize_lowered_query(&locale_lowercase(raw.trim_matches(whitespace)))
}
#[derive(Clone, Debug)]
pub struct PreparedQuery {
    pub normalized: String,
    pub matcher_normalized: String,
}
/// The callback runs once per distinct normalization stage, as in hHTo/jytj.
pub fn prepare_query(raw: &str, locale_lowercase: impl Fn(&str) -> String) -> PreparedQuery {
    let normalized = normalize_query_with_lowercase(raw, &locale_lowercase);
    let matcher_normalized = normalize_query_with_lowercase(&normalized, locale_lowercase);
    PreparedQuery {
        normalized,
        matcher_normalized,
    }
}
/// Clone the actual member and derive the three fields read by hWCs, using
/// XbX7's already-effective profile. For self partial upserts, the caller first
/// preserves omitted email according to computeEffectiveProfile's actual flag.
/// `lowercase` must implement JS toLowerCase (not locale lowercase). No trimming
/// or whitespace collapsing occurs for member names; top-level real_name and
/// profile.real_name_normalized do not replace profile.real_name here.
pub fn derive_member_names(raw: &Value, lowercase: impl Fn(&str) -> String) -> Option<Value> {
    let mut member = raw.as_object()?.clone();
    for (derived, profile) in [
        ("_real_name_normalized_lc", "real_name"),
        ("_display_name_normalized_lc", "display_name"),
        ("_email_normalized_lc", "email"),
    ] {
        let name = raw["profile"][profile].as_str().unwrap_or("");
        member.insert(derived.into(), Value::String(deburr(&lowercase(name))));
    }
    Some(Value::Object(member))
}
