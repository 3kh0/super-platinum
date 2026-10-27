//! Closed canonical emoji title vectors through the existing file lifecycle.
use super::*;
use serde_json::json;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_EMOJI_TITLE_CHECK";
pub(crate) const CASE: &str = "SUPER_PLATINUM_PROFILE_EMOJI_TITLE_CASE";
pub(crate) fn case() -> &'static str {
    match std::env::var(CASE).as_deref() {
        Ok("empty") => "empty",
        Ok("tone") => "tone",
        Ok("tone-empty") => "tone-empty",
        _ => "ordinary",
    }
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && std::env::var(mentions::FLAG).as_deref() == Ok("1")
        && std::env::var(FLAG).as_deref() == Ok("1")
        && matches!(
            std::env::var(CASE).as_deref(),
            Ok("ordinary" | "empty" | "tone" | "tone-empty")
        )
        && std::env::var(files::FLAG).as_deref() == Ok("1")
        && std::env::var(files::BLOCKS_FLAG).as_deref() == Ok("1")
        && !files::list()
        && std::env::var(link_title::FLAG).as_deref() != Ok("1")
        && std::env::var(empty_canvas::FLAG).as_deref() != Ok("1")
        && std::env::var("SUPER_PLATINUM_PROFILE_USER_TITLE_CHECK").as_deref() != Ok("1")
        && std::env::var(completion::FLAG).as_deref() != Ok("1")
        && std::env::var("SUPER_PLATINUM_PROFILE_FILE_COMPLETION_CHECK").as_deref() != Ok("1")
}
pub(crate) fn label(renamed: bool) -> &'static str {
    expected_label(case(), renamed)
}
fn expected_label(case: &str, renamed: bool) -> &'static str {
    match (case, renamed) {
        ("tone-empty", false) => "",
        ("tone-empty", true) => ":wave::skin-tone-5: Plan",
        ("tone", true) => "Project :wave::skin-tone-4: :wave: plan",
        ("empty", false) => "",
        ("empty", true) => ":wave: Plan",
        (_, false) => "Project plan",
        (_, true) => "Project :wave: plan",
    }
}
pub(crate) fn blocks(renamed: bool) -> Value {
    vector(case(), renamed)
}
fn vector(case: &str, renamed: bool) -> Value {
    let emoji = json!({"type":"emoji","name":"wave","style":{"bold":true},"opaque":{"keep":1}});
    let text = |s: &str| json!({"type":"text","text":s});
    let tone = |fields: Value| {
        let mut value = emoji.clone();
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        value
    };
    let leaves = match (case, renamed) {
        ("tone", false) => vec![
            tone(json!({"skinTone":3,"skin_tone":6})),
            text(" Project plan"),
        ],
        ("tone", true) => vec![
            text("Project "),
            tone(json!({"skinTone":4,"skin_tone":6})),
            text(" "),
            tone(json!({"skinTone":0})),
            text(" plan"),
        ],
        ("tone-empty", false) => vec![tone(json!({"skin_tone":2}))],
        ("tone-empty", true) => vec![
            tone(json!({"skinTone":null})),
            tone(json!({"skinTone":0,"skin_tone":5})),
            text(" Plan"),
        ],
        ("empty", false) => vec![emoji],
        ("empty", true) => vec![emoji.clone(), emoji, text(" Plan")],
        (_, false) => vec![emoji, text(" Project plan")],
        (_, true) => vec![text("Project "), emoji, text(" plan")],
    };
    json!([{"type":"rich_text","block_id":"kept-block","elements":[
        {"type":"rich_text_section","elements":leaves}]}])
}
pub(crate) fn snapshot() -> Value {
    json!({"case":case(),"original":blocks(false),"renamed":blocks(true),
        "label":label(false),"renamedLabel":label(true)})
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::slack::api::custom::texty::file_titles;

    #[test]
    fn mounted_vectors_share_dependency_and_label_admission_without_wire_mutation() {
        for case in ["ordinary", "empty", "tone", "tone-empty"] {
            for renamed in [false, true] {
                let effective =
                    json!({"title":"Wrong fallback","title_blocks":vector(case, renamed)});
                let before = effective.clone();
                let context = Some(file_titles::EmojiText::EnglishCanonicalNames);
                assert_eq!(
                    file_titles::dependencies_with_emoji_context(&effective, context).unwrap(),
                    file_titles::TitleDependencies::default()
                );
                let label = file_titles::label_with_emoji_context(
                    file_titles::Kind::Canvas,
                    file_titles::Status::Normal,
                    &effective,
                    file_titles::QuipTitle::Nullish,
                    Some("Canvas"),
                    context,
                    |_| panic!("Tone fixture must not request a member"),
                )
                .unwrap();
                assert_eq!(label, expected_label(case, renamed), "{case}/{renamed}");
                assert_eq!(effective, before);
            }
        }
    }
}
