use super::*;
use crate::bootstrap::search_people::numbered as pages;

pub(crate) fn sort_controls(state: Signal<ShellState>, shell: &ShellState) -> Element {
    use super_platinum_core::slack::api::people_pages::Sort;
    let fixture = shell.search_people_pager_fixture
        || shell.search_people_save_fixture.is_some()
        || shell.search_people_account_fixture.is_some();
    let Some(sort) = (if fixture {
        Some(Sort::Relevant)
    } else {
        pages::current_sort(shell)
    }) else {
        return rsx! {};
    };
    let source = if fixture {
        None
    } else {
        pages::SortSource::capture(shell)
    };
    let saved_source = if fixture {
        None
    } else {
        pages::saved_sort::SavedSortSource::capture(shell)
    };
    let value = match sort {
        Sort::Relevant => "relevant",
        Sort::Name => "name",
        Sort::NameDescending => "name_desc",
    };
    let relevant = if fixture || pages::local_sort_allowed(shell) {
        "Most relevant (default)"
    } else {
        "Most relevant"
    };
    rsx! {
        {account_controls(state,shell,fixture)}
        select {"aria-label":"Sort people",value,disabled:source.is_none()&&saved_source.is_none(),"data-people-sort":true,
            onchange:move |event| {
                if let Some(sort)=Sort::parse(&event.value()) {
                    if let Some(source)=source.clone() {pages::change_sort(state,source,sort);}
                    else if let Some(source)=saved_source.clone() {pages::saved_sort::choose(state,source,sort);}
                }
            },
            option {value:"relevant","{relevant}"}
            option {value:"name","A to Z"}
            option {value:"name_desc","Z to A"}
        }
    }
}
fn account_controls(state: Signal<ShellState>, shell: &ShellState, fixture: bool) -> Element {
    use super_platinum_core::slack::api::people_pages::AccountType;
    let kind = if fixture {
        Some(
            shell
                .search_people_account_fixture
                .unwrap_or(AccountType::Any),
        )
    } else {
        pages::account_filter::current(shell)
    };
    let Some(kind) = kind else {
        return rsx! {};
    };
    let source = if fixture {
        None
    } else {
        pages::account_filter::FilterSource::capture(shell)
    };
    let value = kind.value();
    rsx! {select {"aria-label":"Filter account type",value,disabled:source.is_none(),"data-people-account-type":true,
        onchange:move |event| {if let (Some(source),Some(kind))=(source.clone(),AccountType::parse(&event.value())) {
            pages::account_filter::change(state,source,kind);
        }},
        for kind in [AccountType::Any,AccountType::RegularMembers,AccountType::Deactivated] {
            option {value:kind.value(),{kind.label()}}
        }
    }}
}

pub(super) fn results(state: Signal<ShellState>, shell: &ShellState) -> Element {
    rsx! {for (index,hit) in pages::rows(shell).iter().enumerate() { {row(state,shell,index,hit)} }}
}
fn row(
    state: Signal<ShellState>,
    shell: &ShellState,
    index: usize,
    hit: &super_platinum_core::slack::api::people_pages::Hit,
) -> Element {
    let source = pages::RowSource::capture(shell, index);
    let user = pages::row_user(shell, index);
    let name = user
        .map(|u| super_platinum_core::state::display_name(Some(u), hit.id()))
        .or_else(|| {
            hit.highlight()
                .filter(|s| {
                    !s.contains(['<', '>'])
                        && !s
                            .chars()
                            .any(|c| c.is_control() || ('\u{e000}'..='\u{f8ff}').contains(&c))
                })
                .map(str::to_owned)
        })
        .unwrap_or_else(|| hit.id().to_owned());
    let title = user
        .and_then(|u| u.profile.as_ref())
        .and_then(|p| p.title.clone());
    let email = user
        .and_then(|u| u.profile.as_ref())
        .and_then(|p| p.email.clone());
    let page = pages::page(shell);
    rsx! {
        button {key:"people-page-{page}-row-{index}",class:"search-result",disabled:source.is_none(),
            "data-search-person-id":hit.id(),"data-people-row":"{index}",
            onclick:move |_|{if let Some(source)=source.clone(){pages::open(state,source);}},
            strong {"{name}"}
            if let Some(title)=title.filter(|v|!v.trim().is_empty()) {span {"{title}"}}
            if let Some(email)=email.filter(|v|!v.trim().is_empty()) {span {class:"search-time","{email}"}}
        }
    }
}
pub(crate) fn controls(state: Signal<ShellState>, shell: &ShellState) -> Element {
    if let Some(fixture) = shell.search_people_save_fixture {
        let status = match fixture {
            pages::saved_sort::Fixture::Saving => "Saving sort preference\u{2026}",
            pages::saved_sort::Fixture::Unconfirmed => {
                "Couldn\u{2019}t confirm save. Your choice is retained."
            }
        };
        return rsx! {div {class:"search-footer","data-people-save":"presentation-fixture",
            p {role:"status","{status}"}
            if matches!(fixture,pages::saved_sort::Fixture::Unconfirmed) {button {class:"chip",disabled:true,"Retry save"}}
        }};
    }
    if shell.search_people_pager_fixture {
        return rsx! {
            div {class:"search-footer","data-people-pagination":"presentation-fixture",
                p {role:"status","Loading people\u{2026}"}
                p {"Page 1 of 2"}
                div {class:"search-controls",
                    button {class:"chip",disabled:true,"Previous"}
                    button {class:"chip",disabled:true,"Next"}
                }
            }
        };
    }
    if !pages::active(shell) {
        return rsx! {};
    }
    let page = pages::page(shell);
    let count = pages::page_count(shell);
    let loading = pages::loading(shell);
    let previous = page
        .checked_sub(1)
        .and_then(|p| pages::Selection::capture(shell, p));
    let next = page
        .checked_add(1)
        .filter(|_| page > 0)
        .and_then(|p| pages::Selection::capture(shell, p));
    let retry = pages::Retry::capture(shell);
    let save_status = pages::saved_sort::status(shell);
    let save_retry = pages::saved_sort::RetrySource::capture(shell);
    if page > 0 && count <= 1 && !loading && retry.is_none() && save_status.is_none() {
        return rsx! {};
    }
    let status = if retry.is_some() {
        if page > 0 {
            "This page could not load. Your previous page is retained."
        } else {
            "People could not load. Try again."
        }
    } else if loading {
        "Loading people…"
    } else {
        ""
    };
    rsx! {
        div {class:"search-footer","data-people-pagination":true,
            p {role:"status","{status}"}
            if let Some(status)=save_status {p {role:"status","data-people-save":true,"{status}"}}
            if page>0&&count>0 {p {"Page {page} of {count}"}}
            div {class:"search-controls",
                button {class:"chip",disabled:loading||previous.is_none(),
                    onclick:move |_|{if let Some(value)=previous.clone(){pages::select(state,value);}},"Previous"}
                button {class:"chip",disabled:loading||next.is_none(),
                    onclick:move |_|{if let Some(value)=next.clone(){pages::select(state,value);}},"Next"}
                if let Some(retry)=retry {button {class:"chip",disabled:loading,
                    onclick:move |_|pages::retry(state,retry.clone()),"Retry page"}}
                if let Some(retry)=save_retry {button {class:"chip",
                    onclick:move |_|pages::saved_sort::retry(state,retry.clone()),"Retry save"}}
            }
        }
    }
}
