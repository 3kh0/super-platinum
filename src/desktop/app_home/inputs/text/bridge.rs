use super::*;

#[component]
pub(crate) fn Control(
    state: Signal<ShellState>,
    visit: actions::Key,
    input_key: InputKey,
    multiline: bool,
    initial: String,
    id: String,
    label: String,
    description: String,
    required: String,
    invalid: String,
    placeholder: String,
    locked: bool,
) -> Element {
    let mount = use_hook(next);
    let locked = locked || !super::super::current(&state.read(), visit);
    let email_loading =
        email::supported(&state.read(), &input_key) && email::loading(&state.read(), &input_key);
    let host = format!("home-text-{mount}");
    let mounted_host = host.clone();
    let focus = super::super::focus::config(&state.read(), &input_key)
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".into());
    rsx! {span{id:"{host}",style:"display:contents","data-home-text-mount":mount,
        "data-native-id":id,"data-label":label,"data-description":description,"data-required":required,
        "data-invalid":invalid,"data-placeholder":placeholder,"data-locked":locked.to_string(),
        "data-email-loading":email_loading.to_string(),
        "data-home-focus":focus,
        onmounted:move |_| {let key=input_key.clone();let host=mounted_host.clone();let initial=initial.clone();
            dioxus::core::spawn_forever(mount_control(state,visit,key,mount,host,multiline,initial));}
    }}
}
async fn mount_control(
    mut state: Signal<ShellState>,
    visit: actions::Key,
    key: InputKey,
    mount: u64,
    host: String,
    multiline: bool,
    initial: String,
) {
    let binding = {
        let mut s = state.write();
        if !snapshot_current(&s, visit)
            || !(dispatching(&s, &key) || number::supported(&s, &key) || email::supported(&s, &key))
        {
            return;
        }
        let Some(f) = owner(&s) else { return };
        let h = s.app_home.surface.as_ref().unwrap();
        let b = Binding {
            owner: h.owner.clone(),
            form: f.clone(),
            visit,
            key: key.clone(),
            raw: h.view.as_ref().unwrap().raw().clone(),
        };
        poll(&mut s);
        if s.app_home.text.mounts.len() >= 256 {
            return;
        }
        s.app_home.text.mounts.insert(
            (f, key),
            Mount {
                id: mount,
                seq: 0,
                value: initial.clone(),
                binding: b.clone(),
            },
        );
        b
    };
    let numeric = number::configure(&mut state.write(), &binding);
    let email_config = email::configure(&mut state.write(), &binding);
    if email::supported(&state.peek(), &binding.key) && email_config.is_none() {
        return;
    }
    if number::supported(&state.peek(), &binding.key) && numeric.is_none() {
        return;
    }
    let mut config = serde_json::json!({"mount":mount,"host":host,"initial":initial,"multiline":multiline,"mac":cfg!(target_os="macos"),"visit":visit.visit,"revision":visit.revision});
    config["focus"] =
        super::super::focus::config(&state.peek(), &binding.key).unwrap_or(serde_json::Value::Null);
    if let Some(numeric) = numeric {
        config["number"] = numeric;
    }
    if let Some(email_config) = email_config {
        config["email"] = email_config;
    }
    let script = format!(
        "{}\n{}\n{}\n{}\nconst c=mountHomeText({},dioxus);if(c){{for(;;){{const ack=await dioxus.recv();c.ack(ack);if(ack.finish){{c.stop();dioxus.send({{cleanup:true}});await dioxus.recv();break;}}}}}}",
        super::super::focus::SCRIPT,
        include_str!("bridge.js"),
        include_str!("number/bridge.js"),
        include_str!("email/bridge.js"),
        config
    );
    let mut eval = dioxus::document::eval(&script);
    while let Ok(event) = eval.recv::<Occurrence>().await {
        let current = binding.current(&state.peek());
        let queued = receive(&mut state.write(), &binding, &event);
        let _ =
            eval.send(serde_json::json!({"seq":event.seq,"finish":!current||event.kind=="closed"}));
        if let Some(id) = queued {
            let index = (binding.form.clone(), binding.key.clone());
            if email::supported(&state.peek(), &binding.key) {
                email::launch(state, index, id);
            } else {
                dioxus::core::spawn_forever(async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    delivery::deliver(state, index, id).await;
                });
            }
        }
        if !current || event.kind == "closed" {
            for _ in 0..8 {
                if let Ok(Ok(v)) =
                    tokio::time::timeout(Duration::from_secs(2), eval.recv::<serde_json::Value>())
                        .await
                {
                    if v["cleanup"] == true {
                        break;
                    }
                } else {
                    break;
                }
            }
            let _ = eval.send(());
            break;
        }
    }
}

/// Text lease is a short final-dispatch observation. Queued event text is not
/// compared to current text: ordinary edits are allowed during debounce.
pub(super) struct TextLease {
    release: super::super::native::Release,
    expected: serde_json::Value,
}
impl TextLease {
    pub(super) fn current(&self, s: &ShellState, q: &Queued) -> bool {
        expected(s, q).as_ref() == Some(&self.expected)
    }
    pub(super) fn current_visit(&self, s: &ShellState, visit: actions::Key) -> bool {
        expected_visit(s, visit).as_ref() == Some(&self.expected)
    }
    pub(super) fn release(&self) {
        self.release.release();
    }
}
fn expected(s: &ShellState, q: &Queued) -> Option<serde_json::Value> {
    q.current(s)
        .then(|| expected_visit(s, q.binding.visit))
        .flatten()
}
pub(super) fn expected_visit(s: &ShellState, visit: actions::Key) -> Option<serde_json::Value> {
    if !snapshot_current(s, visit) || !super::ready(s) {
        return None;
    }
    let f = form(s)?;
    let form_owner = owner(s)?;
    let fields = f
        .fields
        .iter()
        .filter_map(|(k, v)| {
            if email::supported(s,k) {
                let m=s.app_home.text.mounts.get(&(form_owner.clone(),k.clone()))?;
                Some(serde_json::json!({"id":crate::view::app_modal::field_id(k),"value":m.value,"mount":m.id,"seq":m.seq,"email":true}))
            } else if number::supported(s, k) {
                let m = s.app_home.text.mounts.get(&(form_owner.clone(), k.clone()))?;
                Some(serde_json::json!({"id":crate::view::app_modal::field_id(k),"value":m.value,"mount":m.id,"seq":m.seq,"number":true}))
            } else if let InputValue::Text(value) = &v.value {
                Some(
                    serde_json::json!({"id":crate::view::app_modal::field_id(k),"value":value,
            "mount":s.app_home.text.mounts.get(&(form_owner.clone(),k.clone())).map(|m|m.id),
            "seq":s.app_home.text.mounts.get(&(form_owner.clone(),k.clone())).map(|m|m.seq)}),
                )
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    Some(serde_json::json!({"visit":visit.visit,"revision":visit.revision,"fields":fields}))
}
pub(super) enum Lock {
    Acquired(TextLease),
    Busy,
    Refused,
}
pub(super) async fn lock(state: Signal<ShellState>, q: &Queued) -> Lock {
    let Some(expected) = expected(&state.peek(), q) else {
        return Lock::Refused;
    };
    lock_expected(expected).await
}
pub(super) async fn lock_visit(state: Signal<ShellState>, visit: actions::Key) -> Lock {
    let Some(expected) = expected_visit(&state.peek(), visit) else {
        return Lock::Refused;
    };
    lock_expected(expected).await
}
async fn lock_expected(expected: serde_json::Value) -> Lock {
    let script = format!(
        "{}\nconst release=lockHomeText({});dioxus.send(typeof release==='function'?'locked':release==='busy'?'busy':'refused');await dioxus.recv();if(typeof release==='function'){{release();dioxus.send(true);await dioxus.recv();}}",
        include_str!("bridge.js"),
        expected
    );
    let mut eval = dioxus::document::eval(&script);
    let result = eval.recv::<String>().await.ok();
    if result.as_deref() == Some("locked") {
        Lock::Acquired(TextLease {
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
