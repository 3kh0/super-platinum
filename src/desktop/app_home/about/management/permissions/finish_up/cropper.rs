//! Renderer-owned geometry and original-node proofs, never a publication grant.
use super::*;
mod view;
pub(super) use view::surface;

#[derive(Default)]
pub(super) struct State {
    pub ready: bool,
    pub loading: bool,
    pub interaction_pending: bool,
    pub geometry: Option<Value>,
    pub generation: u64,
}
impl State {
    pub(super) fn value(&self, url: Option<&str>) -> Value {
        let g = self.geometry.as_ref().unwrap_or(&Value::Null);
        json!({"ready":self.ready,"originalImage":{"url":url,"naturalWidth":g["naturalWidth"],"naturalHeight":g["naturalHeight"]},
            "display":g["display"],"selection":g["selection"],"generation":self.generation,
            "interactionPending":self.interaction_pending})
    }
}
fn mutable(s: &mut ShellState) -> &mut Local {
    s.app_home
        .about
        .management
        .permissions
        .confirmation
        .as_mut()
        .unwrap()
        .finish_up
        .as_mut()
        .unwrap()
}
fn same(s: &ShellState, o: &Original, e: &Value) -> bool {
    local(s, o.visit, o.id)
        .is_some_and(|(_, l)| l.expected() == *e && l.avatar.pane == avatar::Pane::Crop)
}
// Called by the actual opaque image load/mount. Local file dimensions are not
// evidence of what the upload endpoint returned and never initialize this crop.
pub(super) fn loaded(mut state: Signal<ShellState>, visit: u64, id: u64, source: String) {
    let captured = {
        let s = state.peek();
        local(&s, visit, id)
            .filter(|(_, l)| {
                l.avatar.pane == avatar::Pane::Crop
                    && !l.avatar.cropper.loading
                    && l.avatar
                        .cropper
                        .geometry
                        .as_ref()
                        .is_none_or(|g| g["source"] != source)
            })
            .map(|(c, l)| (c.original.clone(), l.expected()))
    };
    let Some((o, e)) = captured else { return };
    mutable(&mut state.write()).avatar.cropper.loading = true;
    dioxus::core::spawn_forever(async move {
        if !native::bind(&o, &e).await {
            let mut s = state.write();
            if same(&s, &o, &e) {
                mutable(&mut s).avatar.cropper.loading = false;
            }
            return;
        }
        if !same(&state.peek(), &o, &e) {
            return;
        }
        // Wait for measured layout, matching the source's two animation frames.
        let original = o.expected();
        let source_json = json!(source);
        let mut eval = document::eval(&format!(
            "{}\nawait new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))); await workflowPermissionsReply(await workflowCropReadyWait({original},{e},{source_json}),dioxus);",
            native::source()
        ));
        let reply = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>()).await;
        let _ = eval.send(());
        let mut s = state.write();
        if !same(&s, &o, &e) {
            return;
        }
        let l = mutable(&mut s);
        l.avatar.cropper.loading = false;
        if !reply.is_ok_and(|reply| reply.is_ok_and(|reply| initialize(l, &reply, &source))) {
            l.avatar.notice = Some(
                "The uploaded image could not be verified. Go back to choose another image.".into(),
            );
            return;
        }
        drop(s);
        actions::bind(state, visit, id);
    });
}
fn initialize(l: &mut Local, reply: &Value, source: &str) -> bool {
    let image = &reply["image"];
    let Some(w) = image["naturalWidth"]
        .as_u64()
        .filter(|v| *v >= 128 && *v <= u32::MAX as u64)
    else {
        return false;
    };
    let Some(h) = image["naturalHeight"]
        .as_u64()
        .filter(|v| *v >= 128 && *v <= u32::MAX as u64)
    else {
        return false;
    };
    if image["source"] != source || l.avatar.upload.is_none() {
        return false;
    }
    let g = json!({"source":source,"naturalWidth":w,"naturalHeight":h,"display":reply["display"],"selection":reply["selection"]});
    if !valid(&g, &reply["full"]) {
        return false;
    }
    // The server image's actual natural dimensions replace local decode metadata.
    let upload = l.avatar.upload.as_mut().unwrap();
    upload.2 = w as u32;
    upload.3 = h as u32;
    l.avatar.cropper.geometry = Some(g);
    l.avatar.cropper.generation = next();
    assign(l, reply);
    true
}
fn assign(l: &mut Local, reply: &Value) {
    l.avatar.x = reply["full"]["x"].as_f64().unwrap();
    l.avatar.y = reply["full"]["y"].as_f64().unwrap();
    l.avatar.size = reply["full"]["size"].as_f64().unwrap();
    l.avatar.cropper.geometry.as_mut().unwrap()["selection"] = reply["selection"].clone();
    l.avatar.cropper.ready = false;
    l.avatar.cropper.interaction_pending = false;
    l.epoch = next();
}
fn finite(v: &Value) -> Option<f64> {
    v.as_f64().filter(|v| v.is_finite() && *v >= 0.)
}
fn valid(g: &Value, full: &Value) -> bool {
    let values = [
        &g["naturalWidth"],
        &g["naturalHeight"],
        &g["display"]["width"],
        &g["display"]["height"],
        &g["selection"]["x"],
        &g["selection"]["y"],
        &g["selection"]["width"],
        &g["selection"]["height"],
        &full["x"],
        &full["y"],
        &full["size"],
    ];
    let Some(v) = values.into_iter().map(finite).collect::<Option<Vec<_>>>() else {
        return false;
    };
    let [nw, nh, w, h, x, y, cw, ch, fx, fy, fs] = v[..] else {
        return false;
    };
    let close = |a: f64, b: f64| (a - b).abs() < 0.001;
    nw >= 128.
        && nh >= 128.
        && w > 0.
        && h > 0.
        && w <= nw + 0.1
        && h <= nh + 0.1
        && close(w / h, nw / nh)
        && close(cw, ch)
        && cw + 0.001 >= 128_f64.min(w.min(h))
        && cw <= 2000.001
        && x + cw <= w + 0.001
        && y + ch <= h + 0.001
        && close(fx, x * (nw / w).max(1.))
        && close(fy, y * (nh / h).max(1.))
        && close(fs, cw * (nw / w).max(1.))
}
pub(super) async fn bind(mut state: Signal<ShellState>, o: &Original, e: &Value) {
    let g = {
        let s = state.peek();
        local(&s, o.visit, o.id)
            .filter(|(_, l)| l.expected() == *e)
            .and_then(|(_, l)| l.avatar.cropper.geometry.clone())
    };
    let Some(g) = g else { return };
    let original = o.expected();
    let mut eval = document::eval(&format!(
        "{}\nawait workflowPermissionsReply(workflowCropInstall({original},{e},{g}),dioxus);",
        native::source()
    ));
    let reply = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    let _ = eval.send(());
    let mut s = state.write();
    if !same(&s, o, e) {
        return;
    }
    mutable(&mut s).avatar.cropper.ready = matches!(reply, Ok(Ok(true)));
}
pub(super) fn event(state: Signal<ShellState>, visit: u64, id: u64, token: String) {
    let Ok(token) = token.parse::<u64>() else {
        return;
    };
    change(state, visit, id, Some(token), None);
}
pub(super) fn numeric(
    state: Signal<ShellState>,
    visit: u64,
    id: u64,
    key: &'static str,
    value: String,
) {
    let Ok(value) = value.parse::<f64>() else {
        return;
    };
    if !value.is_finite() || value < 0. || !matches!(key, "crop-x" | "crop-y" | "crop-size") {
        return;
    }
    change(state, visit, id, None, Some((key, value)));
}
fn change(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
    token: Option<u64>,
    numeric: Option<(&'static str, f64)>,
) {
    let captured = {
        let s = state.peek();
        local(&s, visit, id)
            .filter(|(_, l)| {
                l.avatar.pane == avatar::Pane::Crop
                    && l.avatar.pane_bound
                    && l.avatar.cropper.ready
                    && !l.avatar.cropper.interaction_pending
            })
            .map(|(c, l)| (c.original.clone(), l.expected()))
    };
    let Some((o, e)) = captured else { return };
    mutable(&mut state.write())
        .avatar
        .cropper
        .interaction_pending = true;
    dioxus::core::spawn_forever(async move {
        let original = o.expected();
        let proof = if let Some(token) = token {
            format!("workflowCropCommit({original},{e},{token})")
        } else {
            let (key, value) = numeric.unwrap();
            format!("workflowCropNumeric({original},{e},{},{value})", json!(key))
        };
        let complete = token
            .map(|token| format!("workflowCropComplete({original},{token},accepted);"))
            .unwrap_or_default();
        // Keep the pinned Eval alive until Rust decides whether the exact old
        // owner/epoch still owns this completed gesture. Never replay the event.
        let mut eval = document::eval(&format!(
            "{}\nconst proof={proof};dioxus.send(proof);const accepted=await dioxus.recv();{complete}",
            native::source()
        ));
        let reply = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>()).await;
        let accepted = {
            let mut s = state.write();
            if !same(&s, &o, &e) {
                // A pending server failure may change avatarPending while this
                // proof is awaited. Release only this original epoch's local
                // commit latch; never adopt its old coordinates or a successor.
                if local(&s, visit, id).is_some_and(|(_, l)| json!(l.epoch) == e["epoch"]) {
                    mutable(&mut s).avatar.cropper.interaction_pending = false;
                }
                false
            } else if let Ok(Ok(reply)) = reply {
                let l = mutable(&mut s);
                l.avatar.cropper.interaction_pending = false;
                let mut g = l.avatar.cropper.geometry.clone().unwrap();
                g["selection"] = reply["selection"].clone();
                if valid(&g, &reply["full"]) {
                    assign(l, &reply);
                    true
                } else {
                    false
                }
            } else {
                mutable(&mut s).avatar.cropper.interaction_pending = false;
                false
            }
        };
        let _ = eval.send(accepted);
        if accepted {
            actions::bind(state, visit, id);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn local_image() -> Local {
        let mut avatar = avatar::State::default();
        avatar.pane = avatar::Pane::Crop;
        avatar.upload = Some((
            "AVATAR1".into(),
            "https://example.test/reply.png".into(),
            256,
            256,
        ));
        Local {
            title: "Original title".into(),
            description: "Original description".into(),
            icon_url: "https://example.test/icon.png".into(),
            more: false,
            defaults: true,
            epoch: 7,
            notice: None,
            avatar,
        }
    }
    fn wide() -> Value {
        json!({"image":{"source":"opaque-original","naturalWidth":640,"naturalHeight":320},"display":{"width":460,"height":230},"selection":{"x":120.75,"y":5.75,"width":218.5,"height":218.5},"full":{"x":168,"y":8,"size":304}})
    }
    #[test]
    fn loaded_reply_dimensions_replace_local_file_dimensions_only_with_exact_source_and_geometry() {
        let mut l = local_image();
        let reply = wide();
        assert!(!initialize(&mut l, &reply, "another-source"));
        assert_eq!(l.avatar.upload.as_ref().unwrap().2, 256);
        assert!(initialize(&mut l, &reply, "opaque-original"));
        assert_eq!(
            (
                l.avatar.upload.as_ref().unwrap().2,
                l.avatar.upload.as_ref().unwrap().3
            ),
            (640, 320)
        );
        assert_eq!((l.avatar.x, l.avatar.y, l.avatar.size), (168., 8., 304.));
        assert_eq!(l.icon_url, "https://example.test/icon.png");
        assert!(!l.avatar.cropper.ready);
    }
    #[test]
    fn pending_selection_commit_preserves_original_sent_tuple_operation_and_details() {
        let mut l = local_image();
        assert!(initialize(&mut l, &wide(), "opaque-original"));
        l.avatar.pending = true;
        l.avatar
            .receipts
            .push(json!({"kind":"upload","raw":{"ok":true,"id":"AVATAR1"}}));
        let generation = l.avatar.value()["generation"].clone();
        let sent = json!({"x":l.avatar.x,"y":l.avatar.y,"size":l.avatar.size});
        let before = l.avatar.receipts.clone();
        assign(
            &mut l,
            &json!({"selection":{"x":92,"y":46,"width":128,"height":128},"full":{"x":128,"y":64,"size":128.*640./460.}}),
        );
        assert!(l.avatar.pending);
        assert_eq!(l.avatar.value()["generation"], generation);
        assert_eq!(sent, json!({"x":168.,"y":8.,"size":304.}));
        assert_eq!(l.avatar.receipts, before);
        assert_eq!(
            (&*l.title, &*l.description, &*l.icon_url),
            (
                "Original title",
                "Original description",
                "https://example.test/icon.png"
            )
        );
    }
    #[test]
    fn actual_reply_geometry_requires_proportional_ready_bounds_and_source_display_minimum() {
        let g = json!({"naturalWidth":640,"naturalHeight":320,"display":{"width":460,"height":230},"selection":{"x":92,"y":46,"width":128,"height":128}});
        assert!(valid(&g, &json!({"x":128,"y":64,"size":128.*640./460.})));
        for key in ["x", "y", "size"] {
            let mut full = json!({"x":128,"y":64,"size":128.*640./460.});
            full[key] = json!(999);
            assert!(!valid(&g, &full));
        }
        let mut bad = g.clone();
        bad["selection"]["width"] = json!(92);
        bad["selection"]["height"] = json!(92);
        assert!(!valid(&bad, &json!({"x":128,"y":64,"size":128})));
        let mut bad = g;
        bad["display"]["height"] = json!(220);
        assert!(!valid(
            &bad,
            &json!({"x":128,"y":64,"size":178.08695652173913})
        ));
    }
}
