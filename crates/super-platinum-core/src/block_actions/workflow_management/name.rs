//! kJYS validation, with JavaScript UTF-16 length and trim semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    Reserved,
    RestrictedCharacters,
    SpecialCharacters,
    Emoji,
    Required,
    TooLong,
}
pub fn validate_name(name: &str) -> Result<(), NameError> {
    if matches!(js_trim(&name.to_lowercase()), "constructor" | "__proto__")
        || RESERVED.contains(&js_trim(&name.to_lowercase()))
    {
        return Err(NameError::Reserved);
    }
    if name
        .chars()
        .any(|c| "`~!@#$%^&*+=\\:\"<>?|[]/,â\u{80}\u{8b}".contains(c))
    {
        return Err(NameError::RestrictedCharacters);
    }
    let punctuation_only =
        !name.is_empty() && name.chars().all(|c| "-_.'‘’\",(){}[]/;".contains(c));
    if punctuation_only||name.chars().any(|c|matches!(c as u32,0x3000..=0x3020|0x3029..=0x3030|0x3036..=0x303f|0x3099..=0x309c|0x2026|0x30fb|0x30fd..=0x30ff|0xff01..=0xff0f|0xff1a..=0xff20|0xff3b..=0xff40|0xff9e..=0xff9f|0xffe0..=0xffef|0xff5b..=0xff65)){return Err(NameError::SpecialCharacters);}
    if name.chars().any(|c|matches!(c as u32,0x10000..=0x10ffff|0x2700..=0x27bf|0x3299|0x3297|0x303d|0x3030|0x24c2|0x203c|0x2049|0x25aa..=0x25ab|0x25b6|0x25c0|0x25fb..=0x25fe|0xa9|0xae|0x2122|0x2139|0x2600..=0x26ff|0x2b05..=0x2b07|0x2b1b|0x2b1c|0x2b50|0x2b55|0x231a|0x231b|0x2328|0x23cf|0x23e9..=0x23f3|0x23f8..=0x23fa|0x2934|0x2935|0x2190..=0x21ff)) || keycap(name){return Err(NameError::Emoji);}
    if js_trim(name).is_empty() {
        return Err(NameError::Required);
    }
    if name.encode_utf16().count() > 80 {
        return Err(NameError::TooLong);
    }
    Ok(())
}
pub fn description_invalid(description: &str, maximum: usize) -> bool {
    js_trim(description).is_empty() || description.encode_utf16().count() > maximum
}
fn js_trim(value: &str) -> &str {
    value.trim_matches(|c:char|matches!(c as u32,0x9..=0xd|0x20|0xa0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff))
}
fn keycap(value: &str) -> bool {
    let chars: Vec<_> = value.chars().collect();
    chars.iter().enumerate().any(|(i, c)| {
        matches!(*c, '#'..='9')
            && (chars.get(i + 1) == Some(&'\u{20e3}')
                || chars.get(i + 1) == Some(&'\u{fe0f}') && chars.get(i + 2) == Some(&'\u{20e3}'))
    })
}
// Exact kJYS map keys, including original capitalization. The two inherited
// Object.prototype keys are deliberately checked above: captured code uses
// ordinary property lookup, not an own-property test.
const RESERVED: &[&str] = &[
    "alle",
    "allgemein",
    "aquí",
    "canais",
    "canal",
    "chaîne",
    "channel",
    "conversation",
    "eu",
    "everyone",
    "general",
    "général",
    "geral",
    "group",
    "groupe",
    "grupo",
    "gruppe",
    "here",
    "hier",
    "ici",
    "mí",
    "mir",
    "moi",
    "slack corp",
    "slack",
    "slackbot",
    "teambot",
    "todos",
    "tous",
    "ぐるーぷ",
    "グループ",
    "ここ",
    "ココ",
    "じぶん",
    "ジブン",
    "ぜんいん",
    "ゼンイン",
    "ちゃんねる",
    "チャンネル",
    "全員",
    "此処",
    "自分",
    "나",
    "그룹",
    "채널",
    "모두",
    "여기",
    "전체",
    "모든",
    "전부",
    "tutti",
    "canale",
    "gruppo",
    "qui",
    "me",
    "tu",
    "归档",
    "已归档",
    "全部",
    "频道",
    "创建",
    "删除",
    "已删除的频道",
    "编辑",
    "每个人",
    "组",
    "此处",
    "我",
    "今天",
    "你",
    "全部",
    "頻道",
    "頻道",
    "任意",
    "我",
    "您",
    "一般",
    "Slack 機器人",
    "архив",
    "архивированный",
    "архивный",
    "все",
    "вы",
    "группа",
    "группы",
    "здесь",
    "канал",
    "каналы",
    "общий",
    "редактировать",
    "сегодня",
    "создать",
    "удаленный канал",
    "удалить",
    "я",
];

#[cfg(test)]
mod tests;
