//! Searchable, bounded emoji picker shared by channel messages and replies.

use dioxus::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use super_platinum_core::slack::{Transport, models::Emoji};
use super_platinum_core::state::{EmojiSource, custom_emoji_source};

use crate::state::{Overlay, ReactionTarget, ShellState};

#[derive(Clone)]
struct Choice {
    name: String,
    glyph: String,
    url: Option<String>,
    aliases: String,
}

pub struct CatalogCache {
    transport: std::sync::Weak<Transport>,
    choices: Arc<Vec<Choice>>,
    loaded: std::time::Instant,
}

fn cached(
    shell: &ShellState,
    team: &str,
    transport: &Arc<Transport>,
    require_fresh: bool,
) -> Option<Arc<Vec<Choice>>> {
    let cache = shell.emoji_catalog_loaded.get(team)?;
    ((!require_fresh || cache.loaded.elapsed() < std::time::Duration::from_secs(300))
        && cache
            .transport
            .upgrade()
            .is_some_and(|loaded| Arc::ptr_eq(&loaded, transport)))
    .then(|| cache.choices.clone())
}

const COMMON: &[&str] = &[
    "+1",
    "heart",
    "eyes",
    "joy",
    "tada",
    "fire",
    "rocket",
    "white_check_mark",
];

pub fn open(mut state: Signal<ShellState>, channel: String, ts: String) {
    let mut shell = state.write();
    let Some(team) = shell.core.active_team.clone() else {
        return;
    };
    shell.reaction_target = Some(ReactionTarget { team, channel, ts });
    shell.overlay = Some(Overlay::ReactionPicker);
}

fn choices(shell: &ShellState) -> Vec<Choice> {
    let workspace = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team));
    build_choices(
        workspace
            .map(|workspace| &workspace.custom_emoji)
            .unwrap_or(&HashMap::new()),
    )
}

fn build_choices(custom: &HashMap<String, Emoji>) -> Vec<Choice> {
    let mut result: Vec<_> = super_platinum_core::state::standard_emoji_catalog()
        .iter()
        .filter(|emoji| !custom.contains_key(emoji.name))
        .map(|emoji| Choice {
            name: emoji.name.into(),
            glyph: emoji.glyph.into(),
            url: None,
            aliases: emoji.aliases.clone(),
        })
        .collect();
    result.extend(custom.keys().filter_map(|name| {
        let (glyph, url) = match custom_emoji_source(custom, name)? {
            EmojiSource::Url(url) => (String::new(), Some(url.to_owned())),
            EmojiSource::Unicode(glyph) => (glyph.to_owned(), None),
        };
        Some(Choice {
            name: name.clone(),
            glyph,
            url,
            aliases: name.to_lowercase(),
        })
    }));
    result.sort_by(|a, b| {
        let rank = |name: &str| {
            COMMON
                .iter()
                .position(|common| *common == name)
                .unwrap_or(COMMON.len())
        };
        rank(&a.name)
            .cmp(&rank(&b.name))
            .then_with(|| a.name.cmp(&b.name))
    });
    result
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
        let removing = shell
            .messages
            .iter()
            .chain(shell.thread_messages.iter())
            .find(|message| message.id == target.ts)
            .is_some_and(|message| {
                message
                    .reactions
                    .iter()
                    .any(|reaction| reaction.name == name && reaction.own)
            });
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

#[component]
pub fn ReactionPicker(mut state: Signal<ShellState>) -> Element {
    // Rebuild only on opening/catalog arrival, never on ordinary media paints.
    let mut catalog = use_signal(|| {
        let shell = state.peek();
        if let (Some(team), Some(transport)) = (&shell.core.active_team, &shell.core.transport) {
            if let Some(catalog) = cached(&shell, team, transport, false) {
                return catalog;
            }
        }
        Arc::new(choices(&shell))
    });
    let mut catalog_status = use_signal(String::new);
    use_future(move || async move {
        let Some((transport, client, sessions)) = crate::bootstrap::credentials(&state) else {
            return;
        };
        let Some(team) = state.peek().core.active_team.clone() else {
            return;
        };
        let Some(session) = sessions.iter().find(|session| session.team_id == team) else {
            return;
        };
        if let Some(loaded) = cached(&state.peek(), &team, &transport, true) {
            catalog.set(loaded);
            return;
        }
        catalog_status.set("Loading workspace emoji…".into());
        let result =
            super_platinum_core::slack::api::fetch_emoji_catalog(&transport, &client, session)
                .await;
        if state
            .peek()
            .core
            .transport
            .as_ref()
            .is_none_or(|current| !Arc::ptr_eq(current, &transport))
        {
            return;
        }
        match result {
            Ok(emojis) => {
                let built = tokio::task::spawn_blocking(move || {
                    let map: HashMap<_, _> = emojis
                        .into_iter()
                        .map(|emoji| (emoji.name.clone(), emoji))
                        .collect();
                    let choices = Arc::new(build_choices(&map));
                    (map, choices)
                })
                .await;
                let Ok((emojis, loaded_choices)) = built else {
                    catalog_status.set("Workspace emoji unavailable. Reopen to retry.".into());
                    return;
                };
                let mut shell = state.write();
                if shell
                    .core
                    .transport
                    .as_ref()
                    .is_none_or(|current| !Arc::ptr_eq(current, &transport))
                {
                    return;
                }
                if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                    workspace.custom_emoji = emojis;
                }
                shell.emoji_catalog_loaded.insert(
                    team.clone(),
                    CatalogCache {
                        transport: Arc::downgrade(&transport),
                        choices: loaded_choices.clone(),
                        loaded: std::time::Instant::now(),
                    },
                );
                if shell.core.active_team.as_ref() == Some(&team) {
                    catalog.set(loaded_choices);
                    shell.refresh_from_core();
                }
                catalog_status.set(String::new());
            }
            Err(_error) => {
                catalog_status.set("Workspace emoji unavailable. Reopen to retry.".into());
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
    rsx! {
        div { class: "scrim", onclick: move |_| state.write().overlay = None }
        section { class: "modal emoji-picker", role: "dialog", "aria-modal": "true", "aria-labelledby": "emoji-picker-title",
            header {
                h2 { id: "emoji-picker-title", "Add a reaction" }
                button { "aria-label": "Close", onclick: move |_| state.write().overlay = None,
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
                            choose(state, keyboard_catalog[keyboard_matches[active]].name.clone());
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
                        let image = choice.url.as_ref().map(|url| snapshot.media.register_emoji(&choice.name, url));
                        rsx! {
                            button {
                                key: "{choice.name}",
                                id: "emoji-option-{index}",
                                role: "option",
                                "aria-selected": if index == active { "true" } else { "false" },
                                "aria-label": "{choice.name}",
                                title: ":{choice.name}:",
                                onclick: move |_| choose(state, name.clone()),
                                if let Some(image) = image {
                                    img { src: "{image.uri_at(snapshot.media_epoch)}", alt: "" }
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
                    if catalog_status.read().is_empty() { "{visible} of {matches.len()} emoji" }
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
