//! Closed ordinary plain-mode source facts. No capability to contact Slack.
use super::*;
pub(crate) mod emoji_display;
pub(crate) mod emoticon;
pub(crate) mod go_links;
pub(crate) mod smart_symbols;

pub(crate) const FUTURE: &str = include_str!("plain/future.json");
pub(crate) fn noop() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_NOOP_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("own-edit-plain-noop" | "own-edit-plain-noop-storage")
        )
}
pub(crate) fn named() -> bool {
    if super::plain_members::named()
        || super::plain_channels::named()
        || emoji_display::named()
        || go_links::named()
        || smart_symbols::named()
        || emoticon::named()
        || noop()
        || completion::plain_custom_emoji::named()
    {
        return true;
    }
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("own-edit-plain" | "own-edit-plain-storage" | "own-edit-plain-emoji")
        )
}
pub(crate) fn emoji() -> bool {
    named()
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-plain-emoji")
}
pub(crate) fn future() -> &'static str {
    if super::plain_mixed::named() {
        return super::plain_mixed::future();
    }
    if super::plain_groups::named() {
        return super::plain_groups::FUTURE;
    }
    if super::plain_members::named() {
        return super::plain_members::FUTURE;
    }
    if super::plain_channels::named() {
        return super::plain_channels::FUTURE;
    }
    if emoji_display::named() {
        emoji_display::FUTURE
    } else if go_links::named() {
        go_links::FUTURE
    } else if smart_symbols::named() {
        smart_symbols::FUTURE
    } else if emoticon::named() {
        emoticon::FUTURE
    } else if completion::plain_custom_emoji::named() {
        completion::plain_custom_emoji::FUTURE
    } else if emoji() {
        include_str!("plain/emoji-future.json")
    } else {
        FUTURE
    }
}
pub(crate) fn text() -> &'static str {
    if completion::plain_custom_emoji::named() {
        completion::plain_custom_emoji::TEXT
    } else if emoji() {
        "Named :smile: Unicode \u{1f604}"
    } else {
        "Changed plain comment"
    }
}
pub(crate) fn storage() -> bool {
    named()
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("own-edit-plain-storage" | "own-edit-plain-noop-storage")
        )
}
pub(crate) fn preferences() -> Value {
    let mut value = json!({"ok":true,"prefs":{"plain_text_mode":true,"emoji_mode":"as_text",
        "locale":"en-US","all_notifications_prefs":{"channels":{},"global":{}}}});
    if emoticon::named() {
        value["prefs"]["convert_emoticons"] = json!(emoticon::enabled());
    }
    super::plain_mixed::apply_preferences(&mut value);
    smart_symbols::apply_preferences(&mut value);
    if emoji_display::named() {
        value["prefs"]["emoji_mode"] = json!("native");
    }
    value
}
pub(super) fn seed_boot(boot: &mut Value) {
    assert!(named());
    boot["prefs"] = preferences()["prefs"].clone();
    boot["subteams"] = json!({"self":[]});
    super::plain_channels::seed_boot(boot);
    super::plain_members::seed_boot(boot);
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    if !named() || !own_edit::enabled(shell) {
        return json!({"enabled":false});
    }
    let context = crate::directory_member_fetches::Context::capture(shell, "T1");
    let observed = context.ok().and_then(|c| {
        let w = shell.core.workspaces.get("T1")?;
        let groups = w.directory_usergroups.snapshot(&c.account,&c.session,c.epoch,&w.usergroups).ok()?;
        Some(json!({"locale":w.locale,"membership":groups.membership(),"rows":groups.rows(),"current":c.current(shell)}))
    });
    let mut value = json!({"enabled":true,"storage":storage(),"preferences":preferences(),"canonical":observed,
        "completion":completion::snapshot(shell),"noLiveNetwork":true});
    if super::plain_members::named() {
        value["members"] = super::plain_members::snapshot();
        if super::plain_mixed::named() {
            value["mixed"] = super::plain_mixed::snapshot();
        }
        value["memberSource"] = super::plain_members::source(shell);
        if super::plain_groups::named() {
            value["groupSource"] = super::plain_groups::source(shell);
        }
        if super::plain_member_agents::named() {
            value["agentCatalog"] = super::plain_member_agents::snapshot();
        }
    }
    if super::plain_channels::named() {
        value["channels"] = super::plain_channels::snapshot();
        value["channelSource"] = super::plain_channels::source(shell);
    }
    if emoji() || completion::plain_custom_emoji::named() {
        value["customCatalog"] = completion::plain_custom_emoji::snapshot(shell);
    }
    value
}
