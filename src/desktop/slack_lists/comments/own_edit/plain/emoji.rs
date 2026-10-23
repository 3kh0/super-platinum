//! Opening-owned complete emoji observations, separate from picker and Save.
use super::*;
use crate::directory_member_fetches::Context;
use crate::media::MediaUri;
use delta_editor::emoji_observations::{Appearance, EmojiRow, LiveEmoji};
use super_platinum_core::slack::api::emoji_catalog::{Complete, Read};
mod catalog;

pub(super) struct Observed {
    context: Context,
    preferences_revision: u64,
    custom_revision: u64,
    locale: String,
    // Retain actual complete source bytes; never replace them by a projection.
    _complete: Arc<Complete>,
    pub(super) live: Arc<LiveEmoji>,
}
impl Observed {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        current(
            shell,
            &self.context,
            self.preferences_revision,
            self.custom_revision,
            &self.locale,
        )
    }
}
fn current(shell: &ShellState, context: &Context, prefs: u64, custom: u64, locale: &str) -> bool {
    context.current(shell)
        && shell.core.active_team.as_deref() == Some(context.session.team_id.as_str())
        && crate::app_home::autocomplete::preferences_revision(shell) == prefs
        && shell
            .core
            .workspaces
            .get(&context.session.team_id)
            .is_some_and(|w| {
                w.custom_emoji_revision == custom && w.locale.as_deref() == Some(locale)
            })
}

pub(super) async fn acquire(
    state: Signal<ShellState>,
    source: &Source,
    serial: u64,
    policy: &own_edit_policy::Observation,
) -> Result<Observed, Error> {
    let (context, prefs, custom, locale, version) = {
        let shell = state.peek();
        if !source.current(&shell)
            || !source.owner.reading(&shell, serial)
            || !policy.current(&shell)
        {
            return Err(unavailable());
        }
        let proof = policy.proof();
        // BUnC en-US positively installs no name translation map. A missing
        // locale or an unsupported initialized language is not identity.
        if proof["userPreferences"]["prefs"]["locale"] != "en-US" {
            return Err(unavailable());
        }
        let context = Context::capture(&shell, &source.owner.scope.team)?;
        let workspace = shell
            .core
            .workspaces
            .get(&context.session.team_id)
            .ok_or_else(unavailable)?;
        (
            context,
            crate::app_home::autocomplete::preferences_revision(&shell),
            workspace.custom_emoji_revision,
            workspace
                .locale
                .clone()
                .filter(|v| !v.is_empty())
                .ok_or_else(unavailable)?,
            plain_observations::emoji_version(&proof["assignments"])?,
        )
    };
    let valid = || {
        let shell = state.peek();
        source.current(&shell)
            && source.owner.reading(&shell, serial)
            && policy.current(&shell)
            && current(&shell, &context, prefs, custom, &locale)
    };
    if !valid() {
        return Err(unavailable());
    }
    let request = Read::new(&context.session).request(&context.client, &context.session)?;
    let response = if crate::fixture::is_fixture() {
        crate::slack_lists::comments::fixture::completion::plain_custom_emoji::read(state, &request)
            .await
            .unwrap_or_else(|| {
                Err(Error::Offline(
                    "Closed emoji display catalog fixture".into(),
                ))
            })
    } else {
        context.transport.execute_current(request, valid).await
    };
    if !valid() {
        return Err(unavailable());
    }
    let raw = response?;
    let (complete, prepared) = tokio::task::spawn_blocking(move || {
        let complete = Arc::new(Complete::decode(raw)?);
        let prepared = catalog::Prepared::build(version, &complete)?;
        Ok::<_, Error>((complete, prepared))
    })
    .await
    .map_err(|_| unavailable())??;
    if !valid() {
        return Err(unavailable());
    }
    let live = {
        let shell = state.peek();
        prepared.project("en-US".into(), locale.clone(), |name, url| {
            let slot = format!(
                "list-comment/{}/{}/{}",
                context.account, context.session.team_id, name
            );
            shell
                .media
                .register_emoji_on_display(&slot, url)
                .webview_uri_at(shell.media_epoch)
        })
    };
    Ok(Observed {
        context,
        preferences_revision: prefs,
        custom_revision: custom,
        locale,
        _complete: complete,
        live: Arc::new(live),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use super_platinum_core::composer_emoji::standard::Version;

    #[test]
    fn display_facts_retire_on_original_catalog_preferences_locale_socket_and_account() {
        let (mut shell, _, _) = crate::slack_lists::comments::own_edit::tests::setup();
        shell.core.workspaces.get_mut("T1").unwrap().locale = Some("en-US".into());
        let context = Context::capture(&shell, "T1").unwrap();
        let prefs = crate::app_home::autocomplete::preferences_revision(&shell);
        let custom = shell.core.workspaces["T1"].custom_emoji_revision;
        assert!(current(&shell, &context, prefs, custom, "en-US"));
        assert!(!current(
            &shell,
            &context,
            prefs.wrapping_add(1),
            custom,
            "en-US"
        ));
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .custom_emoji_revision += 1;
        assert!(!current(&shell, &context, prefs, custom, "en-US"));
        let custom = shell.core.workspaces["T1"].custom_emoji_revision;
        shell.core.workspaces.get_mut("T1").unwrap().locale = Some("ja-JP".into());
        assert!(!current(&shell, &context, prefs, custom, "en-US"));
        assert!(current(&shell, &context, prefs, custom, "ja-JP"));
        shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
        assert!(!current(&shell, &context, prefs, custom, "ja-JP"));
        let context = Context::capture(&shell, "T1").unwrap();
        assert!(current(&shell, &context, prefs, custom, "ja-JP"));
        shell.core.active_account = Some("successor".into());
        assert!(!current(&shell, &context, prefs, custom, "ja-JP"));
    }

    #[test]
    fn actual_restored_delta_adds_reached_appearance_without_rewriting_content() {
        let (mut shell, _, _) = crate::slack_lists::comments::own_edit::tests::setup();
        shell.core.workspaces.get_mut("T1").unwrap().locale = Some("en-US".into());
        let complete = Arc::new(Complete::decode(json!({"ok":true,"emoji":{}})).unwrap());
        let facts = Observed {
            context: Context::capture(&shell, "T1").unwrap(),
            preferences_revision: crate::app_home::autocomplete::preferences_revision(&shell),
            custom_revision: shell.core.workspaces["T1"].custom_emoji_revision,
            locale: "en-US".into(),
            live: Arc::new(
                catalog::Prepared::build(Version::V16, &complete)
                    .unwrap()
                    .project("en-US".into(), "en-US".into(), |_, _| unreachable!()),
            ),
            _complete: complete,
        };
        let live = facts.live.clone();
        let mut mount = Mount {
            preferences_revision: facts.preferences_revision,
            emoticons: None,
            policy: native_policy(&json!({"userPreferences":{"ok":true,"prefs":{
                "plain_text_mode":true,"emoji_mode":"native"}},
                "assignments":{"ok":true,"assignments":{}}}))
            .unwrap(),
            emoji: Some(Arc::new(facts)),
        };
        mount.policy.live_emoji = Some(live.clone());
        let cloned = mount.policy.clone();
        assert!(Arc::ptr_eq(cloned.live_emoji.as_ref().unwrap(), &live));
        assert_eq!(
            serde_json::to_value(&cloned).unwrap()["liveEmoji"],
            serde_json::to_value(live.as_ref()).unwrap()
        );
        let raw = vec![
            json!({"insert":{"slackemoji":{"text":":thumbsup::skin-tone-4:"}}}),
            json!({"insert":" restored\n"}),
        ];
        let delta = PlainDelta::capture(raw.clone()).unwrap();
        mount.observe_content(&delta).unwrap();
        assert_eq!(mount.policy.emoji.len(), 1);
        assert_eq!(mount.policy.emoji[0].name, ":thumbsup::skin-tone-4:");
        assert_eq!(delta.ops(), raw.as_slice());
        mount.observe_content(&delta).unwrap();
        assert_eq!(mount.policy.emoji.len(), 1);
        let unknown = PlainDelta::capture(vec![
            json!({"insert":{"slackemoji":{"text":":unavailable:"}}}),
            json!({"insert":"\n"}),
        ])
        .unwrap();
        assert!(mount.observe_content(&unknown).is_err());
    }
}
