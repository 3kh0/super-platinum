//! Shared reaction visuals with caller-owned toggle authority.
use super::*;

/// The reaction pills under a message. Clicking one toggles it.
pub(crate) fn reactions_row(
    state: Signal<ShellState>,
    media: &crate::media::MediaRegistry,
    media_epoch: u64,
    channel: &str,
    message: &MessageVm,
) -> Element {
    rsx! {
        for reaction in &message.reactions {
            {reaction_pill(state, media, media_epoch, channel, &message.id, reaction)}
        }
    }
}

/// Display reactions for a reader that has no authority to change the thread.
pub(crate) fn reactions_read_only(
    state: Signal<ShellState>,
    media: &crate::media::MediaRegistry,
    media_epoch: u64,
    message: &MessageVm,
) -> Element {
    rsx! {
        for reaction in &message.reactions {
            span {
                key: "reaction-{reaction.name}",
                "data-reaction-name": "{reaction.name}",
                class: if reaction.own { "reaction own" } else { "reaction" },
                title: reaction.reactors_label(),
                "aria-label": reaction.reactors_label(),
                {reaction_emoji(state, media, media_epoch, reaction, "reaction-emoji", "reaction-glyph")}
                span { class: "reaction-count", "{reaction.count}" }
            }
        }
    }
}

fn reaction_pill(
    state: Signal<ShellState>,
    media: &crate::media::MediaRegistry,
    media_epoch: u64,
    channel: &str,
    ts: &str,
    reaction: &ReactionVm,
) -> Element {
    let channel = channel.to_owned();
    let ts = ts.to_owned();
    let name = reaction.name.clone();
    let own = reaction.own;
    reaction_button(state, media, media_epoch, reaction, false, move || {
        spawn(crate::bootstrap::toggle_reaction(
            state,
            channel.clone(),
            ts.clone(),
            name.clone(),
            own,
        ));
    })
}

/// Reuses the ordinary pill appearance with only the List-owned toggle route.
pub(crate) fn reactions_list_comments(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    generation: u64,
    message: &MessageVm,
) -> Element {
    let ts = message.ts.clone();
    let can_open = crate::slack_lists::comments::reaction_actions::selection::available(
        snapshot, generation, &ts,
    );
    rsx! {
        button {r#type:"button",class:"reaction",title:"Add a reaction","aria-label":"Add a reaction",
            "data-list-reaction-add":"{ts}","aria-haspopup":"dialog",disabled:!can_open,
            onclick:move |_|crate::slack_lists::comments::reaction_actions::selection::open(state,generation,ts.clone()),
            {crate::icons::icon(crate::icons::Icon::AddReaction,"icon sm")}
        }

        for reaction in &message.reactions {
            {
                let ts = message.ts.clone();
                let name = reaction.name.clone();
                let disabled = !crate::slack_lists::comments::reaction_actions::available(snapshot, generation, &ts, &name);
                reaction_button(state, &snapshot.media, snapshot.media_epoch, reaction, disabled, move || {
                    crate::slack_lists::comments::reaction_actions::toggle(state, generation, ts.clone(), name.clone());
                })
            }
        }
    }
}

fn reaction_button(
    state: Signal<ShellState>,
    media: &crate::media::MediaRegistry,
    media_epoch: u64,
    reaction: &ReactionVm,
    disabled: bool,
    mut toggle: impl FnMut() + 'static,
) -> Element {
    let own = reaction.own;
    let label = reaction.reactors_label();
    let who = reaction.reactors_who();
    let faces = reaction
        .reactors
        .iter()
        .take(crate::model::REACTOR_AVATARS_SHOWN);
    // The card shows art only: a custom emoji (or its loading box) or a
    // standard glyph. An unresolved `:shortcode:` is already in the line below.
    let card_art = reaction.media.is_some() || reaction.resolved_glyph().is_some();
    rsx! {
        button {
            key: "reaction-{reaction.name}",
            r#type: "button",
            "data-reaction-name": "{reaction.name}",
            class: if own { "reaction own" } else { "reaction" },
            "aria-label": "{label}",
            "aria-pressed": own.to_string(),
            "aria-disabled": disabled.to_string(),
            disabled,
            onclick: move |_| { if !disabled { toggle(); } },
            {reaction_emoji(state, media, media_epoch, reaction, "reaction-emoji", "reaction-glyph")}
            span { class: "reaction-count", "{reaction.count}" }
            if !reaction.reactors.is_empty() {
                span { class: "reaction-faces",
                    for reactor in faces {
                        span { key: "reactor-{reactor.user_id}", class: "reaction-face",
                            if let Some(avatar) = reactor.avatar.as_ref().filter(|avatar| media.is_ready(avatar)) {
                                MediaImage {state,id:avatar.clone(),media_epoch,compact:true,fallback:rsx! {"{reactor.initials}"}}
                            } else {
                                "{reactor.initials}"
                            }
                        }
                    }
                }
            }
            // CSS-only hover card: no state write, so hovering a pill never
            // re-renders the transcript.
            span { class: "reaction-card", "aria-hidden": "true",
                if card_art {
                    span { class: "reaction-card-emoji",
                        {reaction_emoji(state, media, media_epoch, reaction, "reaction-card-image", "reaction-card-glyph")}
                    }
                }
                span { class: "reaction-card-label",
                    strong { "{who}" }
                    " reacted with {reaction.fallback()}"
                }
            }
        }
    }
}

/// A reaction's emoji at either size: custom art, a held gap while it loads,
/// or the standard glyph.
fn reaction_emoji(
    state: Signal<ShellState>,
    media: &crate::media::MediaRegistry,
    media_epoch: u64,
    reaction: &ReactionVm,
    image_class: &str,
    glyph_class: &str,
) -> Element {
    rsx! {
        if let Some(emoji) = reaction.media.as_ref().filter(|emoji| media.is_ready(emoji)) {
            MediaImage {
                state, id:emoji.clone(), media_epoch, compact:true,
                class:image_class.to_owned(), alt:format!(":{}:",reaction.name),
            }
        } else if reaction.media.is_some() {
            // A custom emoji still downloading. Hold the pill's shape rather
            // than leaving a gap where the art will be.
            span { class: "{image_class} pending" }
        } else {
            span { class: "{glyph_class}", "{reaction.glyph.clone().unwrap_or_else(|| reaction.fallback())}" }
        }
    }
}
