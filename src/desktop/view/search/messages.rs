//! A single selected Messages page, reusing the existing native row projection.
use super::*;
pub(crate) mod reaction_picker;
use crate::bootstrap::search_message_pages as pages;
const PREVIOUS_ID: &str = "message-search-previous";
const NEXT_ID: &str = "message-search-next";
const RETRY_ID: &str = "message-search-retry";

pub(crate) fn owns(shell: &ShellState) -> bool {
    pages::has_scope(shell) || shell.search_message_pages_fixture.is_some()
}
pub(crate) fn results(state: Signal<ShellState>, shell: &ShellState) -> Element {
    if pages::has_scope(shell) && !pages::active(shell) {
        return rsx! {p {class:"results-empty",role:"status","Press Enter to refresh these results."}};
    }
    let owned = pages::has_scope(shell);
    let rows = if owned {
        pages::rows(shell)
    } else {
        &shell.search_results
    };
    let shown = pages::status(shell)
        .map(|s| s.0)
        .unwrap_or(shell.search_pager.page);
    rsx! {
    if let Some((message,retry))=pages::profiles::status(shell) {div {class:"search-footer",role:"status",
        p {"{message}"}
        if let Some(token)=retry {button {id:"message-search-author-retry",r#type:"button",class:"chip",
            onclick:move |_|pages::profiles::retry(state,token.clone()),"Retry author profiles"}}
    }}
    for (index,hit) in rows.iter().enumerate() {
        {row(state,shell,hit,index,shown,owned)}
    }}
}
fn row(
    mut state: Signal<ShellState>,
    shell: &ShellState,
    hit: &crate::state::SearchResultVm,
    index: usize,
    shown: u32,
    owned: bool,
) -> Element {
    let source = owned
        .then(|| pages::RowSource::capture(shell, index))
        .flatten();
    let author_source = owned
        .then(|| pages::profiles::control(shell, index))
        .flatten();
    let author_fixture = pages::profiles::fixture_view(shell) && index == 0;
    let legacy = hit.clone();
    let deleted = (owned && pages::row_was_deleted(shell, index))
        || (shell.search_message_pages_fixture == Some(pages::Fixture::Deleted) && index == 0);
    let disabled =
        deleted || (owned && source.is_none()) || shell.search_message_pages_fixture.is_some();
    let fixture_body =
        if shell.search_message_pages_fixture == Some(pages::Fixture::Emphasis) && index == 0 {
            pages::emphasis_fixture_body()
        } else if matches!(
            shell.search_message_pages_fixture,
            Some(pages::Fixture::Rich | pages::Fixture::Expanded)
        ) && index == 0
        {
            pages::disclosure_fixture_body()
        } else {
            Vec::new()
        };
    let body = if owned {
        pages::body(shell, index)
    } else {
        &fixture_body
    };
    let expanded = if owned {
        pages::is_expanded(shell, index)
    } else {
        shell.search_message_pages_fixture == Some(pages::Fixture::Expanded) && index == 0
    };
    let expandable = if owned {
        pages::can_expand(shell, index)
    } else {
        shell.search_message_pages_fixture == Some(pages::Fixture::Rich) && index == 0
    };
    let disclosure_source = source.clone();
    let thread_source = owned
        .then(|| pages::ThreadSource::capture(shell, index))
        .flatten();
    let reply_count = if owned {
        pages::reply_count(shell, index)
    } else {
        (shell.search_message_pages_fixture == Some(pages::Fixture::Thread) && index == 0)
            .then_some(3)
    };
    let reactions = if owned {
        pages::reaction_actions::fresh_reactions(shell, index)
            .unwrap_or_else(|| pages::reactions(shell, index))
    } else if shell.search_message_pages_fixture == Some(pages::Fixture::Reactions) && index == 0 {
        pages::fixture_reactions()
    } else if shell.search_message_pages_fixture == Some(pages::Fixture::ReactionGroups)
        && index == 0
    {
        pages::fixture_reaction_groups()
    } else {
        Vec::new()
    };
    let body_class = if expanded {
        "message-text search-message-body expanded"
    } else {
        "message-text search-message-body"
    };
    let reaction_source = owned
        .then(|| pages::RowSource::capture(shell, index))
        .flatten();
    let reaction_candidate = owned && pages::reaction_actions::candidate(shell, index);
    let reaction_available = reaction_candidate && pages::reaction_actions::available(shell, index);
    let key = if owned {
        format!("message-page-{shown}-row-{index}")
    } else {
        format!("{}-{}", hit.channel_id, hit.ts)
    };
    rsx! {div {class:"search-message-result",
        key:"{key}",
        button {
        id:"message-search-hit-{shown}-{index}",
        class:"search-result", disabled,
        onclick:move |_| {
            if let Some(source)=source.clone() {pages::open(state,source);}
            else if !disabled {crate::bootstrap::open_search_hit(state,legacy.clone());}
        },
        strong {{result_heading(hit)}}
        if deleted {span {"This message was deleted."}}
        else if body.is_empty() {span {"{hit.text}"}}
        else {div {id:"message-search-body-{shown}-{index}",class:body_class,
            for node in body {{super::super::rich::rich_node(node,shell.media_epoch,state)}}
        }}
        span {class:"search-time",{super_platinum_core::state::format_relative_ts(&hit.ts)}
            if hit.thread_ts.is_some() {" \u{b7} Thread reply"}
        }
        }
        if !deleted {if let Some(author)=author_source {button {
            id:"message-search-author-{shown}-{index}",r#type:"button",class:"search-message-more",
            "aria-label":"View author profile",onclick:move|event|{
                event.stop_propagation();
                if event.modifiers().is_empty() && event.trigger_button().is_none_or(|b|b==dioxus::html::input_data::MouseButton::Primary) {
                    pages::profiles::open(state,author.clone());
                }
            },"View author profile"
        }} else if author_fixture {button {id:"message-search-author-{shown}-{index}",class:"search-message-more",disabled:true,"View author profile"}}}
        if !deleted && !reactions.is_empty() {div {class:"message-actions",role:"group","aria-label":"Message reactions (read only)",
            {super::super::rich::reaction_groups_read_only(state,&shell.media,shell.media_epoch,&reactions)}
        }}
        if !deleted&&reaction_candidate{div{class:"message-actions",
            button{id:"message-search-react-{shown}-{index}",r#type:"button",class:"icon-btn","aria-label":"Add reaction","aria-haspopup":"dialog",
                disabled:disabled||!reaction_available,
                onclick:move|event|{event.stop_propagation();if let Some(source)=reaction_source.clone(){pages::reaction_actions::open(state,source);}},
                {crate::icons::icon(crate::icons::Icon::AddReaction,"icon sm")}
            }
        }}
        if !deleted {if let Some(count)=reply_count{button{
            id:"message-search-thread-{shown}-{index}",r#type:"button",class:"search-message-more",
            "aria-label":format!("View thread with {count} {}",if count==1{"reply"}else{"replies"}),
            disabled:disabled||thread_source.is_none(),
            onclick:move|event|{event.stop_propagation();if let Some(source)=thread_source.clone(){pages::open_thread(state,source);}},
            "{count} " if count==1{"reply"}else{"replies"}
        }}}
        if expandable && !deleted {button {
            id:"message-search-expand-{shown}-{index}",r#type:"button",
            class:"search-message-more",disabled,
            "aria-expanded":"false",
            "aria-controls":"message-search-body-{shown}-{index}",
            onclick:move |event| {
                event.stop_propagation();
                if let Some(source)=disclosure_source.as_ref() {pages::expand(&mut state.write(),source);}
            },"Show more"
        }}
    }}
}

/// The Source five-number window, clamped at both ends without overflowing u32.
fn range(page: u32, bound: u32) -> Vec<u32> {
    if page == 0 || bound == 0 || page > bound {
        return Vec::new();
    }
    let start = page.saturating_sub(2).min(bound.saturating_sub(4)).max(1);
    (0..5)
        .filter_map(|offset| start.checked_add(offset))
        .filter(|p| *p <= bound)
        .collect()
}
pub(crate) fn controls(state: Signal<ShellState>, shell: &ShellState) -> Element {
    let fixture = shell.search_message_pages_fixture;
    if shell.search_pager.kind != SearchKind::Messages
        || shell.search_filters.is_some()
        || shell.search_history_open
    {
        return rsx! {};
    }
    let status = pages::status(shell).or_else(|| {
        fixture.map(|f| {
            (
                if f == pages::Fixture::Second { 2 } else { 1 },
                if f == pages::Fixture::Failed {
                    4
                } else if f == pages::Fixture::Second {
                    2
                } else {
                    1
                },
                9,
                172,
                false,
                f == pages::Fixture::Failed,
            )
        })
    });
    let Some((shown, target, bound, _total, loading, failed)) = status else {
        return rsx! {};
    };
    let source = pages::Control::capture(shell);
    let previous = source.clone();
    let next = source.clone();
    let retry = source.clone();
    rsx! {div {class:"search-footer","data-message-pagination":if fixture.is_some(){"presentation-fixture"}else{"owned"},
        p {role:"status",
            if loading {"Loading page {target}\u{2026} Showing page {shown} of {bound}."}
            else if failed {"Page {target} couldn\u{2019}t load. Showing page {shown} of {bound}."}
            else {"Page {shown} of {bound}"}
        }
        if failed {button {id:RETRY_ID,class:"chip",disabled:loading||retry.is_none(),
            onclick:move |_|{if let Some(source)=retry.clone(){pages::retry(state,source);}},"Retry page {target}"}}
        if bound > 1 {div {class:"search-controls",role:"navigation","aria-label":"Message search pages",
            button {id:PREVIOUS_ID,class:"chip","aria-label":"Previous page",disabled:loading||previous.is_none()||shown<=1,
                onclick:move |_|{if let Some(source)=previous.clone(){if let Some(page)=shown.checked_sub(1){pages::choose(state,source,page);}}},"Previous"}
            for number in range(shown,bound) {
                button {class:"chip","aria-label":"Page {number}","aria-current":(number==shown).then_some("page"),
                    disabled:loading||source.is_none()||number==shown,
                    onclick:{let source=source.clone();move |_|{if let Some(source)=source.clone(){pages::choose(state,source,number);}}},"{number}"}
            }
            button {id:NEXT_ID,class:"chip","aria-label":"Next page",disabled:loading||next.is_none()||shown>=bound,
                onclick:move |_|{if let Some(source)=next.clone(){if let Some(page)=shown.checked_add(1){pages::choose(state,source,page);}}},"Next"}
        }}
    }}
}

#[cfg(test)]
mod tests;
