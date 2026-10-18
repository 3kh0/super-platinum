//! Selected folder reads share tab ownership and the existing bookmark row UI.
use super::*;
use super_platinum_core::{
    channel_tabs::folder::{Contents, Folder, Item, Open},
    slack::{Error, api},
};

#[derive(Default)]
pub(crate) struct State {
    pub contents: Option<Contents>,
    pub loading: bool,
    pub notice: String,
    pub cooling: bool,
    serial: u64,
    loading_folder: Option<Folder>,
    loading_owner: Option<Owner>,
    opening: Option<u64>,
    retry_at: Option<Instant>,
}
pub(crate) fn selected(shell: &ShellState) -> Option<Folder> {
    match selection::current(shell)? {
        selection::Route::Folder(folder) => Some(folder),
        _ => None,
    }
}
fn active(shell: &ShellState, owner: &Owner, folder: &Folder, serial: u64) -> bool {
    owner.current(shell)
        && shell.overlay.is_none()
        && shell.conversation_visible()
        && selected(shell).as_ref() == Some(folder)
        && shell
            .channel_tabs
            .entries
            .get(&owner.key)
            .is_some_and(|e| e.folder.serial == serial)
}
fn membership(
    mut state: Signal<ShellState>,
    owner: &Owner,
    folder: &Folder,
    serial: u64,
    base: &Inventory,
) -> bool {
    let mut shell = state.write();
    if !active(&shell, owner, folder, serial) {
        return false;
    }
    let valid = folder.current(base);
    let e = shell.channel_tabs.entries.get_mut(&owner.key).unwrap();
    e.inventory = Some(base.clone());
    if !valid {
        e.folder.contents = None;
    }
    valid
}
pub(crate) fn load(state: Signal<ShellState>) {
    let Some((owner, folder, serial)) = prepare(state) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        read(state, owner, folder, serial).await;
    });
}
pub(super) fn prepare(mut state: Signal<ShellState>) -> Option<(Owner, Folder, u64)> {
    let visit = Visit::capture(&state.peek())?;
    let folder = selected(&state.peek())?;
    let base = inventory(&state.peek())?;
    let owner = (*visit.0).clone();
    let serial = {
        let mut shell = state.write();
        if !owner.current(&shell) || shell.overlay.is_some() || !shell.conversation_visible() {
            return None;
        }
        if shell.channel_tabs.entries.get(&owner.key).is_some_and(|e| {
            (e.folder.loading
                && e.folder.loading_folder.as_ref() == Some(&folder)
                && e.folder
                    .loading_owner
                    .as_ref()
                    .is_some_and(|old| old.current(&shell)))
                || e.folder.cooling
        }) {
            return None;
        }
        let root = &mut shell.channel_tabs;
        if !root.entries.contains_key(&owner.key) && root.entries.len() >= 32 {
            return None;
        }
        root.next = root.next.wrapping_add(1);
        let serial = root.next;
        let e = root.entries.entry(owner.key.clone()).or_default();
        e.inventory.get_or_insert(base);
        e.folder.serial = serial;
        e.folder.loading = true;
        e.folder.loading_folder = Some(folder.clone());
        e.folder.loading_owner = Some(owner.clone());
        e.folder.opening = None;
        e.folder.notice.clear();
        serial
    };
    Some((owner, folder, serial))
}
pub(super) async fn read(mut state: Signal<ShellState>, owner: Owner, folder: Folder, serial: u64) {
    let result = async {
        if !active(&state.peek(), &owner, &folder, serial) {
            return Err(Error::Api("folder_changed".into()));
        }
        let (_, base) = run::read(&owner).await?;
        if !membership(state, &owner, &folder, serial, &base) {
            return Err(Error::Api("folder_changed".into()));
        }
        let response = owner
            .execute(api::bookmarks_list(
                &owner.client,
                &owner.session,
                &owner.key.channel,
            ))
            .await?;
        if !active(&state.peek(), &owner, &folder, serial) {
            return Err(Error::Api("folder_changed".into()));
        }
        let contents = Contents::parse(&base, &folder, &response)
            .map_err(|_| Error::Api("folder_contents_unavailable".into()))?;
        let (_, current) = run::read(&owner).await?;
        if !membership(state, &owner, &folder, serial, &current) {
            return Err(Error::Api("folder_changed".into()));
        }
        Ok((current, contents))
    }
    .await;
    let account_current = owner.account_current(&state.peek());
    let still_active = active(&state.peek(), &owner, &folder, serial);
    let mut shell = state.write();
    let Some(e) = shell
        .channel_tabs
        .entries
        .get_mut(&owner.key)
        .filter(|e| e.folder.serial == serial)
    else {
        return;
    };
    e.folder.loading = false;
    e.folder.loading_folder = None;
    e.folder.loading_owner = None;
    if !account_current {
        // Retire only this read's busy flag. No result crosses account custody;
        // returning to the old account can explicitly start a fresh read.
        return;
    }
    match result {
        Ok((base, contents)) if still_active => {
            e.inventory = Some(base);
            e.folder.contents = Some(contents);
            e.folder.notice.clear();
        }
        Err(error) if still_active => {
            drop(shell);
            failure(state, &owner, &folder, serial, None, error);
        }
        _ => {
            e.folder.notice =
                "The folder could not refresh. Return to its current tab and try again.".into();
        }
    }
}
pub(crate) fn open_item(
    mut state: Signal<ShellState>,
    visit: &Visit,
    folder: Folder,
    serial: u64,
    item: Item,
) {
    let owner = (*visit.0).clone();
    if !active(&state.peek(), &owner, &folder, serial)
        || !entry(&state.peek())
            .filter(|e| !e.folder.loading && !e.folder.cooling)
            .and_then(|e| e.folder.contents.as_ref())
            .is_some_and(|c| c.folder() == &folder && c.items().contains(&item))
    {
        return;
    }
    match item.open {
        Open::Link(url) => {
            if crate::media::open_external(&url).is_err() {
                state
                    .write()
                    .show_toast("This folder link could not be opened.");
            }
        }
        Open::File(file) => {
            let Some(opening) = prepare_file(state, &owner, &folder, serial) else {
                return;
            };
            dioxus::core::spawn_forever(async move {
                read_file(state, owner, folder, serial, opening, file).await;
            });
        }
        Open::Unavailable => {}
    }
}

// A server wait outside the bounded timer range stays fenced; it is never
// shortened into an earlier retry or converted into an overflowing deadline.
pub(super) fn deadline(now: Instant, seconds: Option<u64>) -> Option<Instant> {
    let seconds = seconds.unwrap_or(30);
    if seconds > 366 * 24 * 60 * 60 {
        return None;
    }
    now.checked_add(std::time::Duration::from_secs(seconds))
}
fn open_current(
    shell: &ShellState,
    owner: &Owner,
    folder: &Folder,
    serial: u64,
    opening: Option<u64>,
) -> bool {
    active(shell, owner, folder, serial)
        && opening.is_none_or(|id| {
            shell
                .channel_tabs
                .entries
                .get(&owner.key)
                .is_some_and(|e| e.folder.opening == Some(id))
        })
}
fn failure(
    mut state: Signal<ShellState>,
    owner: &Owner,
    folder: &Folder,
    serial: u64,
    opening: Option<u64>,
    error: Error,
) {
    let mut shell = state.write();
    if !open_current(&shell, owner, folder, serial, opening) {
        return;
    }
    let e = shell.channel_tabs.entries.get_mut(&owner.key).unwrap();
    if opening.is_some() {
        e.folder.opening = None;
    }
    let Error::RateLimited { retry_after_secs } = error else {
        e.folder.notice = if opening.is_some() {
            "This file could not open. Refresh the folder or try opening it again."
        } else {
            "The folder could not refresh. Return to its current tab and try again."
        }
        .into();
        return;
    };
    let at = deadline(Instant::now(), retry_after_secs);
    e.folder.retry_at = at;
    e.folder.cooling = true;
    e.folder.notice = if at.is_some() {
        "Folder requests are rate limited. Retry after the wait."
    } else {
        "The server requested a wait this client cannot schedule. Folder requests remain paused."
    }
    .into();
    drop(shell);
    let Some(at) = at else {
        return;
    };
    let key = owner.key.clone();
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await;
        if let Some(e) = state
            .write()
            .channel_tabs
            .entries
            .get_mut(&key)
            .filter(|e| e.folder.serial == serial && e.folder.retry_at == Some(at))
        {
            e.folder.cooling = false;
            e.folder.retry_at = None;
        }
    });
}
pub(super) fn prepare_file(
    mut state: Signal<ShellState>,
    owner: &Owner,
    folder: &Folder,
    serial: u64,
) -> Option<u64> {
    let mut shell = state.write();
    if !active(&shell, owner, folder, serial)
        || shell
            .channel_tabs
            .entries
            .get(&owner.key)
            .is_none_or(|e| e.folder.loading || e.folder.cooling || e.folder.opening.is_some())
    {
        return None;
    }
    shell.channel_tabs.next = shell.channel_tabs.next.wrapping_add(1);
    let opening = shell.channel_tabs.next;
    shell
        .channel_tabs
        .entries
        .get_mut(&owner.key)
        .unwrap()
        .folder
        .opening = Some(opening);
    Some(opening)
}
// Retire only our transient busy token, including when an overlay hides the
// route. This is not permission to adopt a response or modify newer UI state.
fn retire_file(mut state: Signal<ShellState>, owner: &Owner, serial: u64, opening: u64) {
    if let Some(e) = state
        .write()
        .channel_tabs
        .entries
        .get_mut(&owner.key)
        .filter(|e| e.folder.serial == serial && e.folder.opening == Some(opening))
    {
        e.folder.opening = None;
    }
}
pub(super) async fn read_file(
    mut state: Signal<ShellState>,
    owner: Owner,
    folder: Folder,
    serial: u64,
    opening: u64,
    file: String,
) {
    if !open_current(&state.peek(), &owner, &folder, serial, Some(opening)) {
        retire_file(state, &owner, serial, opening);
        return;
    }
    let result = owner
        .execute(api::file_info(&owner.client, &owner.session, &file))
        .await;
    if !open_current(&state.peek(), &owner, &folder, serial, Some(opening)) {
        retire_file(state, &owner, serial, opening);
        return;
    }
    let result = result.and_then(|raw| {
        run::envelope(&raw)?;
        if raw["file"]["id"].as_str() != Some(file.as_str()) {
            return Err(Error::Api("folder_file_changed".into()));
        }
        serde_json::from_value::<super_platinum_core::slack::models::File>(raw["file"].clone())
            .map_err(|_| Error::Api("folder_file_unavailable".into()))
    });
    match result {
        Ok(file) => {
            state
                .write()
                .channel_tabs
                .entries
                .get_mut(&owner.key)
                .unwrap()
                .folder
                .opening = None;
            crate::files::open(state, file);
        }
        Err(error) => failure(state, &owner, &folder, serial, Some(opening), error),
    }
}

#[component]
pub(crate) fn Panel(state: Signal<ShellState>) -> Element {
    let mut page = use_signal(|| 0usize);
    let snapshot = state.read();
    let visit = Visit::capture(&snapshot);
    let folder = selected(&snapshot);
    let e = entry(&snapshot);
    let title = folder
        .as_ref()
        .and_then(|f| {
            e.and_then(|e| e.inventory.as_ref())
                .and_then(|base| base.row(f.tab()))
        })
        .map(super_platinum_core::channel_tabs::label)
        .filter(|s| !s.is_empty())
        .unwrap_or("Bookmarks");
    let contents = e
        .and_then(|e| e.folder.contents.as_ref())
        .filter(|c| Some(c.folder()) == folder.as_ref());
    let loading = e.is_some_and(|e| e.folder.loading);
    let cooling = e.is_some_and(|e| e.folder.cooling);
    let serial = e.map(|e| e.folder.serial).unwrap_or(0);
    let enabled = visit.as_ref().is_some_and(|v| v.current(&snapshot))
        && snapshot.conversation_visible()
        && snapshot.overlay.is_none()
        && !loading
        && !cooling;
    let refresh_visit = visit.clone();
    let refresh_enabled = visit.as_ref().is_some_and(|v| v.current(&snapshot))
        && snapshot.conversation_visible()
        && snapshot.overlay.is_none()
        && !cooling;
    let last_page = contents
        .map(|c| c.items().len().saturating_sub(1) / 64)
        .unwrap_or(0);
    let shown = page().min(last_page);
    rsx! {
        section{class:"bookmarks-panel",id:"channel-folder-panel","aria-label":"Channel folder",
            header{class:"files-header",h2{"{title}"}span{class:"spacer"}
                button{class:"later-refresh",disabled:!refresh_enabled,"aria-busy":loading,onclick:move |_|if refresh_visit.as_ref().is_some_and(|v|v.current(&state.peek())){load(state)},"Refresh"}
            }
            div{class:"bookmarks-content","aria-busy":loading,
                if folder.is_none(){p{role:"status","This folder tab changed. Choose a current tab."}}
                if loading{p{class:"files-status",role:"status","Loading folder..."}}
                if let Some(e)=e {if !e.folder.notice.is_empty(){p{class:"files-status",role:"status","{e.folder.notice}"}}}
                if let Some(contents)=contents {
                    if contents.items().is_empty(){div{class:"files-empty",h3{"This folder is empty"}}}
                    div{role:"list","aria-label":"Folder items",
                        for item in contents.items().iter().skip(shown*64).take(64){
                            {let item=item.clone();let folder=contents.folder().clone();let visit=visit.clone();let bookmark=item.bookmark.clone();let disabled=!enabled||item.open==Open::Unavailable;rsx!{
                                div{class:"bookmark-row",role:"listitem",key:"{bookmark.id}",
                                    crate::bookmarks::row::OpenRow{state,bookmark,disabled,onopen:move |_|if let Some(v)=visit.as_ref(){open_item(state,v,folder.clone(),serial,item.clone())}}
                                }
                            }}
                        }
                    }
                    if last_page>0 {
                        div{class:"scheduled-actions",
                            button{r#type:"button","aria-disabled":shown==0,onclick:move |_|page.set(shown.saturating_sub(1)),"Previous items"}
                            span{"Page {shown+1} of {last_page+1}"}
                            button{r#type:"button","aria-disabled":shown==last_page,onclick:move |_|page.set((shown+1).min(last_page)),"Next items"}
                        }
                    }
                }
            }
        }
    }
}
