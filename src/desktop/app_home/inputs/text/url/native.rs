use super::*;

#[component]
pub(crate) fn Control(
    state: Signal<ShellState>,
    visit: actions::Key,
    input_key: InputKey,
    id: String,
    label: String,
    description: String,
    invalid: String,
    placeholder: String,
    locked: bool,
) -> Element {
    let mount = use_hook(next);
    let host = format!("home-url-{mount}");
    let mounted_host = host.clone();
    let locked = locked || !super::super::super::current(&state.read(), visit);
    let loading = loading(&state.read(), &input_key);
    let focus = super::super::super::focus::config(&state.read(), &input_key)
        .unwrap_or(Value::Null)
        .to_string();
    rsx! {span{id:"{host}",style:"display:contents","data-home-text-mount":mount,
        "data-native-id":id,"data-label":label,"data-description":description,"data-invalid":invalid,
        "data-placeholder":placeholder,"data-locked":locked.to_string(),"data-url-loading":loading.to_string(),"data-home-focus":focus,
        onmounted:move |_|{let host=mounted_host.clone();let key=input_key.clone();
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
    let binding = {
        let mut s = state.write();
        if !snapshot_current(&s, visit) || !supported(&s, &key) {
            return;
        }
        let Some(form) = owner(&s) else { return };
        let h = s.app_home.surface.as_ref().unwrap();
        let b = Binding {
            owner: h.owner.clone(),
            form: form.clone(),
            visit,
            key: key.clone(),
            raw: h.view.as_ref().unwrap().raw().clone(),
        };
        super::super::poll(&mut s);
        if s.app_home.text.mounts.len() >= 256 {
            return;
        }
        s.app_home.text.mounts.insert(
            (form, key),
            Mount {
                id: mount,
                seq: 0,
                value: String::new(),
                binding: b.clone(),
            },
        );
        b
    };
    let Some(policy) = configure(&mut state.write(), &binding) else {
        return;
    };
    let config = json!({"host":host,"mount":mount,"visit":visit.visit,"revision":visit.revision,"policy":policy,
        "focus":super::super::super::focus::config(&state.peek(),&binding.key)});
    let script = format!(
        "{}\n{}\nconst c=mountHomeUrl({},dioxus);if(c){{for(;;){{const ack=await dioxus.recv();c.ack(ack);if(ack.finish){{c.stop();dioxus.send({{cleanup:true}});await dioxus.recv();break;}}}}}}",
        super::super::super::focus::SCRIPT,
        include_str!("bridge.js"),
        config
    );
    let mut eval = dioxus::document::eval(&script);
    while let Ok(event) = eval.recv::<UrlOccurrence>().await {
        let current = binding.current(&state.peek());
        let queued = receive(&mut state.write(), &binding, &event);
        let _ = eval.send(json!({"seq":event.seq,"finish":!current||event.kind=="closed"}));
        if let Some(id) = queued {
            launch(state, (binding.form.clone(), binding.key.clone()), id);
        }
        if !current || event.kind == "closed" {
            for _ in 0..8 {
                if let Ok(Ok(value)) =
                    tokio::time::timeout(Duration::from_secs(2), eval.recv::<Value>()).await
                {
                    if value["cleanup"] == true {
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
