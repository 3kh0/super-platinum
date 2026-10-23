//! Exact ordinary IHC0 call settings and reached-only preparation.
use super::*;
use super_platinum_core::slack_lists::comments::edit_seed::SeedUnavailable;
use super_platinum_core::slack_lists::comments::edit_seed::prepare::{
    MentionNeed, MentionObservations, PlainPreparation, PreparationSettings, PreparationStep,
    StandardEmoji, SyntaxObservations,
};

pub(super) fn settings(proof: &Value) -> Result<(PreparationSettings, bool), Error> {
    let native =
        plain_observations::native_policy(&proof["userPreferences"], &proof["assignments"])?;
    if proof["plainTextMode"] != true
        || !native.plain_text_mode
        || native.supports_portable_content
        || native.markdown_mode
    {
        return Err(missing("policy"));
    }
    // native_policy above validated the complete original assignments carrier.
    let headers =
        proof["assignments"]["assignments"]["rich_text_headers_and_dividers"]["group"] == "on";
    Ok((
        PreparationSettings {
            is_emoji_displayed_as_text: native.emoji_as_text,
            add_text_presentation_selectors: native.composer_smart_symbols
                && !native.disable_smart_symbols,
            skip_markdown_code: false,
            use_channel_tagging: true,
        },
        headers,
    ))
}

pub(super) enum Attempt {
    Ready(PreparedPlain),
    NeedStandard,
    NeedCustom,
    NeedMention(MentionNeed),
}

pub(super) fn attempt_with_mentions(
    ops: &[Value],
    settings: PreparationSettings,
    last_viewed_locale: &str,
    user_locale: Option<&str>,
    standard: Option<&StandardEmoji>,
    custom: Option<&custom::Complete>,
    mentions: &MentionObservations,
) -> Result<Attempt, Error> {
    if last_viewed_locale.is_empty() {
        return Err(missing("last_viewed_locale"));
    }
    let emoji = super::emoji::observations(last_viewed_locale, user_locale, standard, custom);
    // Empty observation containers mean unacquired, not negative results. Core
    // refuses only a reached missing workflow/catalog fact. No fake HSTN map,
    // custom-name universe, source highlight list, or search result is supplied.
    let plan =
        match PlainPreparation::capture(ops, settings, &SyntaxObservations::default(), &emoji) {
            Ok(plan) => plan,
            Err(SeedUnavailable::MissingEmoji(_)) if standard.is_none() => {
                return Ok(Attempt::NeedStandard);
            }
            Err(SeedUnavailable::MissingEmoji(_)) if custom.is_none() => {
                return Ok(Attempt::NeedCustom);
            }
            Err(_) => return Err(missing("preparation")),
        };
    match plan.advance(mentions).map_err(|_| missing("mentions"))? {
        PreparationStep::Complete(prepared) => Ok(Attempt::Ready(prepared)),
        PreparationStep::Need(need) => Ok(Attempt::NeedMention(need)),
    }
}

#[cfg(test)]
pub(super) fn attempt(
    ops: &[Value],
    settings: PreparationSettings,
    last_viewed_locale: &str,
    user_locale: Option<&str>,
    standard: Option<&StandardEmoji>,
    custom: Option<&custom::Complete>,
) -> Result<Attempt, Error> {
    attempt_with_mentions(
        ops,
        settings,
        last_viewed_locale,
        user_locale,
        standard,
        custom,
        &MentionObservations::default(),
    )
}

#[cfg(test)]
pub(super) fn prepare(
    ops: &[Value],
    settings: PreparationSettings,
    locale: &str,
) -> Result<PreparedPlain, Error> {
    match attempt(ops, settings, locale, None, None, None)? {
        Attempt::Ready(prepared) => Ok(prepared),
        Attempt::NeedStandard | Attempt::NeedCustom => Err(missing("catalog")),
        Attempt::NeedMention(_) => Err(missing("automatic_mention_observation")),
    }
}
