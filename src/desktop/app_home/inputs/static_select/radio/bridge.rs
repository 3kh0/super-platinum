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
    let host = format!("home-radio-{mount}");
    let mounted_host = host.clone();
    let s = state.read();
    let options = form(&s)
        .and_then(|f| f.radio_field(&input_key).ok())
        .and_then(|f| match &f.kind {
            InputKind::Radio { options } => Some(options.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let selected = owner(&s)
        .and_then(|o| s.app_home.static_select.radios.get(&(o, input_key.clone())))
        .map(|w| w.visible.clone())
        .unwrap_or_default();
    let locked =
        locked || !super::super::super::current(&s, visit) || loading(&s, &input_key.block_id);
    drop(s);
    rsx! {span{id:"{host}",style:"display:contents","data-home-static-mount":mount,"data-native-id":id.clone(),"data-label":label.clone(),"data-description":description.clone(),"data-required":required.clone(),"data-invalid":invalid.clone(),"data-locked":locked.to_string(),
        onmounted:move |_|{let key=input_key.clone();let host=mounted_host.clone();dioxus::core::spawn_forever(mount_control(state,visit,key,mount,host));},
        div{id:"{id}",class:"app-modal-options block-radio-group",role:"radiogroup","aria-labelledby":label,"aria-describedby":description,"aria-required":required,"aria-invalid":invalid,"aria-disabled":locked.to_string(),
            for (index,option) in options.iter().enumerate(){
                {let row_id=format!("{id}-option-{index}");let label_id=format!("{row_id}-label");let description_id=format!("{row_id}-description");let subtext=option.raw.get("description").filter(|v|v["text"].is_string());let source=serde_json::to_string(&option.raw).unwrap();
                rsx! {label{key:"option-{index}",class:"block-radio-option",r#for:row_id.clone(),"data-option-source":source,
                    input{id:row_id,r#type:"radio",name:"{host}",value:"option-{index}","data-option-index":index.to_string(),initial_checked:selected.as_ref()==Some(&option.value),disabled:locked,"aria-labelledby":label_id.clone(),"aria-describedby":if subtext.is_some(){description_id.clone()}else{String::new()}},
                    span{class:"block-radio-copy",
                        span{id:label_id,class:"block-radio-label",{crate::view::message_radio::composition(state,option.raw.get("text"),&option.text)}}
                        if let Some(raw)=subtext {span{id:description_id.clone(),class:"block-radio-description",{crate::view::message_radio::composition(state,Some(raw),"")}}}
                    }
                }}}
            }
        }
    }}
}
async fn mount_control(
    mut state: Signal<ShellState>,
    visit: actions::Key,
    key: InputKey,
    mount: u64,
    host: String,
) {
    let Some((binding, config)) = ({
        let mut s = state.write();
        if !snapshot_current(&s, visit) || !supported(&s, &key) {
            None
        } else {
            let o = owner(&s).unwrap();
            let h = s.app_home.surface.as_ref().unwrap();
            let b = Binding {
                owner: h.owner.clone(),
                form: o.clone(),
                visit,
                key: key.clone(),
                raw: h.view.as_ref().unwrap().raw().clone(),
            };
            let index = (o, key.clone());
            let visible = s
                .app_home
                .static_select
                .radios
                .get(&index)
                .map(|w| w.visible.clone());
            visible.and_then(|visible|{
                let field=form(&s)?.radio_field(&key).ok()?;let InputKind::Radio{options}=&field.kind else{return None};
                let config=json!({"host":host,"mount":mount,"visit":visit.visit,"revision":visit.revision,"selected":visible,"options":options.iter().map(|o|json!({"value":o.value,"source":serde_json::to_string(&o.raw).unwrap()})).collect::<Vec<_>>()});
                poll(&mut s);if s.app_home.static_select.radio_mounts.len()>=256{return None;}
                s.app_home.static_select.radio_mounts.insert(index,Mount{id:mount,seq:0,value:visible,binding:b.clone()});Some((b,config))
            })
        }
    }) else {
        return;
    };
    let script = format!(
        "{}\nconst c=mountHomeRadio({},dioxus);if(c){{for(;;){{const a=await dioxus.recv();c.ack(a);if(a.finish){{c.stop();dioxus.send({{cleanup:true}});await dioxus.recv();break;}}}}}}",
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
