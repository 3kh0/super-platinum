//! Original Save-owned full catalog read. Never publishes picker/workspace data.
use super::*;
pub(super) use super_platinum_core::slack::api::emoji_catalog::Complete;
use super_platinum_core::{
    slack::api::emoji_catalog::Read,
    slack_lists::comments::edit_seed::prepare::{EmojiEncoding, StandardEmoji, emoji_name},
};

pub(super) async fn acquire(
    state: Signal<ShellState>,
    facts: &Facts,
    policy: &own_edit_policy::Observation,
) -> Result<Arc<Complete>, Error> {
    let current = || facts.current(&state.peek()) && policy.current(&state.peek());
    if facts.custom_revision.is_none() || !current() {
        return Err(missing("owner"));
    }
    let context = &facts.context;
    let request = Read::new(&context.session).request(&context.client, &context.session)?;
    let result = if crate::fixture::is_fixture() {
        crate::slack_lists::comments::fixture::completion::plain_custom_emoji::read(state, &request)
            .await
            .unwrap_or_else(|| Err(Error::Offline("Closed custom emoji catalog fixture".into())))
    } else {
        context.transport.execute_current(request, current).await
    };
    // An already dispatched read may complete after retirement; its raw result
    // is not a current catalog and cannot advance preparation or issue an edit.
    if !current() {
        return Err(missing("owner"));
    }
    let raw = result?;
    let receipt = tokio::task::spawn_blocking(move || Complete::decode(raw))
        .await
        .map_err(|_| missing("worker"))??;
    if !current() {
        return Err(missing("owner"));
    }
    Ok(Arc::new(receipt))
}

pub(super) fn encoding(
    standard: &StandardEmoji,
    custom: Option<&Complete>,
    text: &str,
) -> Result<Option<EmojiEncoding>, Error> {
    let value = standard
        .encoding(text)
        .map_err(|_| missing("emoji_encoding"))?;
    if value.is_some() {
        return Ok(value);
    }
    let name = emoji_name(text)
        .map_err(|_| missing("emoji_encoding"))?
        .ok_or_else(|| missing("emoji_encoding"))?;
    if custom.is_some_and(|receipt| receipt.names().contains(&name)) {
        Ok(None)
    } else {
        Err(missing("custom_name"))
    }
}

/// Positive standard membership avoids an unrelated custom read. A valid
/// nonstandard semantic embed requires the original complete catalog once.
pub(super) fn needs_catalog(
    standard: &StandardEmoji,
    custom: Option<&Complete>,
    text: &str,
) -> Result<bool, Error> {
    if standard
        .encoding(text)
        .map_err(|_| missing("emoji_encoding"))?
        .is_some()
    {
        return Ok(false);
    }
    emoji_name(text)
        .map_err(|_| missing("emoji_encoding"))?
        .ok_or_else(|| missing("emoji_encoding"))?;
    Ok(custom.is_none())
}
