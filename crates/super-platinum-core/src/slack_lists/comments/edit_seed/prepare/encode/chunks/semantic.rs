use super::*;
use icu_normalizer::ComposingNormalizerBorrowed;
use regress::Regex;
use std::sync::OnceLock;

pub(super) fn real_mention(op: &Value) -> Result<bool> {
    let mention = op["attributes"]["slackmention"]
        .as_object()
        .ok_or(SeedUnavailable::MalformedMessage)?;
    let id = mention.get("id").and_then(Value::as_str).unwrap_or("");
    static SHORTCUT: OnceLock<std::result::Result<Regex, SeedUnavailable>> = OnceLock::new();
    let regex=SHORTCUT.get_or_init(||Regex::with_flags(r"https://(dev\.|dev([0-9]+)\.)?((slack|slack-gov|slack-gov-dev|slack-mil-dev)\.com|slack\.mil)/shortcuts/(Ft[A-Za-z0-9_-]{8,})/([A-Za-z0-9_-]{32})","gi").map_err(|_|SeedUnavailable::Patterns)).as_ref().map_err(Clone::clone)?;
    Ok(id != "UNVERIFIED" && regex.find(id).is_none())
}
pub(in super::super) fn mention(
    op: &Value,
    preformatted: bool,
    context: &mut Context<'_>,
) -> Result<Vec<Value>> {
    let mention = &op["attributes"]["slackmention"];
    if preformatted
        || truthy(op["attributes"].get("code"))
        || truthy(mention.get("privateAgentMention"))
    {
        return text::chunks(op, false, false, true);
    }
    let id = mention
        .get("id")
        .and_then(Value::as_str)
        .ok_or(SeedUnavailable::MalformedMessage)?;
    let label = mention
        .get("label")
        .and_then(Value::as_str)
        .ok_or(SeedUnavailable::MalformedMessage)?;
    let mut chunk = if [
        "BKhere",
        "BKchannel",
        "BKeveryone",
        "BKgroup",
        "BKsf:accountteam",
        "BKsf:dealteam",
        "BKassistant:einstein",
    ]
    .contains(&id)
    {
        json!({"type":"broadcast","range":&id[2..]})
    } else if id.starts_with('S') {
        json!({"type":"usergroup","usergroup_id":id})
    } else if id.len() >= 9
        && matches!(id.as_bytes()[0], b'C' | b'G' | b'D' | b'Z')
        && id
            .bytes()
            .skip(1)
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        let mut value = json!({"type":"channel","channel_id":id});
        if truthy(mention.get("tabId")) {
            value["tab_id"] = mention["tabId"].clone();
        }
        value
    } else if (9..=15).contains(&id.len())
        && id.starts_with('F')
        && id
            .bytes()
            .skip(1)
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        json!({"type":if context.file(id)? {"list_record"}else{"file"},"file_id":id})
    } else {
        json!({"type":"user","user_id":id})
    };
    chunk = text::annotate(chunk, op);
    let value = op["insert"].as_str().unwrap();
    if value.encode_utf16().count() > label.encode_utf16().count() {
        // JS split("") uses UTF-16 units; no scalar substitution.
        let count = if label.is_empty() {
            value.encode_utf16().count().saturating_sub(1)
        } else {
            value.matches(label).count()
        };
        context.reserve(&chunk, count.max(1))?;
        return Ok(vec![chunk; count.max(1)]);
    }
    let nfc = ComposingNormalizerBorrowed::new_nfc();
    if nfc.normalize(value) == nfc.normalize(label) {
        Ok(vec![chunk])
    } else {
        text::chunks(op, false, false, true)
    }
}
pub(super) fn emoji(
    op: &Value,
    preformatted: bool,
    context: &mut Context<'_>,
) -> Result<Vec<Value>> {
    let value = op["insert"]["slackemoji"]
        .as_object()
        .ok_or(SeedUnavailable::MalformedMessage)?;
    if preformatted || truthy(op["attributes"].get("code")) {
        return text::chunks(
            &json!({"insert":value.get("text").cloned().unwrap_or(json!(""))}),
            false,
            false,
            true,
        );
    }
    let code = match value.get("text") {
        Some(Value::String(value)) => value.as_str(),
        None if value.is_empty() => "",
        _ => return Err(SeedUnavailable::MalformedMessage),
    };
    let mut chunk = json!({"type":"emoji","name":""});
    if let Some(standard) = context.emoji(code)? {
        chunk["name"] = json!(
            standard
                .name
                .unwrap_or_default()
                .split("::")
                .next()
                .unwrap_or("")
        );
        chunk["unicode"] = json!(standard.unicode.unwrap_or_default());
    } else if let Some((name, tone)) = parse_emoji(code)? {
        chunk["name"] = json!(name);
        if let Some(tone) = tone {
            chunk["skin_tone"] = tone.parse::<u8>().map_or(Value::Null, |v| json!(v));
        }
    }
    if !truthy(chunk.get("name")) && !truthy(chunk.get("unicode")) {
        return Ok(vec![]);
    }
    Ok(vec![text::annotate(chunk, op)])
}
pub(in super::super) fn parse_emoji(code: &str) -> Result<Option<(String, Option<String>)>> {
    // ue25's ordinary-object cache has inherited names. They are not a parsed
    // colon token and need an explicit unsupported boundary, not invented data.
    if [
        "constructor",
        "toString",
        "valueOf",
        "__proto__",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toLocaleString",
        "__defineGetter__",
        "__defineSetter__",
        "__lookupGetter__",
        "__lookupSetter__",
    ]
    .contains(&code)
    {
        return Err(SeedUnavailable::MalformedMessage);
    }
    if code.is_empty() {
        return Ok(None);
    }
    let lower = code.to_lowercase();
    let text = lower.strip_prefix(':').unwrap_or(&lower);
    let text = text.strip_suffix(':').unwrap_or(text);
    static PARSED: OnceLock<std::result::Result<Regex, SeedUnavailable>> = OnceLock::new();
    let regex = PARSED
        .get_or_init(|| {
            Regex::new(r"::[^:]+-([2-6]-[2-6])|::[^:]+-([2-6])$")
                .map_err(|_| SeedUnavailable::Patterns)
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let units: Vec<_> = text.encode_utf16().collect();
    let found = regex.find_from_ucs2(&units, 0).next();
    let (name, tone) = if let Some(found) = found {
        let name = [&units[..found.range.start], &units[found.range.end..]].concat();
        let group = found
            .group(1)
            .or_else(|| found.group(2))
            .ok_or(SeedUnavailable::Patterns)?;
        (
            String::from_utf16(&name).map_err(|_| SeedUnavailable::InvalidUnicode)?,
            Some(String::from_utf16(&units[group]).map_err(|_| SeedUnavailable::InvalidUnicode)?),
        )
    } else {
        (text.to_owned(), None)
    };
    if name.is_empty() || name.contains(':') {
        Ok(None)
    } else {
        Ok(Some((name, tone)))
    }
}
