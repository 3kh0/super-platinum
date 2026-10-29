//! Private Files find controls. Informational hits carry no opener or URL.
use super::*;
use crate::bootstrap::search_file_pages as pages;
use super_platinum_core::slack::api::file_pages::{Page, Sort};
const SORT_ID: &str = "file-search-sort";
const PREVIOUS_ID: &str = "file-search-previous";
const NEXT_ID: &str = "file-search-next";
const RETRY_ID: &str = "file-search-retry";
pub(crate) fn owns(s: &ShellState) -> bool {
    s.search_pager.kind == SearchKind::Files
        && (pages::has_scope(s) || s.search_file_pages_fixture.is_some())
}
fn status(s: &ShellState) -> Option<(u32, u32, u32, u64, bool, bool, Sort)> {
    pages::status(s).or_else(|| {
        s.search_file_pages_fixture.map(|failed| {
            (
                1,
                if failed { 4 } else { 1 },
                7,
                129,
                false,
                failed,
                Sort::Newest,
            )
        })
    })
}
pub(crate) fn busy_or_retired(s: &ShellState) -> bool {
    owns(s) && status(s).is_none_or(|v| v.4)
}
pub(crate) fn count(s: &ShellState) -> usize {
    if s.search_file_pages_fixture.is_some() {
        3
    } else {
        pages::rows(s)
            .iter()
            .filter(|hit| pages::visible(s, hit))
            .count()
    }
}
pub(crate) fn summary(s: &ShellState) -> String {
    status(s)
        .map(|v| format!("{} of {} files \u{b7} {}", count(s), v.3, v.6.label()))
        .unwrap_or_else(|| "Search results need refreshing.".into())
}
fn presentation_page() -> Page {
    Page::decode(serde_json::json!({"ok":true,"items":[{"id":"FFIRST","title":"Product launch notes","pretty_type":"PDF","size":24576},{"id":"FSECOND","title":"Launch campaign image","pretty_type":"PNG","size":8192},{"id":"FFIRST","title":"Product launch notes (second occurrence)","pretty_type":"PDF","size":24576}],"pagination":{"page":1,"page_count":7,"total_count":129}}),1).unwrap()
}
pub(crate) fn results(_state: Signal<ShellState>, s: &ShellState) -> Element {
    if pages::has_scope(s) && !pages::active(s) {
        return rsx! {p{class:"results-empty",role:"status","Press Enter to refresh these file results."}};
    }
    if status(s).is_some_and(|v| v.4) {
        return rsx! {p{class:"results-empty",role:"status","Loading files\u{2026}"}};
    }
    let fixture = s.search_file_pages_fixture.map(|_| presentation_page());
    let rows = fixture
        .as_ref()
        .map(Page::hits)
        .unwrap_or_else(|| pages::rows(s));
    let shown = status(s).map(|v| v.0).unwrap_or(0);
    rsx! {div{role:"list","aria-label":"File search results",for(index,hit)in rows.iter().enumerate().filter(|(_,hit)|pages::visible(s,hit)){
        article{key:"file-page-{shown}-occurrence-{index}",class:"search-result",role:"listitem","data-private-file-id":hit.id(),
            strong{{crate::icons::icon(crate::icons::Icon::AttachFile,"icon")} " " {hit.title()}}
            if let Some(kind)=hit.kind(){span{"{kind}"}}
            if let Some(size)=hit.size(){span{class:"search-time","{size} bytes"}}
        }
    }}}
}
pub(crate) fn sort_control(state: Signal<ShellState>, s: &ShellState) -> Element {
    let source = pages::EntrySource::capture(s);
    let saved = pages::saved::Source::capture(s);
    if source.is_none() && saved.is_none() && !owns(s) {
        return rsx! {};
    }
    let value = status(s).map(|v| v.6.raw()).unwrap_or("");
    rsx! {select{id:SORT_ID,"aria-label":"Sort files",value,disabled:source.is_none()&&saved.is_none(),
        onchange:move|event|{if let Some(sort)=Sort::ALL.into_iter().find(|v|v.raw()==event.value()){if let Some(saved)=saved.clone(){pages::saved::choose(state,saved,sort);}else if let Some(source)=source.clone(){pages::change_sort(state,source,sort);}}},
        if value.is_empty(){option{value:"",disabled:true,"Choose file order\u{2026}"}}
        for sort in Sort::ALL{option{value:sort.raw(),"{sort.label()}"}}
    }}
}
fn range(page: u32, bound: u32) -> Vec<u32> {
    if page == 0 || bound == 0 || page > bound {
        return Vec::new();
    }
    let start = page.saturating_sub(2).min(bound.saturating_sub(4)).max(1);
    (0..5)
        .filter_map(|n| start.checked_add(n))
        .filter(|p| *p <= bound)
        .collect()
}
pub(crate) fn controls(state: Signal<ShellState>, s: &ShellState) -> Element {
    if !owns(s) || s.search_filters.is_some() || s.search_history_open {
        return rsx! {};
    }
    let Some((shown, target, bound, _, loading, failed, _)) = status(s) else {
        return rsx! {};
    };
    let source = pages::Control::capture(s);
    let previous = source.clone();
    let next = source.clone();
    let retry = source.clone();
    let save_retry = pages::saved::RetrySource::capture(s);
    let save_status = pages::saved::status(s);
    rsx! {div{class:"search-footer","data-file-pagination":if s.search_file_pages_fixture.is_some(){"presentation-fixture"}else{"owned"},
        if pages::saved::owns(s){p{role:"status","aria-live":"polite","data-file-save-status":"true",{save_status.unwrap_or("\u{a0}")}}}
        if save_retry.is_some(){button{id:"file-search-save-retry",class:"chip",onclick:move |_|{if let Some(source)=save_retry.clone(){pages::saved::retry(state,source);}},"Retry saving sort"}}
        p{role:"status",if loading{"Loading page {target}\u{2026}"}else if failed{if shown>0{"Page {target} couldn\u{2019}t load. Showing page {shown} of {bound}."}else{"File search couldn\u{2019}t load."}}else if bound==0{"No files found."}else{"Page {shown} of {bound}"}}
        if failed{button{id:RETRY_ID,class:"chip",disabled:loading||retry.is_none(),onclick:move |_|{if let Some(source)=retry.clone(){pages::retry(state,source);}},"Retry page {target}"}}
        if bound>1{div{class:"search-controls",role:"navigation","aria-label":"File search pages",
            button{id:PREVIOUS_ID,class:"chip",disabled:loading||previous.is_none()||shown<=1,onclick:move |_|{if let Some(source)=previous.clone(){if let Some(page)=shown.checked_sub(1){pages::choose(state,source,page);}}},"Previous"}
            for number in range(shown,bound){button{class:"chip","aria-label":"File page {number}","aria-current":(number==shown).then_some("page"),disabled:loading||source.is_none()||number==shown,onclick:{let source=source.clone();move |_|{if let Some(source)=source.clone(){pages::choose(state,source,number);}}},"{number}"}}
            button{id:NEXT_ID,class:"chip",disabled:loading||next.is_none()||shown>=bound,onclick:move |_|{if let Some(source)=next.clone(){if let Some(page)=shown.checked_add(1){pages::choose(state,source,page);}}},"Next"}
        }}
    }}
}
#[cfg(test)]
mod tests;
