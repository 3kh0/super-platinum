//! Searchable, bounded emoji picker shared by channel messages and replies.

use crate::media::MediaUri;
use dioxus::prelude::*;
use std::sync::Arc;

use crate::state::{Overlay, ReactionTarget, ShellState};

type Choice = crate::emoji_catalog::PickerChoice;

pub fn open(mut state: Signal<ShellState>, channel: String, ts: String) {
    let mut shell = state.write();
    let Some(team) = shell.core.active_team.clone() else {
        return;
    };
    shell.reaction_target = Some(ReactionTarget { team, channel, ts });
    shell.overlay = Some(Overlay::ReactionPicker);
}

fn matching(choices: &[Choice], query: &str) -> Vec<usize> {
    let query = query.trim().trim_matches(':').to_lowercase();
    let mut matches: Vec<_> = choices
        .iter()
        .enumerate()
        .filter(|(_, choice)| {
            query.is_empty() || choice.aliases.contains(&query) || choice.glyph == query
        })
        .map(|(index, _)| index)
        .collect();
    if !query.is_empty() {
        matches.sort_by_key(|index| {
            let choice = &choices[*index];
            if choice.name == query || choice.glyph == query {
                0
            } else if choice.name.starts_with(&query) {
                1
            } else {
                2
            }
        });
    }
    matches
}

fn choose(mut state: Signal<ShellState>, name: String) {
    let (target, removing) = {
        let mut shell = state.write();
        let Some(target) = shell.reaction_target.take() else {
            return;
        };
        shell.overlay = None;
        if shell.core.active_team.as_ref() != Some(&target.team) {
            return;
        }
        let removing = owns_reaction(&shell, &target, &name);
        (target, removing)
    };
    // The dialog unmounts immediately. A scope-owned task would be cancelled
    // along with it, potentially before the request ever reaches Slack.
    dioxus::core::spawn_forever(async move {
        if state.peek().core.active_team.as_ref() == Some(&target.team) {
            crate::bootstrap::toggle_reaction(state, target.channel, target.ts, name, removing)
                .await;
        }
    });
}

/// Opening the picker does not pin its message in the rendered window. Read
/// the scoped source so scrolling cannot turn a remove into an add.
fn owns_reaction(shell: &ShellState, target: &ReactionTarget, name: &str) -> bool {
    let Some(workspace) = shell.core.workspaces.get(&target.team) else {
        return false;
    };
    workspace
        .messages
        .get(&target.channel)
        .and_then(|bag| {
            bag.messages
                .iter()
                .find(|message| message.ts.as_deref() == Some(&target.ts))
        })
        .or_else(|| {
            shell
                .core
                .threads
                .iter()
                .filter(|((team, channel, _), _)| {
                    team == &target.team && channel == &target.channel
                })
                .find_map(|(_, bag)| {
                    bag.messages
                        .iter()
                        .find(|message| message.ts.as_deref() == Some(&target.ts))
                })
        })
        .is_some_and(|message| {
            message.reactions.iter().any(|reaction| {
                reaction.name == name
                    && super_platinum_core::state::reaction_has_user(
                        reaction,
                        &workspace.self_user_id,
                    )
            })
        })
}

#[component]
pub fn ReactionPicker(mut state: Signal<ShellState>) -> Element {
    rsx! {EmojiPicker {
        state,title:"Add a reaction".to_owned(),
        onchoose:move |selection:crate::emoji_catalog::Selection|choose(state,selection.name),
        onclose:move |_|state.write().overlay=None,
    }}
}

/// The same bounded catalogue and keyboard picker serves reactions and status.
/// Callbacks own the result; opening this component never creates a reaction.
#[component]
pub(crate) fn EmojiPicker(
    mut state: Signal<ShellState>,
    title: String,
    #[props(default)] composer_visit: Option<u64>,
    #[props(default)] require_preferences: bool,
    #[props(default = "shell-overlay-dialog".to_owned())] dialog_id: String,
    #[props(default = true)] choice_ready: bool,
    #[props(default)] choice_status: Option<String>,
    onchoose: EventHandler<crate::emoji_catalog::Selection>,
    onclose: EventHandler,
) -> Element {
    let _render_release = crate::performance::emoji_retirement::scope(12, 0, 0);
    let mut catalog = use_signal(|| {
        Arc::new(crate::performance::emoji_retirement::payload(
            Vec::<Choice>::new(),
            2,
        ))
    });
    let mut catalog_status = use_signal(|| "Loading emoji?".to_owned());
    let requested = use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(None::<String>)));
    use_future(move || async move {
        crate::emoji_preferences::ensure(state);
        crate::emoji_catalog::ensure(state);
    });
    use_effect(move || {
        let _effect_release = crate::performance::emoji_retirement::scope(11, 0, 0);
        let shell = state.read();
        let loaded = crate::emoji_catalog::current(&shell);
        let revision = serde_json::to_string(&(
            shell.core.active_account.clone(),
            shell.core.account_epoch,
            shell.core.active_team.clone(),
            shell
                .core
                .active_team
                .as_ref()
                .and_then(|t| shell.core.workspaces.get(t))
                .map(|w| (w.custom_emoji_revision, w.locale.clone())),
        ))
        .unwrap();
        drop(shell);
        if let Some(loaded) = loaded {
            if !Arc::ptr_eq(&catalog.peek(), &loaded.picker) {
                catalog.set(loaded.picker.clone());
            }
            if !catalog_status.peek().is_empty() {
                catalog_status.set(String::new());
            }
        } else {
            if requested.borrow().as_ref() != Some(&revision) {
                *requested.borrow_mut() = Some(revision);
                crate::emoji_catalog::ensure(state);
            }
            if !catalog.peek().is_empty() {
                catalog.set(Arc::new(crate::performance::emoji_retirement::payload(
                    vec![],
                    2,
                )));
            }
        }
    });
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
    let mut limit = use_signal(|| 120usize);
    let matched = use_memo(move || Arc::new(matching(&catalog.read(), &query.read())));
    let catalog = catalog.read();
    let matches = matched.read();
    let visible = matches.len().min(limit());
    let active = selected().min(visible.saturating_sub(1));
    let keyboard_catalog = catalog.clone();
    let keyboard_matches = matches.clone();
    let snapshot = state.read();
    // An effect publishes rows after the shell's new Loaded allocation arrives.
    // During that frame gap, never bind old visible rows to the new identity.
    let displayed = crate::emoji_catalog::current(&snapshot)
        .filter(|loaded| Arc::ptr_eq(&catalog, &loaded.picker));
    let choices_ready = choice_ready
        && displayed.is_some()
        && ((!require_preferences && composer_visit.is_none())
            || snapshot
                .core
                .active_team
                .as_ref()
                .and_then(|team| snapshot.core.workspaces.get(team))
                .is_some_and(|workspace| workspace.emoji_preferences.loaded()));
    let keyboard_displayed = displayed.clone();
    rsx! {
        div { class: "scrim", onclick: move |_| onclose.call(()) }
        section { id:dialog_id, class: "modal emoji-picker", role: "dialog", "aria-modal": "true", "aria-labelledby": "emoji-picker-title",
            tabindex:"-1","data-keyboard-dialog":"true","data-dialog-kind":"other",
            "data-emoji-choice-ready":if choices_ready {"true"} else {"false"},
            "data-emoji-lifecycle-catalog":if crate::performance::emoji_lifecycle::enabled() || crate::performance::emoji_retirement::enabled() { displayed.as_ref().map(|c|c.token.clone()) } else {None},
            onkeydown:move |event|if event.key()==Key::Escape {event.prevent_default();event.stop_propagation();onclose.call(());},
            header {
                h2 { id: "emoji-picker-title", "{title}" }
                button { "aria-label": "Close", onclick: move |_| onclose.call(()),
                    {crate::icons::icon(crate::icons::Icon::Close, "icon")}
                }
            }
            input {
                id: "emoji-query",
                placeholder: "Search emoji…",
                "aria-label": "Search emoji",
                "aria-controls": "emoji-options",
                "aria-activedescendant": if visible > 0 { format!("emoji-option-{active}") } else { String::new() },
                autofocus: true,
                onmounted: move |_| crate::view::composer::focus_field_when_mounted("emoji-query"),
                oninput: move |event| { query.set(event.value()); selected.set(0); limit.set(120); },
                onkeydown: move |event| {
                    let next = match event.key() {
                        Key::ArrowRight => Some((active + 1).min(visible.saturating_sub(1))),
                        Key::ArrowLeft => Some(active.saturating_sub(1)),
                        Key::ArrowDown => Some((active + 8).min(visible.saturating_sub(1))),
                        Key::ArrowUp => Some(active.saturating_sub(8)),
                        Key::Enter if visible > 0 => {
                            event.prevent_default();
                            if choices_ready && keyboard_displayed.as_ref().is_some_and(|shown|crate::emoji_catalog::current(&state.peek()).is_some_and(|current|Arc::ptr_eq(shown,&current))) {
                                onchoose.call(crate::emoji_catalog::Selection {name:keyboard_catalog[keyboard_matches[active]].name.clone(),query:query(),loaded:keyboard_displayed.as_ref().unwrap().clone()});
                            } else if let Some(serial) = composer_visit {
                                crate::composer_emoji::release_choice(serial);
                            }
                            None
                        }
                        _ => None,
                    };
                    if let Some(next) = next {
                        event.prevent_default();
                        selected.set(next);
                        dioxus::document::eval(&format!("requestAnimationFrame(() => document.getElementById('emoji-option-{next}')?.scrollIntoView({{ block: 'nearest' }}));"));
                    }
                },
            }
            div { class: "emoji-options", id: "emoji-options", role: "listbox", "aria-label": "Emoji",
                for (index, choice_index) in matches.iter().take(visible).enumerate() {
                    {
                        let choice = &catalog[*choice_index];
                        let name = choice.name.clone();
                        let displayed = displayed.clone();
                        let image = choice.url.as_ref().map(|url| snapshot.media.register_emoji(&choice.name, url));
                        rsx! {
                            button {
                                key: "{choice.name}",
                                id: "emoji-option-{index}",
                                role: "option",
                                "aria-selected": if index == active { "true" } else { "false" },
                                "aria-label": "{choice.name}",
                                title: ":{choice.name}:",
                                disabled:!choices_ready,
                                onclick: move |_| {if choices_ready && displayed.as_ref().is_some_and(|shown|crate::emoji_catalog::current(&state.peek()).is_some_and(|current|Arc::ptr_eq(shown,&current))) {onchoose.call(crate::emoji_catalog::Selection {name:name.clone(),query:query(),loaded:displayed.as_ref().unwrap().clone()});} else if let Some(serial) = composer_visit {crate::composer_emoji::release_choice(serial);}},
                                if let Some(image) = image {
                                    img { src: "{image.webview_uri_at(snapshot.media_epoch)}", alt: "" }
                                } else { "{choice.glyph}" }
                            }
                        }
                    }
                }
            }
            if matches.is_empty() {
                p { class: "emoji-empty", "No matching emoji. Try another name." }
            }
            footer { class: "emoji-footer",
                span { role: "status",
                    if let Some(status) = &choice_status { "{status}" }
                    else if catalog_status.read().is_empty() { "{visible} of {matches.len()} emoji" }
                    else { "{catalog_status}" }
                }
                if visible < matches.len() {
                    button { onclick: move |_| limit.set(limit() + 120), "Show more" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_ownership_survives_render_window_changes_and_stays_in_destination() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let target = ReactionTarget {
            team: "T1".into(),
            channel: "C2".into(),
            ts: "100.000001".into(),
        };
        let raw = serde_json::from_value(serde_json::json!({
            "ts":target.ts,"user":"U1","text":"Offscreen reaction target",
            "reactions":[{"name":"eyes","count":1,"users":["U0"]}]
        }))
        .unwrap();
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .messages
            .push(raw);
        shell.messages.clear();
        shell.thread_messages.clear();
        assert!(owns_reaction(&shell, &target, "eyes"));
        assert!(!owns_reaction(&shell, &target, "heart"));
        let other = ReactionTarget {
            channel: "C1".into(),
            ..target.clone()
        };
        assert!(!owns_reaction(&shell, &other, "eyes"));
        let raw = shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .messages
            .pop()
            .unwrap();
        shell
            .core
            .threads
            .entry(("T1".into(), "C2".into(), "100.0".into()))
            .or_default()
            .messages
            .push(raw);
        assert!(owns_reaction(&shell, &target, "eyes"));
        assert!(!owns_reaction(&shell, &other, "eyes"));
    }

    #[test]
    fn searches_aliases_glyphs_and_colon_wrapped_names() {
        let choices = vec![
            Choice {
                name: "+1".into(),
                glyph: "👍".into(),
                aliases: "+1 thumbsup".into(),
                url: None,
            },
            Choice {
                name: "thumbsup_party".into(),
                glyph: String::new(),
                aliases: "thumbsup_party".into(),
                url: Some("https://example.com/emoji.png".into()),
            },
        ];
        assert_eq!(matching(&choices, " :THUMBSUP: "), vec![1, 0]);
        assert_eq!(matching(&choices, "👍"), vec![0]);
        assert!(matching(&choices, "missing").is_empty());
        assert_eq!(matching(&choices, ""), vec![0, 1]);
    }
}
