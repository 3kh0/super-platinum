//! Exact retained hvK/ pA/rZ expressions used by both TbRQ and IRZY.
use regress::Regex;
use std::sync::OnceLock;
pub(super) fn slack_identity(url: &str) -> Option<(&'static str, String)> {
    static USER: OnceLock<Regex> = OnceLock::new();
    static CHANNEL: OnceLock<Regex> = OnceLock::new();
    let patterns=[
        ("user",USER.get_or_init(||Regex::new(r"https://((.+?)(\.dev[0-9]*?|\.staging)?\.)?slack\.com/team/((U|W)[A-Z0-9]{8,})").expect("retained fixed user URL"))),
        ("channel",CHANNEL.get_or_init(||Regex::new(r"https://((.+?)(\.dev[0-9]*?|\.staging)?\.)?slack\.com/archives/((C|G|D)[A-Z0-9]{8,})$").expect("retained fixed channel URL"))),
    ];
    let units = url.encode_utf16().collect::<Vec<_>>();
    for (kind, pattern) in patterns {
        if let Some(found) = pattern.find_from_ucs2(&units, 0).next()
            && let Some(Some(range)) = found.captures.get(3)
        {
            return Some((kind, String::from_utf16(&units[range.clone()]).ok()?));
        }
    }
    None
}
