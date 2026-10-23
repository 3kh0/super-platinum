//! Save-owned standard facts, separate from the provisional V16 picker cache.
use super::*;
use super_platinum_core::slack_lists::comments::edit_seed::prepare::{
    EmojiObservations, EmojiTranslation, StandardEmoji, ToneSupport,
};

pub(super) fn observations(
    last_viewed_locale: &str,
    user_locale: Option<&str>,
    standard: Option<&StandardEmoji>,
    custom: Option<&custom::Complete>,
) -> EmojiObservations {
    let mut observed = EmojiObservations {
        current_locale: Some(last_viewed_locale.into()),
        locale: user_locale.map(str::to_owned),
        // BUnC's en-US initialization installs keywords but no HSTN names map.
        // A different/missing acquired locale is not identity translation.
        translation: if user_locale == Some("en-US") {
            EmojiTranslation::KnownAbsent
        } else {
            EmojiTranslation::Unobserved
        },
        ..Default::default()
    };
    if let Some(standard) = standard {
        observed.positive_names = standard.positive_names().clone();
        if let Some(custom) = custom {
            observed.names = Some(
                standard
                    .positive_names()
                    .union(custom.names())
                    .cloned()
                    .collect(),
            );
        }
        // rNCs passes teamId:void 0 to bm2k. Its UHpe.Fm/pA lookups therefore
        // receive absent state and empty teamId and positively return false.
        // This is not the live composer's catalog-support/preferred-tone test.
        observed.tones = observed
            .names
            .as_ref()
            .unwrap_or(&observed.positive_names)
            .iter()
            .cloned()
            .chain(std::iter::once(String::new()))
            .map(|name| ToneSupport {
                name,
                single: false,
                multiple: false,
            })
            .collect();
    }
    observed
}

pub(super) async fn acquire(
    state: Signal<ShellState>,
    facts: &Facts,
    policy: &own_edit_policy::Observation,
    proof: &Value,
) -> Result<Arc<StandardEmoji>, Error> {
    if proof["userPreferences"]["prefs"]["locale"] != "en-US"
        || !facts.current(&state.peek())
        || !policy.current(&state.peek())
    {
        return Err(missing("emoji_locale"));
    }
    let version = plain_observations::emoji_version(&proof["assignments"])?;
    let standard = tokio::task::spawn_blocking(move || StandardEmoji::build(version))
        .await
        .map_err(|_| missing("worker"))?
        .map_err(|_| missing("standard_catalog"))?;
    if !facts.current(&state.peek()) || !policy.current(&state.peek()) {
        return Err(missing("owner"));
    }
    Ok(Arc::new(standard))
}
