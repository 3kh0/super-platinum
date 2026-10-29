//! A single selected Messages page, reusing the existing native row projection.
use super::*;
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
    rsx! {for (index,hit) in rows.iter().enumerate() {
        {row(state,shell,hit,index,shown,owned)}
    }}
}
fn row(
    state: Signal<ShellState>,
    shell: &ShellState,
    hit: &crate::state::SearchResultVm,
    index: usize,
    shown: u32,
    owned: bool,
) -> Element {
    let source = owned
        .then(|| pages::RowSource::capture(shell, index))
        .flatten();
    let legacy = hit.clone();
    let disabled = (owned && source.is_none()) || shell.search_message_pages_fixture.is_some();
    let key = if owned {
        format!("message-page-{shown}-row-{index}")
    } else {
        format!("{}-{}", hit.channel_id, hit.ts)
    };
    rsx! {button {
        key:"{key}",
        class:"search-result", disabled,
        onclick:move |_| {
            if let Some(source)=source.clone() {pages::open(state,source);}
            else if !disabled {crate::bootstrap::open_search_hit(state,legacy.clone());}
        },
        strong {{result_heading(hit)}}
        span {"{hit.text}"}
        span {class:"search-time",{super_platinum_core::state::format_relative_ts(&hit.ts)}
            if hit.thread_ts.is_some() {" \u{b7} Thread reply"}
        }
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
