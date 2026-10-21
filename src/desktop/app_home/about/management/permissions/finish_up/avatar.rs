//! Selected local bytes and server avatar receipts belong to this modal only.
use super::*;
use super_platinum_core::slack::api::avatar as api;
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Pane {
    #[default]
    Details,
    Upload,
    Crop,
}
impl Pane {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Details => "details",
            Self::Upload => "upload",
            Self::Crop => "crop",
        }
    }
}
#[derive(Default)]
pub(super) struct State {
    pub pane: Pane,
    pub pane_bound: bool,
    pub pending: bool,
    pub crop_bound: bool,
    pub cropper: cropper::State,
    generation: u64,
    pub upload: Option<(String, String, u32, u32)>,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub receipts: Vec<Value>,
    pub notice: Option<String>,
}
impl State {
    fn discard_crop(&mut self) {
        self.upload = None;
        self.notice = None;
        self.pane = Pane::Details;
        self.pane_bound = false;
        self.crop_bound = false;
        self.cropper = cropper::State::default();
        self.pending = false;
        self.generation = next();
    }
    pub(super) fn status(&self) -> &'static str {
        match (self.pane, self.pending, self.notice.is_some()) {
            (Pane::Details, _, _) => "idle",
            (Pane::Upload, true, _) => "uploading",
            (Pane::Crop, true, _) => "saving",
            (_, _, true) => "error",
            _ => "ready",
        }
    }
    pub(super) fn value(&self) -> Value {
        json!({"pane":self.pane.name(),"status":self.status(),"paneBound":self.pane_bound,"pending":self.pending,"cropBound":self.crop_bound,"generation":self.generation,"uploaded":self.upload.as_ref().map(|(id,url,w,h)|json!({"id":id,"url":url,"width":w,"height":h})),"crop":{"x":self.x,"y":self.y,"size":self.size},"cropper":self.cropper.value(self.upload.as_ref().map(|u|u.1.as_str())),"receipts":self.receipts,"notice":self.notice})
    }
}
fn current(s: &ShellState, o: &Original, g: u64) -> bool {
    local(s, o.visit, o.id).is_some_and(|(_, l)| l.avatar.generation == g && l.avatar.pending)
}
pub(super) fn choose(mut state: Signal<ShellState>, visit: u64, id: u64, path: std::path::PathBuf) {
    let captured = {
        let s = state.peek();
        local(&s, visit, id)
            .filter(|(_, l)| {
                l.avatar.pane == Pane::Details && l.avatar.pane_bound && !l.avatar.pending
            })
            .map(|(c, l)| (c.original.clone(), l.expected()))
    };
    let Some((o, e)) = captured else { return };
    // The pinned desktop interpreter names its DOM File with the full selected
    // path; only the multipart image filename is the basename below.
    let Some(selected_path) = path.to_str().map(str::to_owned) else {
        return;
    };
    let filename = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_owned();
    dioxus::core::spawn_forever(async move {
        if !native::current(
            &o,
            &e,
            Some(&json!({"kind":"upload","value":selected_path})),
        )
        .await
        {
            return;
        }
        let generation = next();
        let upload_expected = {
            let mut s = state.write();
            if !local(&s, visit, id).is_some_and(|(_, l)| l.expected() == e) {
                return;
            }
            let l = s
                .app_home
                .about
                .management
                .permissions
                .confirmation
                .as_mut()
                .unwrap()
                .finish_up
                .as_mut()
                .unwrap();
            l.avatar.pending = true;
            l.avatar.generation = generation;
            l.avatar.notice = None;
            l.avatar.pane = Pane::Upload;
            l.avatar.pane_bound = false;
            l.epoch = next();
            l.expected()
        };
        actions::bind(state, visit, id);
        let decoded = tokio::task::spawn_blocking(move || {
            let mime = match path
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                _ => return Err(unavailable()),
            };
            let bytes = std::fs::read(path).map_err(|_| unavailable())?;
            api::Image::parse(filename, mime, bytes)
        })
        .await;
        let result = async {
            let image = decoded.map_err(|_| unavailable())??;
            if !current(&state.peek(), &o, generation)
                || !native::bind(&o, &upload_expected).await
                || !current(&state.peek(), &o, generation)
                || !native::current(&o, &upload_expected, None).await
            {
                return Err(unavailable());
            }
            let (w, h) = (image.width(), image.height());
            let request = api::upload(&o.owner.ctx.client, &o.owner.ctx.session, image);
            let raw = send(state, &o, generation, &request).await?;
            if !current(&state.peek(), &o, generation) {
                return Err(unavailable());
            }
            {
                let mut s = state.write();
                let l = s
                    .app_home
                    .about
                    .management
                    .permissions
                    .confirmation
                    .as_mut()
                    .unwrap()
                    .finish_up
                    .as_mut()
                    .unwrap();
                l.avatar.receipts.push(json!({"kind":"upload","raw":raw}));
            }
            let reply = api::UploadReply::parse(&raw)?;
            Ok((reply.id().to_owned(), reply.url().to_owned(), w, h))
        }
        .await;
        let mut s = state.write();
        if !current(&s, &o, generation) {
            return;
        }
        let l = s
            .app_home
            .about
            .management
            .permissions
            .confirmation
            .as_mut()
            .unwrap()
            .finish_up
            .as_mut()
            .unwrap();
        l.avatar.pending = false;
        match result {
            Ok(value) => {
                l.avatar.x = 0.;
                l.avatar.y = 0.;
                l.avatar.size = value.2.min(value.3) as f64;
                l.avatar.upload = Some(value);
                l.avatar.crop_bound = false;
                l.avatar.cropper = cropper::State::default();
                l.avatar.pane = Pane::Crop;
            }
            Err(ref error) => {
                l.avatar.notice = Some(upload_error(error).into());
            }
        }
        l.avatar.pane_bound = false;
        l.epoch = next();
        drop(s);
        actions::bind(state, visit, id);
    });
}
async fn send(
    state: Signal<ShellState>,
    o: &Original,
    g: u64,
    request: &api::AvatarRequest,
) -> Result<Value, Error> {
    if !current(&state.peek(), o, g) {
        return Err(unavailable());
    }
    let result = if crate::fixture::is_fixture() {
        fixture::first_publish::avatar(state, request)
            .await
            .unwrap_or_else(|| Err(unavailable()))
    } else {
        o.owner
            .ctx
            .transport
            .execute_workflow_avatar_current(
                request,
                || current(&state.peek(), o, g),
                |receipt| {
                    let mut state = state;
                    state
                        .write()
                        .app_home
                        .about
                        .management
                        .permissions
                        .first_avatar_receipts
                        .push(json!({"id":o.id,"generation":g,"attempt":receipt}));
                },
            )
            .await
    };
    let mut state = state;
    state.write().app_home.about.management.permissions.first_avatar_receipts.push(json!({"id":o.id,"generation":g,"request":request.inspect(),"raw":result.as_ref().ok(),"failed":result.is_err()}));
    result
}
pub(super) fn back(mut state: Signal<ShellState>, visit: u64, id: u64, kind: &'static str) {
    let captured = {
        let s = state.peek();
        local(&s, visit, id)
            .filter(|(_, l)| {
                l.avatar.pane_bound
                    && match kind {
                        "upload-back" => l.avatar.pane == Pane::Upload,
                        "upload-retry" => {
                            l.avatar.pane == Pane::Upload
                                && !l.avatar.pending
                                && l.avatar.notice.is_some()
                        }
                        "crop-back" | "crop-cancel" => {
                            l.avatar.pane == Pane::Crop && l.avatar.crop_bound
                        }
                        _ => false,
                    }
            })
            .map(|(c, l)| (c.original.clone(), l.expected()))
    };
    let Some((o, e)) = captured else { return };
    dioxus::core::spawn_forever(async move {
        if !native::current(&o, &e, Some(&json!({"kind":kind}))).await {
            return;
        }
        let mut s = state.write();
        if !local(&s, visit, id).is_some_and(|(_, l)| l.expected() == e) {
            return;
        }
        let l = s
            .app_home
            .about
            .management
            .permissions
            .confirmation
            .as_mut()
            .unwrap()
            .finish_up
            .as_mut()
            .unwrap();
        // Back abandons only this avatar generation; an already sent server
        // operation is not claimed rolled back and its receipt remains retained.
        l.avatar.discard_crop();
        l.epoch = next();
        drop(s);
        actions::bind(state, visit, id);
    });
}
pub(super) fn edit(
    state: Signal<ShellState>,
    visit: u64,
    id: u64,
    key: &'static str,
    value: String,
) {
    cropper::numeric(state, visit, id, key, value);
}
pub(super) fn apply(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let captured = {
        let s = state.peek();
        local(&s, visit, id)
            .filter(|(_, l)| {
                l.avatar.pane == Pane::Crop
                    && l.avatar.pane_bound
                    && !l.avatar.pending
                    && l.avatar.crop_bound
                    && l.avatar.cropper.ready
                    && !l.avatar.cropper.interaction_pending
            })
            .and_then(|(c, l)| {
                l.avatar.upload.clone().map(|u| {
                    (
                        c.original.clone(),
                        l.expected(),
                        u,
                        l.avatar.x,
                        l.avatar.y,
                        l.avatar.size,
                    )
                })
            })
    };
    let Some((o, e, (avatar, _, w, h), x, y, size)) = captured else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !native::current(&o, &e, Some(&json!({"kind":"crop-apply"}))).await {
            return;
        }
        let Ok(crop) = api::Crop::new(x, y, size, w, h) else {
            return;
        };
        let Ok(request) = api::crop(&o.owner.ctx.client, &o.owner.ctx.session, &avatar, crop)
        else {
            return;
        };
        let g = next();
        {
            let mut s = state.write();
            if local(&s, visit, id).is_none() {
                return;
            }
            let l = s
                .app_home
                .about
                .management
                .permissions
                .confirmation
                .as_mut()
                .unwrap()
                .finish_up
                .as_mut()
                .unwrap();
            if !begin_crop(l, &e, g) {
                return;
            }
        }
        let result = send(state, &o, g, &request).await;
        let mut s = state.write();
        if !current(&s, &o, g) {
            return;
        }
        let l = s
            .app_home
            .about
            .management
            .permissions
            .confirmation
            .as_mut()
            .unwrap()
            .finish_up
            .as_mut()
            .unwrap();
        settle_crop(l, result);
        drop(s);
        actions::bind(state, visit, id);
    });
}

// Called only after the original owner and native proof, with no intervening
// await before send. A proof of old coordinates cannot start a new request.
fn begin_crop(l: &mut Local, expected: &Value, generation: u64) -> bool {
    if l.expected() != *expected
        || l.avatar.pane != Pane::Crop
        || !l.avatar.pane_bound
        || l.avatar.pending
        || !l.avatar.crop_bound
        || l.avatar.upload.is_none()
    {
        return false;
    }
    l.avatar.pending = true;
    l.avatar.generation = generation;
    l.avatar.notice = None;
    true
}

fn settle_crop(l: &mut Local, result: Result<Value, Error>) {
    l.avatar.pending = false;
    match result {
        Ok(raw) => {
            l.avatar.receipts.push(json!({"kind":"crop","raw":raw}));
            match api::CropReply::parse(&raw) {
                Ok(reply) => {
                    l.icon_url = reply.url().into();
                    l.avatar.discard_crop();
                    l.epoch = next();
                }
                Err(_) => {
                    l.avatar.notice = Some("Sorry! Something went wrong. Please try again.".into())
                }
            }
        }
        Err(_) => l.avatar.notice = Some("Sorry! Something went wrong. Please try again.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uploaded() -> Local {
        Local {
            title: "Local title".into(),
            description: "Local description".into(),
            icon_url: "https://example.test/original.png".into(),
            more: true,
            defaults: false,
            epoch: 17,
            notice: None,
            avatar: State {
                pane: Pane::Crop,
                pane_bound: true,
                crop_bound: true,
                upload: Some((
                    "AVATAR_ORIGINAL".into(),
                    "https://example.test/upload.png".into(),
                    256,
                    256,
                )),
                x: 64.,
                y: 64.,
                size: 128.,
                ..State::default()
            },
        }
    }

    #[test]
    fn leaving_either_pending_pane_retires_generation_and_retains_original_details_and_receipts() {
        for pane in [Pane::Upload, Pane::Crop] {
            let mut l = uploaded();
            l.avatar.pane = pane;
            l.avatar.pending = true;
            let generation = next();
            l.avatar.generation = generation;
            l.avatar
                .receipts
                .push(json!({"kind":"upload","raw":{"ok":true,"id":"original"}}));
            let receipts = l.avatar.receipts.clone();
            l.avatar.discard_crop();
            assert!(l.avatar.pane == Pane::Details);
            assert!(!l.avatar.pane_bound);
            assert!(!l.avatar.pending);
            assert_ne!(l.avatar.generation, generation);
            assert!(l.avatar.upload.is_none());
            assert_eq!(l.avatar.receipts, receipts);
            assert_eq!(l.icon_url, "https://example.test/original.png");
            assert_eq!(
                (&*l.title, &*l.description),
                ("Local title", "Local description")
            );
            assert!(!l.valid());
            l.avatar.pane_bound = true;
            assert!(l.valid());
        }
    }

    #[test]
    fn rejected_crop_retry_retains_raw_receipts_and_clears_only_stale_guidance() {
        let mut l = uploaded();
        let e = l.expected();
        assert!(begin_crop(&mut l, &e, 20));
        let rejected = json!({"ok":false,"error":"crop_rejected","opaque":[1,"original"]});
        settle_crop(&mut l, Ok(rejected.clone()));
        assert_eq!(
            l.avatar.receipts,
            vec![json!({"kind":"crop","raw":rejected})]
        );
        assert_eq!(
            l.avatar.notice.as_deref(),
            Some("Sorry! Something went wrong. Please try again.")
        );
        assert_eq!(l.icon_url, "https://example.test/original.png");
        assert_eq!(l.expected(), e);
        assert!(begin_crop(&mut l, &e, 21));
        assert!(l.avatar.notice.is_none());
        assert!(l.avatar.pending);
        assert!(!begin_crop(&mut l, &e, 22));
        let accepted = json!({"ok":true,"url":"https://example.test/cropped.png","opaque":"keep"});
        settle_crop(&mut l, Ok(accepted.clone()));
        assert_eq!(l.avatar.receipts[1], json!({"kind":"crop","raw":accepted}));
        assert_eq!(l.icon_url, "https://example.test/cropped.png");
        assert!(l.avatar.upload.is_none());
        assert!(l.avatar.notice.is_none());
        assert!(!l.avatar.pending);
        assert_eq!(
            (&*l.title, &*l.description),
            ("Local title", "Local description")
        );
    }

    #[test]
    fn crop_cancel_after_failure_retains_details_icon_and_all_receipts() {
        let mut l = uploaded();
        settle_crop(&mut l, Ok(json!({"ok":false,"error":"not_allowed"})));
        let receipts = l.avatar.receipts.clone();
        l.avatar.discard_crop();
        assert!(l.avatar.upload.is_none());
        assert!(l.avatar.notice.is_none());
        assert_eq!(l.avatar.receipts, receipts);
        assert_eq!(l.icon_url, "https://example.test/original.png");
        assert_eq!(
            (&*l.title, &*l.description),
            ("Local title", "Local description")
        );
    }

    #[test]
    fn post_native_proof_changes_refuse_without_clearing_notice_or_starting_request() {
        for change in 0..7 {
            let mut l = uploaded();
            l.avatar.notice = Some("Prior crop failure".into());
            let e = l.expected();
            match change {
                0 => l.avatar.x = 32.,
                1 => l.title.push_str(" ahead"),
                2 => l.description.push_str(" ahead"),
                3 => l.epoch += 1,
                4 => l.avatar.crop_bound = false,
                5 => l.avatar.pending = true,
                _ => l.avatar.upload = None,
            }
            let before = l.avatar.value();
            assert!(!begin_crop(&mut l, &e, 99), "change {change}");
            assert_eq!(l.avatar.value(), before, "change {change}");
        }
    }
}

fn upload_error(error: &Error) -> &'static str {
    match error {
        Error::Api(code) if code == "invalid_type" => "Profile photos must be GIFs, JPGs or PNGs.",
        Error::Api(code) if code == "max_size" => {
            "That\u{2019}s a very nice photo, but it\u{2019}s a bit too big. Try one that\u{2019}s smaller than 2 MB."
        }
        Error::Api(code) if code == "too_small" => {
            "That\u{2019}s a very nice photo, but it\u{2019}s a bit too small. Try one that\u{2019}s larger than 128x128."
        }
        Error::Api(code) if code == "too_large" => {
            "That\u{2019}s a very nice photo, but it\u{2019}s a bit too big. Try one that\u{2019}s smaller than 2000x2000."
        }
        _ => "Sorry! Something went wrong. Please try again.",
    }
}
