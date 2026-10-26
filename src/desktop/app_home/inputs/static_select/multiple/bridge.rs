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
    locked: bool,
) -> Element {
    let mount = use_hook(next);
    let host = format!("home-multi-static-{mount}");
    let mounted_host = host.clone();
    let locked = locked
        || !super::super::super::current(&state.read(), visit)
        || loading(&state.read(), &input_key.block_id);
    let maximum = form(&state.read())
        .and_then(|f| f.multi_static_field(&input_key).ok())
        .and_then(|f| match f.kind {
            InputKind::StaticSelect {
                max_selected_items, ..
            } => max_selected_items,
            _ => None,
        });
    rsx! {span{id:"{host}",style:"display:contents","data-home-static-mount":mount,"data-native-id":id,
        "data-label":label,"data-description":description,"data-required":required,"data-invalid":invalid,
        "data-maximum":maximum.map(|n|n.to_string()).unwrap_or_default(),"data-locked":locked.to_string(),
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
        let Some(w) = s.app_home.static_select.multiples.get(&index) else {
            return;
        };
        let visible = w.visible.clone();
        let field = form(&s).unwrap().multi_static_field(&key).unwrap();
        let InputKind::StaticSelect { options, .. } = &field.kind else {
            return;
        };
        let config = json!({"host":host,"mount":mount,"visit":visit.visit,"revision":visit.revision,
            "selected":visible,"options":options.iter().map(|o|json!({"label":o.text,"group":o.group,"value":o.value,"raw":o.raw})).collect::<Vec<_>>()});
        poll(&mut s);
        if s.app_home.static_select.multi_mounts.len() >= 256 {
            return;
        }
        s.app_home.static_select.multi_mounts.insert(
            index,
            Mount {
                id: mount,
                seq: 0,
                values: values(&visible),
                binding: b.clone(),
            },
        );
        (b, config)
    };
    let script = format!(
        "{}\nconst c=mountHomeMultiStatic({},dioxus);if(c){{for(;;){{const a=await dioxus.recv();c.ack(a);if(a.finish){{c.stop();dioxus.send({{cleanup:true}});await dioxus.recv();break;}}}}}}",
        include_str!("bridge.js"),
        config
    );
    let mut eval = dioxus::document::eval(&script);
    while let Ok(event) = eval.recv::<Occurrence>().await {
        let current = binding.current(&state.peek());
        let dispatch = receive(&mut state.write(), &binding, &event);
        let _ = eval.send(json!({"seq":event.seq,"finish":!current||event.kind=="closed"}));
        if let Some(id) = dispatch {
            let index = (binding.form.clone(), binding.key.clone());
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
