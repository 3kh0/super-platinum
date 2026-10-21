//! Closed complete preference observation and independent selected-reaction replies.
use super::*;
pub(crate) const FIRST: &str = "1781524801.000001";
pub(crate) const CUSTOM: &str = "fixture_selection_custom";
pub(crate) const ALIAS: &str = "fixture_selection_alias";
pub(crate) const TONE: &str = "+1::skin-tone-4";
const URL: &str = "https://fixture.invalid/selection-custom.png";
pub(crate) fn enabled(s: &ShellState) -> bool {
    compose::enabled(s)
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("reaction-selection" | "reaction-selection-failures" | "reaction-selection-custody")
        )
}
pub(crate) fn allowed(method: &str, name: &str) -> bool {
    matches!(method, "reactions.add" | "reactions.remove") && [TONE, CUSTOM, "eyes"].contains(&name)
}
fn preferences() -> Value {
    json!({"ok":true,"prefs":{"preferred_skin_tone":4,"emoji_mode":"native","emoji_use":{},"frecency":"{}"},"fixture_opaque":"original complete prefs"})
}
pub(super) fn seed(s: &mut ShellState) -> Result<Value, String> {
    if !enabled(s) {
        return Err("Original closed selection fixture required".into());
    }
    let raw = preferences();
    let parsed = super_platinum_core::composer_emoji::api::parse_preferences(&raw)
        .map_err(|e| e.to_string())?;
    let w = s
        .core
        .workspaces
        .get_mut("T1")
        .ok_or("Selection workspace missing")?;
    w.emoji_preferences.replace(&parsed);
    for (name, value) in [(CUSTOM, URL.to_owned()), (ALIAS, format!("alias:{CUSTOM}"))] {
        if w.custom_emoji.contains_key(name) {
            return Err("Selection custom identity already exists".into());
        }
        w.custom_emoji.insert(
            name.into(),
            super_platinum_core::slack::models::Emoji {
                name: name.into(),
                value,
                ..Default::default()
            },
        );
    }
    w.custom_emoji_revision += 1;
    let adopted = json!({"raw":raw,"parsed":parsed,"preferences":w.emoji_preferences,"customRevision":w.custom_emoji_revision,"custom":w.custom_emoji.iter().filter(|(name,_)|[CUSTOM,ALIAS].contains(&name.as_str())).map(|(name,row)|(name.clone(),json!({"name":row.name,"value":row.value}))).collect::<BTreeMap<_,_>>()});
    for name in [CUSTOM, ALIAS] {
        let id = s.media.register_emoji(name, URL);
        s.media.insert(
            id,
            "image/png",
            Arc::<[u8]>::from(
                &include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/icons/png/icon_16.png"
                ))[..],
            ),
        );
    }
    Ok(adopted)
}
pub(crate) fn initial_reactions() -> Value {
    json!([
        {"name":"+1::skin-tone-2","count":1,"users":["U0"]},
        {"name":"thumbsup::skin-tone-4","count":1,"users":["U0"]},
        {"name":"eyes","count":1,"users":["U0"]}
    ])
}
pub(super) fn decorate(raw: &mut Value) {
    if let Some(row) = raw["messages"]
        .as_array_mut()
        .and_then(|rows| rows.iter_mut().find(|r| r["ts"] == FIRST))
    {
        row["reactions"] = initial_reactions();
        row["selection_fixture_opaque"] =
            json!({"keep":"exact selection differs from raw-base family"});
    }
}
pub(crate) fn server(s: &ShellState, model: &Model, reactions: Value) -> Result<Value, String> {
    if !enabled(s)
        || !s
            .slack_lists
            .comments
            .fixture
            .as_ref()
            .is_some_and(|m| Arc::ptr_eq(&m.0, &model.0))
        || !reactions.is_array()
    {
        return Err("Original selection source required".into());
    }
    let mut l = model.0.lock().map_err(|_| "Selection ledger poisoned")?;
    l.reaction_readbacks.insert(FIRST.into(), reactions);
    let mut raw = reply(ROOT, false);
    decorate(&mut raw);
    apply_reaction_readbacks(&l.reaction_readbacks, &mut raw);
    Ok(raw)
}
pub(crate) fn observed(s: &ShellState, model: &Model) -> Result<Value, String> {
    if !enabled(s) {
        return Err("Closed selection fixture required".into());
    }
    let w = s
        .core
        .workspaces
        .get("T1")
        .ok_or("Selection workspace missing")?;
    let original = model
        .0
        .lock()
        .map_err(|_| "Selection ledger poisoned")?
        .selection_seed
        .clone();
    let custom = w
        .custom_emoji
        .iter()
        .filter(|(name, _)| [CUSTOM, ALIAS].contains(&name.as_str()))
        .map(|(name, row)| (name.clone(), json!({"name":row.name,"value":row.value})))
        .collect::<BTreeMap<_, _>>();
    Ok(
        json!({"original":original,"preferences":w.emoji_preferences,"customRevision":w.custom_emoji_revision,"custom":custom,"catalog":crate::emoji_catalog::current(s).map(|c|json!({"token":c.token,"current":c.current(s)}))}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_preference_seed_is_parsed_not_defaulted() {
        let raw = preferences();
        let parsed = super_platinum_core::composer_emoji::api::parse_preferences(&raw).unwrap();
        let mut prefs = super_platinum_core::composer_emoji::preferences::Preferences::default();
        assert!(!prefs.loaded());
        assert!(prefs.replace(&parsed));
        assert_eq!(prefs.preferred_tone(), Some("4"));
        assert_eq!(prefs.raw().get("preferred_skin_tone"), Some(&json!(4)));
    }
    #[test]
    fn selected_wire_allowlist_does_not_admit_family_siblings_or_alias_text() {
        for method in ["reactions.add", "reactions.remove"] {
            for name in [TONE, CUSTOM, "eyes"] {
                assert!(allowed(method, name));
            }
        }
        for name in [
            "+1",
            "+1::skin-tone-2",
            "thumbsup::skin-tone-4",
            ALIAS,
            "wave",
            "",
        ] {
            assert!(!allowed("reactions.remove", name));
        }
        assert!(!allowed("chat.postMessage", TONE));
    }
    #[test]
    fn actual_catalog_and_complete_preferences_resolve_closed_choices() {
        use super_platinum_core::composer_emoji::{
            catalog::{Catalog, ReactionChoice},
            preferences::Preferences,
            standard::Version,
        };
        let catalog = Catalog::build(
            Version::V16,
            vec![
                super_platinum_core::slack::models::Emoji {
                    name: CUSTOM.into(),
                    value: URL.into(),
                    ..Default::default()
                },
                super_platinum_core::slack::models::Emoji {
                    name: ALIAS.into(),
                    value: format!("alias:{CUSTOM}"),
                    ..Default::default()
                },
            ],
        )
        .unwrap();
        let mut prefs = Preferences::default();
        prefs.replace(
            &super_platinum_core::composer_emoji::api::parse_preferences(&preferences()).unwrap(),
        );
        let reactions: Vec<super_platinum_core::slack::models::Reaction> =
            serde_json::from_value(initial_reactions()).unwrap();
        for (selected, expected) in [
            ("+1", TONE),
            (CUSTOM, CUSTOM),
            (ALIAS, CUSTOM),
            ("eyes", "eyes"),
        ] {
            assert!(catalog.names.iter().any(|name| name == selected));
            assert_eq!(
                ReactionChoice::resolve(&catalog, &prefs, &reactions, selected)
                    .unwrap()
                    .name(),
                expected
            );
        }
    }
}
