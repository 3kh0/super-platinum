pub(crate) mod check;
mod file_types;
mod files;
mod filters;
pub(crate) mod history;
mod message_scopes;
mod messages;
pub(crate) use files::{
    busy_or_retired as file_page_busy_or_retired, controls as file_page_controls,
    owns as file_page_scope,
};
mod people;
use crate::state::ShellState;
use dioxus::prelude::*;
pub(crate) use messages::{
    controls as message_page_controls, owns as message_page_scope, results as message_results,
};
pub(crate) use people::numbered_controls as people_page_controls;
pub(crate) use people::results as people_results;
use super_platinum_core::{
    search::{SearchKind, SearchSort},
    search_query::SearchFilters,
};

pub(crate) fn result_heading(hit: &crate::state::SearchResultVm) -> String {
    let prefix = if hit.direct_conversation { "" } else { "# " };
    format!("{prefix}{} · {}", hit.channel_name, hit.author)
}

pub(crate) fn result_count(shell: &ShellState) -> usize {
    match shell.search_pager.kind {
        SearchKind::Messages => {
            if crate::bootstrap::search_message_pages::has_scope(shell) {
                crate::bootstrap::search_message_pages::rows(shell).len()
            } else {
                shell.search_results.len()
            }
        }
        SearchKind::People => {
            if crate::bootstrap::search_people::numbered::active(shell) {
                crate::bootstrap::search_people::numbered::rows(shell).len()
            } else {
                crate::bootstrap::search_people::rows(shell).len()
            }
        }
        SearchKind::Files if files::owns(shell) => files::count(shell),
        SearchKind::Files => shell
            .search_file_results
            .iter()
            .filter(|f| crate::bootstrap::search_files::visible(shell, f))
            .count(),
    }
}

pub(crate) fn supports_filters(shell: &ShellState) -> bool {
    shell.search_pager.kind != SearchKind::People
}
pub(crate) fn summary(shell: &ShellState) -> String {
    if files::owns(shell) {
        return files::summary(shell);
    }
    if crate::bootstrap::search_message_pages::has_scope(shell)
        && !crate::bootstrap::search_message_pages::active(shell)
    {
        return "Search results need refreshing.".into();
    }
    if crate::bootstrap::search_people::numbered::active(shell) {
        return format!(
            "{} of {} people",
            result_count(shell),
            crate::bootstrap::search_people::numbered::total(shell)
        );
    }
    if shell.search_pager.kind == SearchKind::People {
        format!(
            "{} shown · Up to {} people",
            result_count(shell),
            super_platinum_core::slack::api::USER_SEARCH_LIMIT
        )
    } else {
        format!(
            "{} of {} results · {}",
            result_count(shell),
            shell.search_pager.total,
            shell.search_pager.sort.label()
        )
    }
}

pub(crate) fn file_results(state: Signal<ShellState>, shell: &ShellState) -> Element {
    if files::owns(shell) {
        return files::results(state, shell);
    }
    let workspace = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t));
    rsx! {
        for file in shell.search_file_results.iter().filter(|f| crate::bootstrap::search_files::visible(shell, f)) {
            if let Some(source) = file.id.as_deref().and_then(|id| crate::bootstrap::search_files::Source::capture(shell, id)) {
                button { key: "{file.id.as_deref().unwrap_or_default()}", class:"search-result", "data-search-file-id":file.id.as_deref(),
                    onclick:move |_|crate::bootstrap::search_files::open(state, source.clone()),
                    strong {{crate::icons::icon(crate::icons::Icon::AttachFile,"icon")} " " {super_platinum_core::state::file_title(file)}}
                    span {{super_platinum_core::state::file_summary(file)}}
                    if let Some(user)=super_platinum_core::state::file_uploader_id(file) {
                        span {class:"search-time",{super_platinum_core::state::display_name(workspace.and_then(|w|w.users.get(user)),user)}}
                    }
                }
            }
        }
    }
}

pub(crate) fn controls(mut state: Signal<ShellState>, shell: &ShellState) -> Element {
    let filters = if shell.search_history_open || !supports_filters(shell) {
        None
    } else {
        shell.search_filters.clone()
    };
    let kind_source = crate::bootstrap::search_control_memory::Control::capture(shell);
    let sort_source = kind_source.clone();
    let filter_toggle = filters::Toggle::capture(shell);
    let filter_source = filters::Source::capture(shell);
    let clear_source = filter_source.clone();
    let apply_source = filter_source.clone();
    let disabled_filters = !filter_source.allows(shell);
    rsx! {
        div { class: "search-controls",
            select { "aria-label": "Search type", value: shell.search_pager.kind.label(),
                onchange: move |event| {
                    if let Some(kind) = SearchKind::ALL.into_iter().find(|k| k.label() == event.value()) {
                        if let Some(source)=kind_source.clone() {crate::bootstrap::search_control_memory::choose_kind(state,source,kind);}
                    }
                },
                for kind in SearchKind::ALL { option { value: kind.label(), "{kind.label()}" } }
            }
            if supports_filters(shell) { button { "aria-expanded": filters.is_some(), "aria-controls": "search-filter-panel",
                onclick: move |_| {
                    filters::toggle(&mut state.write(), filter_toggle.clone());
                }, "Filters"
            }}
            if supports_filters(shell) && !files::owns(shell) { select { "aria-label": "Sort results", value: "{shell.search_pager.sort.label()}",
                onchange: move |event| {
                    if let Some(sort) = SearchSort::ALL.into_iter().find(|sort| sort.label() == event.value()) {
                        if let Some(source)=sort_source.clone() {crate::bootstrap::search_control_memory::choose_sort(state,source,sort);}
                    }
                },
                for sort in SearchSort::ALL { option { value: "{sort.label()}", "{sort.label()}" } }
            }
            }
            if shell.search_pager.kind == SearchKind::Files { {files::sort_control(state,shell)} }
            if shell.search_pager.kind == SearchKind::People { {people::sort_controls(state,shell)} }
            button { title:"Recent searches", "aria-label":"Recent searches",
                "aria-expanded":shell.search_history_open, "aria-controls":"search-history-panel",
                onclick:move |_|history::toggle(&mut state.write()),
                {crate::icons::icon(crate::icons::Icon::Schedule,"icon")}
            }
        }
        if shell.search_history_open { {history::panel(state,shell)} }
        if let Some(ref filters) = filters {
            div { id: "search-filter-panel", class: "search-filter-panel",
                {message_scopes::panel(state, shell)}
                div { class: "search-filter-grid",
                    if !crate::state::search_message_scopes::typed_panel(shell) {
                    {field(state, "In channel", "Channel name", "channel", &filters.channel, Some("search-channel-options"), filter_source.clone())}
                    {field(state, "From", "Username or user ID", "sender", &filters.sender, Some("search-sender-options"), filter_source.clone())}
                    }
                    if shell.search_pager.kind == SearchKind::Messages {
                        {field(state, "With person", "@Name", "with_person", &filters.with_person, Some("search-with-person-options"), filter_source.clone())}
                    }
                    {field(state, "On date", "YYYY-MM-DD", "on", &filters.on, None, filter_source.clone())}
                    {field(state, "After date", "YYYY-MM-DD", "after", &filters.after, None, filter_source.clone())}
                    {field(state, "Before date", "YYYY-MM-DD", "before", &filters.before, None, filter_source.clone())}
                }
                {file_types::panel(state, shell)}
                if shell.search_pager.kind == SearchKind::Messages { div { class: "search-filter-options",
                    {has_filter(state, "Has files", MessageFlag::File, filters.has_file, filter_source.clone())}
                    {has_filter(state, "Has links", MessageFlag::Link, filters.has_link, filter_source.clone())}
                    {has_filter(state, "In threads", MessageFlag::Thread, filters.in_thread, filter_source.clone())}
                    {has_filter(state, "Saved items", MessageFlag::Saved, filters.saved, filter_source.clone())}
                    {has_filter(state, "Pinned messages", MessageFlag::Pinned, filters.pinned, filter_source.clone())}
                }}
                datalist { id: "search-channel-options",
                    for name in channel_options(shell, &filters.channel) { option { value: "{name}" } }
                }
                datalist { id: "search-sender-options",
                    option { value: "me", label: "You" }
                    for (value, name) in sender_options(shell, &filters.sender) { option { value: "{value}", label: "{name}" } }
                }
                if shell.search_pager.kind == SearchKind::Messages {
                    datalist { id: "search-with-person-options",
                        for (value, name) in with_options(shell, &filters.with_person) { option { value: "{value}", label: "{name}" } }
                    }
                }
                if let Some(error) = shell.search_filter_error { p { role: "alert", "{error}" } }
                div { class: "search-filter-actions",
                    button { "data-file-filter-clear": "true", disabled: disabled_filters, onclick: move |_| {
                        filters::clear(state, clear_source.clone());
                    }, "Clear filters" }
                    button { "data-file-filter-apply": "true", disabled: disabled_filters, onclick: move |_| {
                        filters::submit(state, apply_source.clone());
                    }, "Apply filters" }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum MessageFlag {
    File,
    Link,
    Thread,
    Saved,
    Pinned,
}
fn has_filter(
    mut state: Signal<ShellState>,
    label: &str,
    flag: MessageFlag,
    checked: bool,
    source: filters::Source,
) -> Element {
    rsx! {
        label {
            input { r#type: "checkbox", "aria-label": label, checked,
                onchange: move |event| {
                    let mut shell = state.write();
                    if !source.allows(&shell){return;}
                    shell.search_filter_error = None;
                    if let Some(filters) = shell.search_filters.as_mut() {
                        match flag {
                            MessageFlag::File => filters.has_file = event.checked(),
                            MessageFlag::Link => filters.has_link = event.checked(),
                            MessageFlag::Thread => filters.in_thread = event.checked(),
                            MessageFlag::Saved => filters.saved = event.checked(),
                            MessageFlag::Pinned => filters.pinned = event.checked(),
                        }
                    }
                }
            }
            "{label}"
        }
    }
}

fn field(
    mut state: Signal<ShellState>,
    label: &str,
    placeholder: &str,
    name: &'static str,
    value: &str,
    list: Option<&str>,
    source: filters::Source,
) -> Element {
    let key = source
        .key()
        .map(|key| format!("{name}-{key}"))
        .unwrap_or_else(|| name.into());
    let input_source = source.clone();
    let enabled = {
        let shell = state.peek();
        source.allows(&shell)
    };
    rsx! {
        label { "{label}",
            input { key: "{key}", "aria-label": "{label}", placeholder, list, initial_value: value, disabled: !enabled,
                autocomplete: "off", "autocorrect": "off", autocapitalize: "none", spellcheck: "false",
                oninput: move |event| {
                    let mut shell = state.write();
                    if !enabled || !input_source.allows(&shell){return;}
                    shell.search_filter_error = None;
                    if let Some(filters) = shell.search_filters.as_mut() {
                        match name {
                            "channel" => filters.channel = event.value(),
                            "sender" => filters.sender = event.value(),
                            "with_person" => filters.with_person = event.value(),
                            "on" => filters.on = event.value(),
                            "after" => filters.after = event.value(),
                            "before" => filters.before = event.value(),
                            _ => {}
                        }
                    }
                },
                onkeydown: move |event| {
                    if event.key() == Key::Enter {
                        event.prevent_default();
                        filters::submit(state, source.clone());
                    }
                },
            }
        }
    }
}

fn channel_options(shell: &ShellState, query: &str) -> Vec<String> {
    let query = query.trim_start_matches('#').to_lowercase();
    let mut names = shell
        .channels
        .iter()
        .filter(|c| !c.is_im && !c.is_mpim && c.name.to_lowercase().contains(&query))
        .map(|c| c.name.clone())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    names.truncate(30);
    names
}

fn sender_options(shell: &ShellState, query: &str) -> Vec<(String, String)> {
    let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
    else {
        return Vec::new();
    };
    let query = query.trim_start_matches('@').to_lowercase();
    let mut users = workspace
        .users
        .values()
        .filter(|user| !user.deleted)
        .filter_map(|user| {
            let name = super_platinum_core::state::display_name(Some(user), &user.id);
            let username = user.name.as_deref().unwrap_or(&user.id);
            (name.to_lowercase().contains(&query) || username.to_lowercase().contains(&query))
                .then(|| (format!("<@{}>", user.id), name))
        })
        .collect::<Vec<_>>();
    users.sort_unstable_by(|a, b| a.1.cmp(&b.1));
    users.truncate(30);
    users
}

// The documented with: syntax takes a person's display name. Keep these bounded
// cached suggestions independent of the From field; no ID syntax is invented.
fn with_options(shell: &ShellState, query: &str) -> Vec<(String, String)> {
    let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
    else {
        return Vec::new();
    };
    let query = query.trim_start_matches('@').to_lowercase();
    let mut names = workspace
        .users
        .values()
        .filter(|u| !u.deleted)
        .filter_map(|u| {
            let name = super_platinum_core::state::display_name(Some(u), &u.id);
            (name != u.id && name.to_lowercase().contains(&query))
                .then(|| (format!("@{name}"), name))
        })
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    names.truncate(30);
    names
}

#[cfg(test)]
mod context_tests;
