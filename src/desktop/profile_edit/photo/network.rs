use super::*;
use tokio::io::AsyncReadExt;

pub(super) async fn load(mut state: Signal<ShellState>, token: Token, visit: u64, serial: u64) {
    if !current(&state.peek(), &token, visit, serial) {
        return;
    }
    let ctx = context(&state.peek(), &token);
    let result = match ctx {
        Some(c) => protocol::fetch_capability(&c.transport, &c.client, &c.session).await,
        None => Err(Error::TransportNotConfigured),
    };
    let mut shell = state.write();
    if !current(&shell, &token, visit, serial) {
        return;
    }
    let d = shell.profile_edit.photo.draft.as_mut().unwrap();
    d.loading = false;
    match result {
        Ok(cap) => {
            d.cap = Some(cap);
            d.message = None;
        }
        Err(e) => {
            d.message = Some(
                "Could not verify photo permissions. Cancel and open the photo editor again."
                    .into(),
            );
            shell
                .profile_edit
                .photo
                .entries
                .entry(token.owner)
                .or_default()
                .retry_at = cooldown(&e);
        }
    }
}
pub(super) async fn choose(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let token = {
        let shell = state.peek();
        if !presentation(&shell)
            .is_some_and(|v| v.visit == visit && v.serial == serial && v.can_choose)
        {
            return;
        }
        shell
            .profile_edit
            .photo
            .draft
            .as_ref()
            .unwrap()
            .token
            .clone()
    };
    state
        .write()
        .profile_edit
        .photo
        .draft
        .as_mut()
        .unwrap()
        .loading = true;
    let file = rfd::AsyncFileDialog::new()
        .add_filter("Profile photos", &["png", "jpg", "jpeg", "gif"])
        .pick_file()
        .await;
    if !current(&state.peek(), &token, visit, serial) {
        return;
    }
    let Some(file) = file else {
        state
            .write()
            .profile_edit
            .photo
            .draft
            .as_mut()
            .unwrap()
            .loading = false;
        return;
    };
    let path = file.path().to_owned();
    let result = async {
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|_| Error::Api("photo_file_unreadable".into()))?;
        let mut bytes = Vec::new();
        file.take((protocol::MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| Error::Api("photo_file_unreadable".into()))?;
        tokio::task::spawn_blocking(move || protocol::Image::parse(bytes))
            .await
            .map_err(|_| Error::Api("photo_file_unreadable".into()))?
    }
    .await;
    let mut shell = state.write();
    if !current(&shell, &token, visit, serial) {
        return;
    }
    match result {
        Ok(image) => install_preview(&mut shell, image, None),
        Err(error) => {
            let d = shell.profile_edit.photo.draft.as_mut().unwrap();
            d.loading = false;
            d.message = Some(
                if matches!(&error, Error::Api(code) if code == "photo_file_too_large") {
                    "This file exceeds the app's 256 MiB preview capacity. Choose a smaller export of the image.".into()
                } else {
                    "Choose a GIF, JPG or PNG, 36–4000 pixels on each side.".into()
                },
            );
        }
    }
}
pub(super) fn install_preview(
    shell: &mut ShellState,
    image: protocol::Image,
    prepared: Option<protocol::PreparedPhoto>,
) {
    let asset = Arc::new(shell.media.insert_fresh_original(
        "profile-photo:local",
        "image/png",
        image.preview_bytes(),
    ));
    let size = image.width().min(image.height());
    let crop = protocol::SquareCrop::normalize(
        ((image.width() - size) / 2) as f64,
        ((image.height() - size) / 2) as f64,
        size as f64,
        image.width(),
        image.height(),
    )
    .unwrap();
    let d = shell.profile_edit.photo.draft.as_mut().unwrap();
    d.preview = Some(Preview {
        image,
        asset,
        prepared,
        decoded: false,
        crop,
    });
    d.loading = false;
    d.remove = false;
    d.message = None;
}
pub(super) async fn prepare(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let (token, image, baseline, observed) = {
        let s = state.peek();
        if !presentation(&s)
            .is_some_and(|v| v.visit == visit && v.serial == serial && v.can_prepare)
        {
            return;
        }
        let d = s.profile_edit.photo.draft.as_ref().unwrap();
        (
            d.token.clone(),
            d.preview.as_ref().unwrap().image.clone(),
            d.cap.as_ref().unwrap().photo.clone(),
            d.observed.clone(),
        )
    };
    let Some(ctx) = context(&state.peek(), &token) else {
        return;
    };
    state
        .write()
        .profile_edit
        .photo
        .draft
        .as_mut()
        .unwrap()
        .loading = true;
    let result = async {
        let fresh = protocol::fetch_capability(&ctx.transport, &ctx.client, &ctx.session).await?;
        if !current(&state.peek(), &token, visit, serial)
            || fresh.photo != baseline
            || !fresh.permission.allowed()
            || local_photo(&state.peek(), &token.owner) != observed
        {
            return Err(Error::Api("photo_owner_or_baseline_changed".into()));
        }
        let prepared = ctx
            .transport
            .prepare_own_photo(protocol::prepare(&ctx.client, &ctx.session, image))
            .await?;
        if !current(&state.peek(), &token, visit, serial) {
            return Err(Error::Api("photo_owner_changed".into()));
        }
        // Crop the actual prepared image returned by Slack, not an assumed
        // original size. The closed transport shares the local preview capacity
        // and refuses redirects, without exposing a generic download authority.
        let bytes = ctx.transport.read_prepared_own_photo(&prepared).await?;
        let image = tokio::task::spawn_blocking(move || protocol::Image::parse(bytes))
            .await
            .map_err(|_| Error::Api("photo_prepared_image_unreadable".into()))??;
        Ok((image, prepared))
    }
    .await;
    let mut s = state.write();
    if !current(&s, &token, visit, serial) {
        return;
    }
    match result {
        Ok((image, prepared)) => install_preview(&mut s, image, Some(prepared)),
        Err(e) => {
            let d = s.profile_edit.photo.draft.as_mut().unwrap();
            d.loading = false;
            d.message = Some(
                "Could not prepare this photo. Your current profile photo has not been changed."
                    .into(),
            );
            s.profile_edit
                .photo
                .entries
                .entry(token.owner)
                .or_default()
                .retry_at = cooldown(&e);
        }
    }
}
pub(super) async fn save(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let (attempt, prepared) = {
        let mut s = state.write();
        if !presentation(&s).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_save) {
            return;
        }
        let d = s.profile_edit.photo.draft.as_ref().unwrap();
        let a = PhotoAttempt {
            id: next_id(),
            token: d.token.clone(),
            baseline: d.cap.as_ref().unwrap().photo.clone(),
            observed: d.observed.clone(),
            remove: d.remove,
            phase: Phase::Preparing,
            ack: false,
            read: None,
        };
        let p = d
            .preview
            .as_ref()
            .and_then(|p| p.prepared.clone().map(|prepared| (prepared, p.crop)));
        let e = s
            .profile_edit
            .photo
            .entries
            .entry(a.token.owner.clone())
            .or_default();
        e.pending = Some(a.clone());
        e.checked = None;
        e.message = None;
        (a, p)
    };
    let Some(ctx) = context(&state.peek(), &attempt.token) else {
        preflight_failed(&mut state.write(), &attempt, None);
        return;
    };
    let fresh = protocol::fetch_capability(&ctx.transport, &ctx.client, &ctx.session).await;
    let allowed = {
        let s = state.peek();
        current(&s, &attempt.token, visit, serial)
            && !crate::self_status::blocks_profile_edit(&s)
            && !crate::profile_title::blocks_profile_writes(&s)
            && local_photo(&s, &attempt.token.owner) == attempt.observed
            && fresh.as_ref().is_ok_and(|c| {
                c.team == attempt.token.owner.team
                    && c.user == attempt.token.owner.user
                    && c.permission.allowed()
                    && c.photo == attempt.baseline
            })
    };
    if !allowed {
        preflight_failed(&mut state.write(), &attempt, fresh.as_ref().err());
        return;
    }
    {
        let mut s = state.write();
        let Some(e) = entry(&mut s, &attempt) else {
            return;
        };
        if e.pending.as_ref().unwrap().phase != Phase::Preparing {
            return;
        }
        e.pending.as_mut().unwrap().phase = Phase::Saving;
    }
    let mutation = if attempt.remove {
        protocol::remove(&ctx.client, &ctx.session)
    } else {
        let Some((prepared, crop)) = prepared else {
            preflight_failed(&mut state.write(), &attempt, None);
            return;
        };
        protocol::set(&ctx.client, &ctx.session, &prepared, crop)
    };
    let result = ctx.transport.mutate_own_photo(mutation).await;
    let check_now = {
        let mut s = state.write();
        let current = current(&s, &attempt.token, visit, serial);
        let Some(e) = entry(&mut s, &attempt) else {
            return;
        };
        if e.pending.as_ref().unwrap().phase != Phase::Saving {
            return;
        }
        match result {
            Ok(()) => {
                let p = e.pending.as_mut().unwrap();
                p.ack = true;
                p.phase = Phase::Uncertain;
                e.message =
                    Some("Slack accepted the photo change. Checking your current photo…".into());
                current
            }
            Err(error) => {
                e.retry_at = cooldown(&error);
                if uncertain(&error) {
                    e.pending.as_mut().unwrap().phase = Phase::Uncertain;
                    e.message=Some("Slack has not confirmed this photo change. Check your current photo before trying again.".into());
                } else {
                    e.pending = None;
                    e.message = Some(
                        "Slack did not accept the photo change. Your preview is retained.".into(),
                    );
                }
                false
            }
        }
    };
    if check_now {
        check(state, visit, serial).await;
    }
}
fn entry<'a>(s: &'a mut ShellState, a: &PhotoAttempt) -> Option<&'a mut PhotoEntry> {
    s.profile_edit
        .photo
        .entries
        .get_mut(&a.token.owner)
        .filter(|e| e.pending.as_ref().is_some_and(|p| p.id == a.id))
}
fn preflight_failed(s: &mut ShellState, a: &PhotoAttempt, error: Option<&Error>) {
    if let Some(e) = entry(s, a) {
        if e.pending.as_ref().unwrap().phase != Phase::Preparing {
            return;
        }
        e.pending = None;
        e.retry_at = error.map_or(0, cooldown);
        e.message=Some("Photo or permissions changed. No photo update was sent; cancel and review the current photo.".into());
    }
}
pub(super) async fn check(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let (token, id, read) = {
        let mut s = state.write();
        if !presentation(&s).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_check)
        {
            return;
        }
        let token = s.profile_edit.photo.draft.as_ref().unwrap().token.clone();
        let read = next_id();
        let e = s.profile_edit.photo.entries.get_mut(&token.owner).unwrap();
        let p = e.pending.as_mut().unwrap();
        p.phase = Phase::Checking;
        p.read = Some(read);
        e.checked = None;
        (token, p.id, read)
    };
    let ctx = context(&state.peek(), &token);
    let result = if let Some(ctx) = ctx {
        protocol::read(&ctx.transport, &ctx.client, &ctx.session).await
    } else {
        Err(Error::TransportNotConfigured)
    };
    let before = local_photo(&state.peek(), &token.owner);
    finish_check(&mut state.write(), &token, visit, serial, id, read, result);
    let adopted =
        token.current(&state.peek()) && local_photo(&state.peek(), &token.owner) != before;
    if adopted {
        crate::bootstrap::persist_workspace(&state, &token.owner.team);
    }
}
pub(super) fn finish_check(
    s: &mut ShellState,
    token: &Token,
    visit: u64,
    serial: u64,
    id: u64,
    read: u64,
    result: Result<protocol::Photo, Error>,
) {
    let current = current(s, token, visit, serial);
    let observed = local_photo(s, &token.owner);
    let Some(e) = s.profile_edit.photo.entries.get_mut(&token.owner) else {
        return;
    };
    let Some(a) = e
        .pending
        .as_ref()
        .filter(|p| p.id == id && p.phase == Phase::Checking && p.read == Some(read))
        .cloned()
    else {
        return;
    };
    e.pending.as_mut().unwrap().phase = Phase::Uncertain;
    e.pending.as_mut().unwrap().read = None;
    if !current {
        return;
    }
    match result {
        Ok(photo) => {
            if observed != a.observed && observed.as_ref() != Some(&photo) {
                e.checked = None;
                e.message = Some("Your photo changed again while this check was running. Check again before resolving the earlier change.".into());
                if let Some(d) = s.profile_edit.photo.draft.as_mut() {
                    d.preview = None;
                    d.remove = false;
                    if let Some(newer) = observed.clone() {
                        if let Some(cap) = d.cap.as_mut() {
                            cap.photo = newer;
                        }
                    } else {
                        d.cap = None;
                    }
                    d.observed = observed;
                }
                return;
            }
            let accepted = a.ack
                && if a.remove {
                    !photo.custom()
                } else {
                    photo.custom() && photo != a.baseline
                };
            e.checked = Some(photo.clone());
            e.message=Some(if accepted{"Photo change accepted; current photo loaded."}else{"This is your current photo. The earlier change is still unconfirmed. Check again, or keep this photo without sending another change."}.into());
            if accepted {
                e.pending = None;
            }
            // Do not overwrite a newer realtime photo; no unrelated profile fields
            // are copied from this read. Unknown outcomes remain explicitly retained.
            if observed == a.observed || observed.as_ref() == Some(&photo) {
                if let Some(user) = s
                    .core
                    .workspaces
                    .get_mut(&token.owner.team)
                    .and_then(|w| w.users.get_mut(&token.owner.user))
                {
                    photo.adopt(user.profile.get_or_insert_with(Default::default));
                }
            }
            let next_observed = local_photo(s, &token.owner);
            if let Some(d) = s.profile_edit.photo.draft.as_mut() {
                d.preview = None;
                d.remove = false;
                if accepted {
                    d.cap.as_mut().map(|c| c.photo = photo);
                    d.observed = next_observed;
                }
            }
        }
        Err(error) => {
            e.retry_at = cooldown(&error);
            e.message=Some("Could not read your current photo. The earlier result is still retained; no change was sent again.".into());
        }
    }
}
