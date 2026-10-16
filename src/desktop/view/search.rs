pub(crate) mod check;
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::{search::SearchSort, search_query::SearchFilters};

pub(crate) fn controls(mut state: Signal<ShellState>, shell: &ShellState) -> Element {
    let filters = shell.search_filters.clone();
    rsx! {
        div { class: "search-controls",
            button { "aria-expanded": filters.is_some(), "aria-controls": "search-filter-panel",
                onclick: move |_| {
                    let mut shell = state.write();
                    shell.search_filter_error = None;
                    shell.search_filters = if shell.search_filters.is_some() { None } else { Some(SearchFilters::from_query(&shell.search_query)) };
                }, "Filters"
            }
            select { "aria-label": "Sort messages", value: "{shell.search_pager.sort.label()}",
                onchange: move |event| {
                    if let Some(sort) = SearchSort::ALL.into_iter().find(|sort| sort.label() == event.value()) {
                        let changed = {
                            let mut shell = state.write();
                            let changed = shell.search_pager.change_sort(sort);
                            if changed { shell.cancel_search_open(); shell.search_results.clear(); }
                            changed
                        };
                        if changed { spawn(crate::bootstrap::search(state)); }
                    }
                },
                for sort in SearchSort::ALL { option { value: "{sort.label()}", "{sort.label()}" } }
            }
        }
        if let Some(ref filters) = filters {
            div { id: "search-filter-panel", class: "search-filter-panel",
                div { class: "search-filter-grid",
                    {field(state, "In channel", "Channel name", "channel", &filters.channel, Some("search-channel-options"))}
                    {field(state, "From", "Username or user ID", "sender", &filters.sender, Some("search-sender-options"))}
                    {field(state, "After date", "YYYY-MM-DD", "after", &filters.after, None)}
                    {field(state, "Before date", "YYYY-MM-DD", "before", &filters.before, None)}
                }
                div { class: "search-filter-options",
                    {has_filter(state, "Has files", true, filters.has_file)}
                    {has_filter(state, "Has links", false, filters.has_link)}
                }
                datalist { id: "search-channel-options",
                    for name in channel_options(shell, &filters.channel) { option { value: "{name}" } }
                }
                datalist { id: "search-sender-options",
                    option { value: "me", label: "You" }
                    for (value, name) in sender_options(shell, &filters.sender) { option { value: "{value}", label: "{name}" } }
                }
                if let Some(error) = shell.search_filter_error { p { role: "alert", "{error}" } }
                div { class: "search-filter-actions",
                    button { onclick: move |_| {
                        state.write().search_filters = Some(SearchFilters::default());
                        if state.write().apply_search_filters() { spawn(crate::bootstrap::search(state)); }
                    }, "Clear filters" }
                    button { onclick: move |_| {
                        if state.write().apply_search_filters() { spawn(crate::bootstrap::search(state)); }
                    }, "Apply filters" }
                }
            }
        }
    }
}

fn has_filter(mut state: Signal<ShellState>, label: &str, file: bool, checked: bool) -> Element {
    rsx! {
        label {
            input { r#type: "checkbox", checked,
                onchange: move |event| {
                    let mut shell = state.write();
                    shell.search_filter_error = None;
                    if let Some(filters) = shell.search_filters.as_mut() {
                        if file { filters.has_file = event.checked(); }
                        else { filters.has_link = event.checked(); }
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
) -> Element {
    rsx! {
        label { "{label}",
            input { key: "{name}", "aria-label": "{label}", placeholder, list, initial_value: value,
                autocomplete: "off", "autocorrect": "off", autocapitalize: "none", spellcheck: "false",
                oninput: move |event| {
                    let mut shell = state.write();
                    shell.search_filter_error = None;
                    if let Some(filters) = shell.search_filters.as_mut() {
                        match name {
                            "channel" => filters.channel = event.value(),
                            "sender" => filters.sender = event.value(),
                            "after" => filters.after = event.value(),
                            "before" => filters.before = event.value(),
                            _ => {}
                        }
                    }
                },
                onkeydown: move |event| {
                    if event.key() == Key::Enter {
                        event.prevent_default();
                        if state.write().apply_search_filters() { spawn(crate::bootstrap::search(state)); }
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
