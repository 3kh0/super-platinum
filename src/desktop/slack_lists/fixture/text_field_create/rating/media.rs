//! Local custom content only. A disconnected fixture never starts media fetches.
use super::*;
const NAME: &str = "fixture_quality";
const URL: &str = "https://fixture.invalid/rating-quality.png";
pub(crate) fn seed(shell: &mut ShellState) -> Result<(), String> {
    if !super::super::super::enabled(shell)
        || shell.core.active_team.as_deref() != Some("T1")
        || !create::field_create_mode().is_some_and(create::FieldCreateMode::is_rating)
    {
        return Err("Rating media seed gate".into());
    }
    let workspace = shell
        .core
        .workspaces
        .get_mut("T1")
        .ok_or("Missing fixture workspace")?;
    if workspace.custom_emoji.contains_key(NAME) {
        return Err("Custom Rating identity already exists".into());
    }
    workspace.custom_emoji.insert(
        NAME.into(),
        super_platinum_core::slack::models::Emoji {
            name: NAME.into(),
            value: URL.into(),
            ..Default::default()
        },
    );
    workspace.custom_emoji_revision += 1;
    workspace.emoji_preferences.replace(&Default::default());
    let id = shell.media.register_emoji(NAME, URL);
    if shell.media.is_ready(&id) {
        return Err("Rating custom media unexpectedly ready".into());
    }
    Ok(())
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || shell.core.active_team.as_deref() != Some("T1")
    {
        return Err("Rating media isolation lost".into());
    }
    let workspace = shell
        .core
        .workspaces
        .get("T1")
        .ok_or("Missing current Rating workspace")?;
    let e = workspace
        .custom_emoji
        .get(NAME)
        .ok_or("Missing fixed custom Rating emoji")?;
    if e.name != NAME || e.value != URL {
        return Err("Rating custom source changed".into());
    }
    let id = shell.media.register_emoji(NAME, URL);
    Ok(
        json!({"team":"T1","name":NAME,"source":URL,"revision":workspace.custom_emoji_revision,"ready":shell.media.is_ready(&id),"noSession":true,"noTransport":true}),
    )
}
pub(crate) fn ready(shell: &mut ShellState) -> Result<Value, String> {
    let before = snapshot(shell)?;
    let s = state(shell)?;
    if s.mode != create::FieldCreateMode::RatingCustom
        || s.reads.len() != 1
        || !s.navigations.is_empty()
        || before["ready"] != false
        || !operation(shell)?.is_null()
    {
        return Err("Custom image transition out of order".into());
    }
    let id = shell.media.register_emoji(NAME, URL);
    let bytes: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/icons/png/icon_16.png"
    ));
    shell
        .media
        .insert(id, "image/png", std::sync::Arc::<[u8]>::from(bytes));
    // The existing dirty tick owns media_epoch/rendering; do not manufacture it.
    let after = snapshot(shell)?;
    if after["ready"] != true {
        return Err("Local fixture bytes not inserted".into());
    }
    Ok(
        json!({"before":before,"after":after,"asset":"assets/icons/png/icon_16.png","bytes":bytes.len(),"network":false}),
    )
}
