use super::*;
#[component]
pub(crate) fn Control(
    state: Signal<ShellState>,
    visit: actions::Key,
    input_key: InputKey,
    id: String,
    label: String,
    description: String,
    required: String,
    invalid: String,
    placeholder: String,
    locked: bool,
    #[props(default)] compact: bool,
    #[props(default)] accessible_label: String,
) -> Element {
    let mount = use_hook(next);
    let host = format!("home-static-{mount}");
    let mounted_host = host.clone();
    let locked = locked
        || !super::super::current(&state.read(), visit)
        || loading(&state.read(), &input_key.block_id);
    rsx! {span{id:"{host}",style:"display:contents","data-home-static-mount":mount,"data-native-id":id,
        "data-label":label,"data-description":description,"data-required":required,"data-invalid":invalid,
        "data-placeholder":placeholder,"data-locked":locked.to_string(),
        "data-compact":compact.to_string(),"data-accessible-label":accessible_label,
        onmounted:move |_|{let key=input_key.clone();let host=mounted_host.clone();
            dioxus::core::spawn_forever(mount_control(state,visit,key,mount,host));}
    }}
}
async fn mount_control(
    mut state: Signal<ShellState>,
    visit: actions::Key,
    key: InputKey,
    mount: u64,
    host: String,
) {
    let (binding, config) = {
        let mut s = state.write();
        if !snapshot_current(&s, visit) || !dispatching(&s, &key) {
            return;
        }
        let Some(o) = owner(&s) else { return };
        let h = s.app_home.surface.as_ref().unwrap();
        let b = Binding {
            owner: h.owner.clone(),
            form: o.clone(),
            visit,
            key: key.clone(),
            raw: h.view.as_ref().unwrap().raw().clone(),
        };
        let index = (o, key.clone());
        let Some(w) = s.app_home.static_select.widgets.get(&index) else {
            return;
        };
        let visible = w.visible.clone();
        let field = form(&s).unwrap().static_field(&key).unwrap();
        let InputKind::StaticSelect { options, .. } = &field.kind else {
            return;
        };
        let allows_clear = form(&s).unwrap().static_allows_clear(&key).unwrap();
        let config = json!({"host":host,"mount":mount,"visit":visit.visit,"revision":visit.revision,"optional":allows_clear,
            "selected":visible.as_ref().and_then(|v|options.iter().position(|o|&o.value==v)),
            "options":options.iter().map(|o|json!({"label":o.text,"group":o.group,"value":o.value})).collect::<Vec<_>>()});
        poll(&mut s);
        if s.app_home.static_select.mounts.len() >= 256 {
            return;
        }
        s.app_home.static_select.mounts.insert(
            index,
            Mount {
                id: mount,
                seq: 0,
                value: visible,
                binding: b.clone(),
            },
        );
        (b, config)
    };
    let script = format!(
        "{}\nconst c=mountHomeStatic({},dioxus);if(c){{for(;;){{const a=await dioxus.recv();c.ack(a);if(a.finish){{c.stop();dioxus.send({{cleanup:true}});await dioxus.recv();break;}}}}}}",
        include_str!("bridge.js"),
        config
    );
    let mut eval = dioxus::document::eval(&script);
    while let Ok(event) = eval.recv::<Occurrence>().await {
        let current = binding.current(&state.peek());
        let dispatch = receive(&mut state.write(), &binding, &event);
        let _ = eval.send(json!({"seq":event.seq,"finish":!current||event.kind=="closed"}));
        if let Some((id, leading)) = dispatch {
            let index = (binding.form.clone(), binding.key.clone());
            if let Some(q) = leading {
                dioxus::core::spawn_forever(delivery::deliver(state, q));
            }
            dioxus::core::spawn_forever(trailing(state, index, id));
        }
        if !current || event.kind == "closed" {
            for _ in 0..8 {
                match tokio::time::timeout(Duration::from_secs(2), eval.recv::<Value>()).await {
                    Ok(Ok(v)) if v["cleanup"] == true => break,
                    Ok(Ok(_)) => {}
                    _ => break,
                }
            }
            let _ = eval.send(());
            break;
        }
    }
}
pub(crate) struct Lease {
    release: super::super::native::Release,
    expected: Value,
}
impl Lease {
    pub(crate) fn current_visit(&self, s: &ShellState, visit: actions::Key) -> bool {
        expected_visit(s, visit).as_ref() == Some(&self.expected)
    }
    pub(super) fn current(&self, s: &ShellState, q: &Queued) -> bool {
        expected(s, q).as_ref() == Some(&self.expected)
    }
    pub(crate) fn release(&self) {
        self.release.release();
    }
}
fn expected(s: &ShellState, q: &Queued) -> Option<Value> {
    if !q.current(s) {
        return None;
    }
    expected_visit(s, q.binding.visit)
}
pub(super) fn expected_visit(s: &ShellState, visit: actions::Key) -> Option<Value> {
    if !super::super::current(s, visit) {
        return None;
    }
    let f = form(s)?;
    let o = owner(s)?;
    let mut fields = vec![];
    for (key, field) in &f.fields {
        if let InputKind::Radio { options } = &field.kind {
            f.radio_field(key).ok()?;
            let m = s
                .app_home
                .static_select
                .radio_mounts
                .get(&(o.clone(), key.clone()))?;
            if !m.binding.current(s) || m.seq == 0 {
                return None;
            }
            fields.push(json!({"id":crate::view::app_modal::field_id(key),"radio":true,"mount":m.id,"seq":m.seq,"value":m.value,
                "options":options.iter().enumerate().map(|(i,o)|json!({"value":format!("option-{i}"),"sourceValue":o.value,"source":serde_json::to_string(&o.raw).ok()})).collect::<Vec<_>>() }));
            continue;
        }
        if let InputKind::Checkboxes { options } = &field.kind {
            f.checkbox_field(key).ok()?;
            let m = s
                .app_home
                .static_select
                .check_mounts
                .get(&(o.clone(), key.clone()))?;
            if !m.binding.current(s) || m.seq == 0 {
                return None;
            }
            fields.push(json!({"id":crate::view::app_modal::field_id(key),"checkbox":true,"mount":m.id,"seq":m.seq,"values":m.values,
                "options":options.iter().enumerate().map(|(i,o)|json!({"value":format!("option-{i}"),"sourceValue":o.value,"source":serde_json::to_string(&o.raw).ok()})).collect::<Vec<_>>() }));
            continue;
        }
        let InputKind::StaticSelect {
            options, multiple, ..
        } = &field.kind
        else {
            continue;
        };
        let index = (o.clone(), key.clone());
        if *multiple {
            let m = if multiple::dispatching(s, key) {
                Some(s.app_home.static_select.multi_mounts.get(&index)?)
            } else {
                None
            };
            let values = if let Some(m) = m {
                m.values.clone()
            } else {
                match &field.value {
                    InputValue::Selected(v) => v.clone(),
                    _ => return None,
                }
            };
            fields.push(json!({"id":crate::view::app_modal::field_id(key),"multi":true,"mount":m.map(|m|m.id),"seq":m.map(|m|m.seq),"values":values,
                "options":options.iter().enumerate().map(|(i,o)|json!({"value":format!("option-{i}"),"sourceValue":o.value,"text":o.group.as_ref().map(|g|if m.is_some(){format!("{g}: {}",o.text)}else{format!("{g} \u{2014} {}",o.text)}).unwrap_or_else(||o.text.clone())})).collect::<Vec<_>>() }));
            continue;
        }
        let m = if dispatching(s, key) {
            Some(s.app_home.static_select.mounts.get(&index)?)
        } else {
            None
        };
        let visible = if let Some(m) = m {
            m.value.as_ref()
        } else {
            match &field.value {
                InputValue::Selected(v) => v.first(),
                _ => return None,
            }
        };
        fields.push(json!({"id":crate::view::app_modal::field_id(key),"mount":m.map(|m|m.id),"seq":m.map(|m|m.seq),
            "index":visible.and_then(|v|options.iter().position(|o|&o.value==v)),
            "options":options.iter().enumerate().map(|(i,o)|json!({"value":format!("option-{i}"),"text":o.group.as_ref().map(|g|format!("{g} \u{2014} {}",o.text)).unwrap_or_else(||o.text.clone())})).collect::<Vec<_>>() }));
    }
    Some(json!({"visit":visit.visit,"revision":visit.revision,"fields":fields}))
}
pub(super) enum Lock {
    Acquired(Lease),
    Busy,
    Refused,
}
pub(super) async fn lock(state: Signal<ShellState>, q: &Queued) -> Lock {
    let Some(expected) = expected(&state.peek(), q) else {
        return Lock::Refused;
    };
    lock_expected(expected).await
}
pub(crate) async fn lock_for_button(
    state: Signal<ShellState>,
    visit: actions::Key,
) -> Option<Lease> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if Instant::now() >= deadline {
            return None;
        }
        let expected = expected_visit(&state.peek(), visit)?;
        match lock_expected(expected).await {
            Lock::Acquired(lease) => return Some(lease),
            Lock::Refused => return None,
            Lock::Busy => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
}
async fn lock_expected(expected: Value) -> Lock {
    let script = format!(
        "{}\nconst r=lockHomeStatic({});dioxus.send(typeof r==='function'?'locked':r==='busy'?'busy':'refused');await dioxus.recv();if(typeof r==='function'){{r();dioxus.send(true);await dioxus.recv();}}",
        include_str!("bridge.js"),
        expected
    );
    let mut eval = dioxus::document::eval(&script);
    let result = eval.recv::<String>().await.ok();
    if result.as_deref() == Some("locked") {
        Lock::Acquired(Lease {
            release: super::super::native::retain_acknowledged(eval),
            expected,
        })
    } else {
        let _ = eval.send(());
        if result.as_deref() == Some("busy") {
            Lock::Busy
        } else {
            Lock::Refused
        }
    }
}
