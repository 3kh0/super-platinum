//! Bounded app overflow menu. Native focus stays in the menu until dismissal.
use crate::{
    icons::{Icon, icon},
    message_buttons::{self, overflow, source::Source},
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::block_actions::BlockElementMetadata;

#[component]
pub(crate) fn MessageOverflow(
    state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    accessibility_label: String,
) -> Element {
    let id = use_hook(overflow::id);
    let mut menu = use_signal(|| None::<overflow::Ticket>);
    let mut stateless_source = use_signal(|| None::<Source>);
    use_drop(move || {
        if let Some(ticket) = menu.peek().as_ref() {
            ticket.cancel();
        }
    });
    let hydration_revision = state.read().message_buttons.hydration_revision;
    use_effect(use_reactive(
        (&source, &hydration_revision),
        move |(source, _)| {
            message_buttons::inputs::ensure(state, source.clone());
            stateless_source
                .set(overflow::stateless(&state.peek(), &source).then_some(source.clone()));
            message_buttons::hydration::ensure(state, source);
        },
    ));
    let view = overflow::presentation(
        &state.read(),
        &source,
        &metadata,
        stateless_source.read().as_ref() == Some(&source),
    );
    let active = menu
        .read()
        .clone()
        .filter(|ticket| ticket.current(&state.read()));
    let expanded = active.is_some();
    let label = if accessibility_label.trim().is_empty() {
        "More actions"
    } else {
        &accessibility_label
    };
    let title = view.reason.as_deref().unwrap_or(label);
    let hover_source = source.clone();
    let keyboard_source = source.clone();
    let keyboard_metadata = metadata.clone();
    rsx! {
        button {
            id: "message-overflow-trigger-{id}", class:"block-button block-overflow",
            disabled:!view.enabled, title:"{title}",
            "aria-label":"{label}", "aria-haspopup":"menu", "aria-expanded":expanded.to_string(),
            "aria-controls":"message-overflow-menu-{id}", "aria-busy":view.busy.to_string(),
            "data-block-id":metadata.block_id.clone().unwrap_or_default(),
            "data-action-id":metadata.action_id.clone().unwrap_or_default(),
            onmouseenter:move |_|message_buttons::hydration::retry(state,hover_source.clone()),
            ondoubleclick:move |event|event.stop_propagation(),
            onkeydown:move |event| {
                if matches!(event.key(), Key::ArrowDown | Key::ArrowUp) {
                    event.prevent_default(); event.stop_propagation();
                    if let Some(ticket) = menu.peek().as_ref() { ticket.cancel(); }
                    menu.set(overflow::open(state,keyboard_source.clone(),keyboard_metadata.clone()));
                }
            },
            onclick:move |event| {
                event.stop_propagation();
                if let Some(ticket) = menu.peek().as_ref() { ticket.cancel(); }
                menu.set(if expanded {None} else {overflow::open(state,source.clone(),metadata.clone())});
            },
            {icon(Icon::More, "icon")}
        }
        if let Some(ticket) = active {
            div { class:"message-overflow-backdrop", "aria-hidden":"true",
                onclick:move |event| { event.stop_propagation(); dismiss(menu,id); },
            }
            div {
                id:"message-overflow-menu-{id}", class:"message-overflow-menu", role:"menu", "aria-label":"{label}",
                "data-local-keyboard":"true",
                onmounted:move |_| { document::eval(&format!(r#"requestAnimationFrame(()=>{{const m=document.getElementById('message-overflow-menu-{id}'),b=document.getElementById('message-overflow-trigger-{id}');if(!m||!b)return;const r=b.getBoundingClientRect();m.style.left=Math.max(8,Math.min(r.left,innerWidth-m.offsetWidth-8))+'px';m.style.top=Math.max(8,Math.min(r.bottom+4,innerHeight-m.offsetHeight-8))+'px';m.querySelector('button')?.focus({{preventScroll:true}});}});"#)); },
                ondoubleclick:move |event|event.stop_propagation(),
                onkeydown:move |event| {
                    event.stop_propagation();
                    match event.key() {
                        Key::Escape | Key::Tab => { event.prevent_default(); dismiss(menu,id); }
                        Key::ArrowDown | Key::ArrowUp | Key::Home | Key::End => {
                            event.prevent_default();
                            let direction = match event.key() { Key::ArrowUp => -1, Key::Home => -2, Key::End => 2, _ => 1 };
                            document::eval(&format!(r#"(()=>{{const m=document.getElementById('message-overflow-menu-{id}');if(!m)return;const b=[...m.querySelectorAll('button')],i=b.indexOf(document.activeElement),d={direction};b[d===-2?0:d===2?b.length-1:(i+d+b.length)%b.length]?.focus({{preventScroll:true}});}})();"#));
                        }
                        _=>{}
                    }
                },
                for option in ticket.options.clone() {
                    { let choice=ticket.clone(); let value=option.value.clone(); rsx! {
                        button { key:"{value}", class:"message-overflow-option", role:"menuitem", tabindex:"-1", "data-option-value":"{value}",
                            onclick:move |event| { event.stop_propagation(); menu.set(None); overflow::choose(state,choice.clone(),value.clone()); restore_focus(id); },
                            "{option.text}"
                        }
                    }}
                }
            }
        }
    }
}

fn dismiss(mut menu: Signal<Option<overflow::Ticket>>, id: u64) {
    if let Some(ticket) = menu.peek().as_ref() {
        ticket.cancel();
    }
    menu.set(None);
    restore_focus(id);
}
fn restore_focus(id: u64) {
    document::eval(&format!(
        "document.getElementById('message-overflow-trigger-{id}')?.focus({{preventScroll:true}});"
    ));
}
