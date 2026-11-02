//! Existing Root pills only. Glyphs/names are reading; a fresh purpose owns IO.
use super::*;
use crate::thread_feed::reaction_toggle as purpose;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use super_platinum_core::slack::models::ThreadViewItem;

pub(super) fn render(
    state: Signal<ShellState>,
    s: &ShellState,
    item: &ThreadViewItem,
    read: Option<&crate::thread_feed::Source>,
) -> Element {
    let Some(team) = s.core.active_team.as_ref() else {
        return replies::root_counts(s, item);
    };
    let Some(w) = s.core.workspaces.get(team) else {
        return replies::root_counts(s, item);
    };
    let checked = purpose::displayed(s, item);
    let reactions = checked.as_deref().unwrap_or(&item.root_msg.reactions);
    let viewer = s
        .core
        .session
        .as_ref()
        .and_then(|session| session.workspaces.get(team))
        .filter(|session| {
            session.team_id == *team
                && !session.user_id.is_empty()
                && session.user_id == w.self_user_id
        })
        .map(|session| session.user_id.as_str());
    let mut names = std::collections::HashMap::new();
    for r in reactions {
        *names.entry(r.name.as_str()).or_insert(0usize) += 1;
    }
    // Preserve the old reading boundary before grouping. This function never
    // registers a resource or turns a reactor's identity into a write grant.
    let rows = reactions
        .iter()
        .filter(|r| {
            !r.name.trim().is_empty() && r.count > 0 && names.get(r.name.as_str()) == Some(&1)
        })
        .take(50)
        .map(|r| {
            let mut seen = std::collections::HashSet::new();
            crate::state::ReactionVm {
                name: r.name.clone(),
                count: r.count,
                own: viewer.is_some_and(|me| r.users.iter().any(|id| id == me)),
                reactors: r
                    .users
                    .iter()
                    .filter(|id| !id.is_empty() && seen.insert(id.as_str()))
                    .take(crate::model::REACTOR_NAMES_SHOWN)
                    .map(|id| crate::model::ReactorVm {
                        user_id: id.clone(),
                        name: if viewer == Some(id.as_str()) {
                            "You".into()
                        } else {
                            w.display_name(id)
                        },
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();
    let groups = super::super::rich::grouped_reaction_vms(&rows);
    let prepared = read.and_then(|source| purpose::prepare(source, s, item));
    let disabled = prepared.as_ref().is_none_or(|row| row.disabled());
    let actions = groups
        .iter()
        .map(|group| {
            // A grouped label can be a synthesized family base with no actual
            // raw row. Bind the intent to its first supplied variant instead;
            // the fresh membership selector resolves this exact same family.
            let name = group.variants.first()?.name.as_str();
            prepared.as_ref()?.action(name)
        })
        .collect::<Vec<_>>();
    let status = purpose::status(s, item);
    let layout = s.core.session.is_none()
        && s.core.transport.is_none()
        && s.core.active_account.is_none()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
            Some("thread-feed-root-reactions-toggle" | "thread-feed-root-reactions-toggle-narrow")
        );
    if !layout && checked.is_none() && status.is_none() && actions.iter().all(Option::is_none) {
        return replies::root_counts(s, item);
    }
    rsx! {
        if !groups.is_empty(){
            div{class:"thread-feed-reactions","aria-label":"Reactions",
                for (index,group) in groups.iter().enumerate(){
                    {
                        let name=if group.variants.len()==1{group.variants[0].name.as_str()}else{group.name};
                        let description=if group.own{format!("{} · You reacted",group.label())}else{group.label()};
                        let glyphs=group.variants.iter().take(10).map(|v|super_platinum_core::state::emoji_glyph(&v.name)).collect::<Vec<_>>();
                        let overflow=group.variants.len().saturating_sub(10);
                        let content=rsx!{
                            span{class:"reaction-variants",
                                for glyph in glyphs{span{class:"reaction-glyph","{glyph}"}}
                                if overflow>0{span{class:"reaction-variants-overflow",title:format!("{overflow} more emoji variants"),
                                    {crate::icons::icon(crate::icons::Icon::More,"icon sm")}
                                }}
                            }
                            span{class:"reaction-count","{group.count}"}
                        };
                        let class=if group.own{"thread-feed-reaction own"}else{"thread-feed-reaction"};
                        rsx!{
                            if let Some(action)=actions[index].as_ref(){
                                {
                                    let control_key=action.id();
                                    rsx!{Pill{key:"{control_key}",state,action:action.clone(),disabled,
                                        name:name.to_owned(),description,count:group.count,class:class.to_owned(),children:content}}
                                }
                            }else if layout{
                                button{r#type:"button",class,disabled:true,"data-reaction-name":name,
                                    "aria-label":format!("{name}: {} reactions",group.count),title:description.clone(),"aria-description":description,
                                    {content}}
                            }else{
                                span{class,"data-reaction-name":name,"aria-label":format!("{name}: {} reactions",group.count),
                                    title:description.clone(),"aria-description":description,{content}}
                            }
                        }
                    }
                }
            }
        }
        if let Some(status)=status{
            p{role:"status",class:"channel-discussion-context","{status.notice}"}
            if let Some(check)=status.check{
                {
                    let control_key=check.id();
                    rsx!{CheckControl{key:"{control_key}",state,check,disabled:status.pending}}
                }
            }
        }else if layout{
            p{role:"status",class:"channel-discussion-context","Reaction controls are unavailable in this offline preview."}
        }
    }
}
#[component]
fn Pill(
    state: Signal<ShellState>,
    action: purpose::Action,
    disabled: bool,
    name: String,
    description: String,
    count: u64,
    class: String,
    children: Element,
) -> Element {
    let alive = use_hook(|| Arc::new(AtomicBool::new(true)));
    let retirement = alive.clone();
    use_drop(move || retirement.store(false, Ordering::Release));
    rsx! {button{r#type:"button",class,id:action.id(),disabled,"data-reaction-name":name.clone(),
    "aria-label":format!("{name}: {count} reactions"),title:description.clone(),"aria-description":description,
    onclick:move|event:MouseEvent|{
        let m=event.modifiers();
        if alive.load(Ordering::Acquire)&&!disabled&&!(m.ctrl()||m.meta()||m.alt()||m.shift())
            &&event.trigger_button().is_none_or(|b|b==dioxus::html::input_data::MouseButton::Primary){purpose::start(state,action.clone());}
    },{children}}}
}
#[component]
fn CheckControl(state: Signal<ShellState>, check: purpose::Check, disabled: bool) -> Element {
    let alive = use_hook(|| Arc::new(AtomicBool::new(true)));
    let retirement = alive.clone();
    use_drop(move || retirement.store(false, Ordering::Release));
    rsx! {button{r#type:"button",class:"chip",id:check.id(),disabled,
    onclick:move|event:MouseEvent|{
        let m=event.modifiers();
        if alive.load(Ordering::Acquire)&&!disabled&&!(m.ctrl()||m.meta()||m.alt()||m.shift())
            &&event.trigger_button().is_none_or(|b|b==dioxus::html::input_data::MouseButton::Primary){check.start(state);}
    },"Check current reactions"}}
}
