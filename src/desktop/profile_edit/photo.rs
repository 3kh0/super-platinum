//! Photo pane shares the existing profile editor's captured owner and native visit.
use super::*;
use crate::media::{FreshOriginal, MediaUri};
use super_platinum_core::{MediaAssetKind, slack::api::photo as protocol};
pub(crate) mod fixture;
mod network;
#[cfg(test)]
mod tests;
#[derive(Clone, Default)]
pub(crate) struct State {
    draft: Option<PhotoDraft>,
    entries: BTreeMap<Owner, PhotoEntry>,
}
#[derive(Clone)]
struct Preview {
    image: protocol::Image,
    asset: Arc<FreshOriginal>,
    prepared: Option<protocol::PreparedPhoto>,
    decoded: bool,
    crop: protocol::SquareCrop,
}
#[derive(Clone)]
struct PhotoDraft {
    token: Token,
    visit: u64,
    serial: u64,
    cap: Option<protocol::Capability>,
    observed: Option<protocol::Photo>,
    preview: Option<Preview>,
    remove: bool,
    loading: bool,
    message: Option<String>,
}
#[derive(Clone)]
struct PhotoAttempt {
    id: u64,
    token: Token,
    baseline: protocol::Photo,
    observed: Option<protocol::Photo>,
    remove: bool,
    phase: Phase,
    ack: bool,
    read: Option<u64>,
}
#[derive(Clone, Default)]
struct PhotoEntry {
    pending: Option<PhotoAttempt>,
    retry_at: i64,
    checked: Option<protocol::Photo>,
    message: Option<String>,
}
pub(crate) struct View {
    pub visit: u64,
    pub serial: u64,
    pub busy: bool,
    pub message: Option<String>,
    pub url: Option<String>,
    pub crop: Option<protocol::SquareCrop>,
    pub dimensions: Option<(u32, u32)>,
    pub image_key: String,
    pub can_choose: bool,
    pub can_prepare: bool,
    pub can_crop: bool,
    pub crop_controls: bool,
    pub can_save: bool,
    pub can_remove: bool,
    pub remove: bool,
    pub uncertain: bool,
    pub can_check: bool,
    pub can_keep: bool,
}
fn local_photo(shell: &ShellState, owner: &Owner) -> Option<protocol::Photo> {
    let p = shell
        .core
        .workspaces
        .get(&owner.team)?
        .users
        .get(&owner.user)?
        .profile
        .as_ref()?;
    protocol::Photo::from_profile(&serde_json::to_value(p).ok()?).ok()
}
fn current(shell: &ShellState, token: &Token, visit: u64, serial: u64) -> bool {
    draft_current(shell, token, visit)
        && shell
            .profile_edit
            .photo
            .draft
            .as_ref()
            .is_some_and(|d| d.token == *token && d.serial == serial)
}
pub(crate) fn pending(shell: &ShellState) -> bool {
    Owner::capture(shell)
        .and_then(|o| shell.profile_edit.photo.entries.get(&o))
        .is_some_and(|e| e.pending.is_some())
}
pub(crate) fn presentation(shell: &ShellState) -> Option<View> {
    let d = shell
        .profile_edit
        .photo
        .draft
        .as_ref()
        .filter(|d| current(shell, &d.token, d.visit, d.serial))?;
    let entry = shell.profile_edit.photo.entries.get(&d.token.owner);
    let p = entry.and_then(|e| e.pending.as_ref());
    let busy = d.loading || p.is_some_and(|p| p.phase != Phase::Uncertain);
    let uncertain = p.is_some();
    let cooling = entry.is_some_and(|e| e.retry_at > now());
    let conflict = sections::pending(shell)
        || crate::self_status::blocks_profile_edit(shell)
        || crate::profile_title::blocks_profile_writes(shell)
        || shell
            .profile_edit
            .entries
            .get(&d.token.owner)
            .is_some_and(|e| e.pending.is_some());
    let allowed = d.cap.as_ref().is_some_and(|c| c.permission.allowed())
        && !busy
        && !uncertain
        && !cooling
        && !conflict;
    let preview = d.preview.as_ref();
    let local = local_photo(shell, &d.token.owner);
    let url = preview
        .map(|p| p.asset.id().webview_uri_at(shell.media_epoch))
        .or_else(|| {
            let photo = if local != d.observed {
                local.as_ref()
            } else {
                entry
                    .and_then(|e| e.checked.as_ref())
                    .or_else(|| d.cap.as_ref().map(|c| &c.photo))
            }?;
            let id = shell
                .media
                .register_image(MediaAssetKind::Avatar, photo.url(), "image/png");
            Some(id.webview_uri_at(shell.media_epoch))
        });
    let message = entry
        .and_then(|e| e.message.clone())
        .or_else(|| d.message.clone())
        .or_else(|| {
            d.cap
                .as_ref()
                .and_then(|c| c.permission.reason())
                .map(str::to_owned)
        });
    Some(View {
        image_key: preview
            .map(|p| p.asset.id().opaque().to_owned())
            .unwrap_or_default(),
        visit: d.visit,
        serial: d.serial,
        busy,
        message,
        url,
        crop: preview.map(|p| p.crop),
        dimensions: preview.map(|p| (p.image.width(), p.image.height())),
        can_choose: allowed,
        can_prepare: allowed && preview.is_some_and(|p| p.decoded && p.prepared.is_none()),
        can_crop: allowed && preview.is_some_and(|p| p.decoded && p.prepared.is_some()),
        crop_controls: preview.is_some_and(|p| p.prepared.is_some()),
        can_save: allowed
            && (d.remove || preview.is_some_and(|p| p.decoded && p.prepared.is_some())),
        can_remove: allowed && d.cap.as_ref().is_some_and(|c| c.photo.custom()),
        remove: d.remove,
        uncertain,
        can_check: p.is_some_and(|p| p.phase == Phase::Uncertain) && !cooling,
        can_keep: !busy
            && entry.is_some_and(|e| {
                e.checked
                    .as_ref()
                    .is_some_and(|p| local.as_ref() == Some(p))
            }),
    })
}
pub(crate) fn open(mut state: Signal<ShellState>, visit: u64) {
    let load = open_shell(&mut state.write(), visit);
    if let Some((token, serial)) = load {
        dioxus::core::spawn_forever(network::load(state, token, visit, serial));
    }
}
/// Returns a load only after the same admission used by the actual open action.
fn open_shell(shell: &mut ShellState, visit: u64) -> Option<(Token, u64)> {
    if sections::active(shell) || sections::pending(shell) {
        return None;
    }
    let capture = {
        let Some(d) = shell
            .profile_edit
            .draft
            .as_ref()
            .filter(|d| d.visit == visit && draft_current(&shell, &d.token, visit))
        else {
            return None;
        };
        if shell
            .profile_edit
            .entries
            .get(&d.token.owner)
            .is_some_and(|e| e.pending.is_some())
        {
            return None;
        }
        if shell.profile_edit.photo.entries.len() >= 16
            && !shell
                .profile_edit
                .photo
                .entries
                .contains_key(&d.token.owner)
        {
            return None;
        }
        (d.token.clone(), local_photo(&shell, &d.token.owner))
    };
    let (token, observed) = capture;
    let serial = next_id();
    let cooling = shell
        .profile_edit
        .photo
        .entries
        .get(&token.owner)
        .is_some_and(|e| e.retry_at > now());
    let retained = shell
        .profile_edit
        .photo
        .entries
        .get(&token.owner)
        .is_some_and(|e| e.pending.is_some());
    shell.profile_edit.photo.draft = Some(PhotoDraft {
        token: token.clone(),
        visit,
        serial,
        cap: None,
        observed,
        preview: None,
        remove: false,
        loading: !retained && !cooling,
        message: cooling
            .then(|| "Slack asked us to wait before checking photo permissions again.".into()),
    });
    (!retained && !cooling).then_some((token, serial))
}
pub(crate) fn cancel(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let mut shell = state.write();
    if !presentation(&shell).is_some_and(|v| v.visit == visit && v.serial == serial && !v.busy) {
        return;
    }
    shell.profile_edit.photo.draft = None;
}
pub(super) fn intercept_close(shell: &mut ShellState, visit: Option<u64>) -> bool {
    let Some(d) = shell.profile_edit.photo.draft.as_ref() else {
        return false;
    };
    if visit.is_some_and(|v| v != d.visit) {
        return true;
    }
    if presentation(shell).is_some_and(|v| !v.busy) {
        shell.profile_edit.photo.draft = None;
    }
    true
}
pub(super) fn poll(shell: &mut ShellState) -> bool {
    let stale = shell
        .profile_edit
        .photo
        .draft
        .as_ref()
        .is_some_and(|d| !current(shell, &d.token, d.visit, d.serial));
    if stale {
        shell.profile_edit.photo.draft = None;
    }
    let visible = shell
        .profile_edit
        .photo
        .draft
        .as_ref()
        .map(|d| d.token.owner.clone());
    let mut changed = stale;
    for e in shell.profile_edit.photo.entries.values_mut() {
        if e.retry_at > 0 && e.retry_at <= now() {
            e.retry_at = 0;
            changed = true;
        }
    }
    shell
        .profile_edit
        .photo
        .entries
        .retain(|o, e| Some(o) == visible.as_ref() || e.pending.is_some() || e.retry_at > now());
    changed
}
pub(crate) fn decoded(mut state: Signal<ShellState>, visit: u64, serial: u64, key: &str, ok: bool) {
    let mut s = state.write();
    let Some(d) = s
        .profile_edit
        .photo
        .draft
        .as_mut()
        .filter(|d| d.visit == visit && d.serial == serial)
    else {
        return;
    };
    if let Some(p) = d.preview.as_mut().filter(|p| p.asset.id().opaque() == key) {
        p.decoded = ok;
    } else {
        return;
    }
    if !ok {
        d.message =
            Some("This photo could not be displayed. Choose a GIF, JPG or PNG image.".into());
    }
}
pub(crate) fn crop(
    mut state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    axis: u8,
    value: String,
) {
    let mut shell = state.write();
    if !presentation(&shell).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_crop) {
        return;
    }
    let Some(p) = shell
        .profile_edit
        .photo
        .draft
        .as_mut()
        .and_then(|d| d.preview.as_mut())
    else {
        return;
    };
    let Ok(v) = value.parse::<f64>() else {
        return;
    };
    let old = p.crop;
    if let Some(c) = protocol::SquareCrop::normalize(
        if axis == 0 { v } else { old.x() as f64 },
        if axis == 1 { v } else { old.y() as f64 },
        if axis == 2 { v } else { old.size() as f64 },
        p.image.width(),
        p.image.height(),
    ) {
        p.crop = c;
    }
}
pub(crate) fn remove(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let mut shell = state.write();
    if !presentation(&shell).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_remove)
    {
        return;
    }
    let d = shell.profile_edit.photo.draft.as_mut().unwrap();
    d.remove = true;
    d.preview = None;
    d.message =
        Some("Remove your profile photo? Slack will replace it with its default avatar.".into());
}
pub(crate) fn keep(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let mut s = state.write();
    if !presentation(&s).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_keep) {
        return;
    }
    let owner = s
        .profile_edit
        .photo
        .draft
        .as_ref()
        .unwrap()
        .token
        .owner
        .clone();
    let e = s.profile_edit.photo.entries.get_mut(&owner).unwrap();
    e.pending = None;
    e.message = None;
    s.profile_edit.photo.draft = None;
}
pub(crate) fn choose(state: Signal<ShellState>, visit: u64, serial: u64) {
    dioxus::core::spawn_forever(network::choose(state, visit, serial));
}
pub(crate) fn prepare(state: Signal<ShellState>, visit: u64, serial: u64) {
    dioxus::core::spawn_forever(network::prepare(state, visit, serial));
}
pub(crate) fn save(state: Signal<ShellState>, visit: u64, serial: u64) {
    dioxus::core::spawn_forever(network::save(state, visit, serial));
}
pub(crate) fn check(state: Signal<ShellState>, visit: u64, serial: u64) {
    dioxus::core::spawn_forever(network::check(state, visit, serial));
}
