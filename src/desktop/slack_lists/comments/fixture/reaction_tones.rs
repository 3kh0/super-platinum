//! Original tone names and locally supplied custom content; no action admission.
use super::*;
pub(crate) const FIRST: &str = "1781524801.000001";
pub(crate) const CUSTOM: &str = "fixture_tone_custom";
const URL: &str = "https://fixture.invalid/reaction-tone-custom.png";
pub(crate) const NAMES: &[&str] = &[
    "thumbsup::skin-tone-3",
    "+1::skin-tone-3",
    "handshake::skin-tone-2-6",
    "wave",
    CUSTOM,
    "not_real::skin-tone-3",
];
pub(crate) fn enabled(shell: &ShellState) -> bool {
    compose::enabled(shell)
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("reaction-tones")
}
pub(super) fn seed(shell: &mut ShellState) -> Result<(), String> {
    if !enabled(shell) || reaction_actions::enabled(shell) {
        return Err("Closed display-only tone fixture required".into());
    }
    let workspace = shell
        .core
        .workspaces
        .get_mut("T1")
        .ok_or("Workspace missing")?;
    if workspace.custom_emoji.contains_key(CUSTOM) {
        return Err("Tone custom identity already exists".into());
    }
    workspace.custom_emoji.insert(
        CUSTOM.into(),
        super_platinum_core::slack::models::Emoji {
            name: CUSTOM.into(),
            value: URL.into(),
            ..Default::default()
        },
    );
    workspace.custom_emoji_revision += 1;
    workspace.emoji_preferences.replace(&Default::default());
    let id = shell.media.register_emoji(CUSTOM, URL);
    let bytes: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/icons/png/icon_16.png"
    ));
    shell
        .media
        .insert(id, "image/png", Arc::<[u8]>::from(bytes));
    Ok(())
}
fn reactions(updated: bool) -> Value {
    Value::Array(NAMES.iter().map(|name| {
        let alias = updated && *name == "+1::skin-tone-3";
        json!({"name":name,"count":if alias {2}else{1},"users":if alias {json!(["U1","U_RT_REACTOR"])}else{json!(["U1"])}})
    }).collect())
}
pub(super) fn decorate(raw: &mut Value) {
    if let Some(row) = raw["messages"]
        .as_array_mut()
        .and_then(|rows| rows.iter_mut().find(|r| r["ts"] == FIRST))
    {
        row["reactions"] = reactions(false);
        row["tone_fixture_opaque"] = json!({"keep":"original raw names"});
    }
}
pub(crate) fn server(shell: &ShellState, model: &Model) -> Result<Value, String> {
    if !enabled(shell)
        || !shell
            .slack_lists
            .comments
            .fixture
            .as_ref()
            .is_some_and(|m| Arc::ptr_eq(&m.0, &model.0))
    {
        return Err("Original tone fixture owner required".into());
    }
    let mut ledger = model.0.lock().map_err(|_| "Tone ledger poisoned")?;
    ledger
        .reaction_readbacks
        .insert(FIRST.into(), reactions(true));
    let mut raw = reply(ROOT, false);
    decorate(&mut raw);
    apply_reaction_readbacks(&ledger.reaction_readbacks, &mut raw);
    Ok(raw)
}
pub(crate) fn media(shell: &ShellState) -> Result<Value, String> {
    if !enabled(shell) {
        return Err("Tone fixture required".into());
    }
    let source = shell
        .core
        .workspaces
        .get("T1")
        .and_then(|w| w.custom_emoji.get(CUSTOM))
        .ok_or("Custom tone missing")?;
    let id = shell.media.register_emoji(CUSTOM, URL);
    Ok(
        json!({"name":source.name,"source":source.value,"ready":shell.media.is_ready(&id),"asset":"assets/icons/png/icon_16.png","network":false}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_aliases_are_distinct_and_future_read_never_rewrites_original() {
        let mut original = reply(ROOT, false);
        decorate(&mut original);
        let retained = original.clone();
        let mut future = original.clone();
        apply_reaction_readbacks(
            &BTreeMap::from([(FIRST.into(), reactions(true))]),
            &mut future,
        );
        assert_eq!(original, retained);
        let a = original["messages"][1]["reactions"].as_array().unwrap();
        assert_eq!(
            a.iter()
                .map(|r| r["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            NAMES
        );
        assert_eq!(future["messages"][1]["reactions"][0]["count"], 1);
        assert_eq!(future["messages"][1]["reactions"][1]["count"], 2);
        assert_eq!(
            future["messages"][1]["tone_fixture_opaque"],
            retained["messages"][1]["tone_fixture_opaque"]
        );
    }
}
