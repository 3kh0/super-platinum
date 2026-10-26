use super::{
    Binding, Entry, Event, InputKey, ShellState, actions, configure, form, next, owner,
    preferences, receive, snapshot_current,
};
use dioxus::prelude::*;
use serde_json::json;
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
    let host = format!("home-time-{mount}");
    let mounted_host = host.clone();
    let locked = locked
        || !super::super::super::current(&state.read(), visit)
        || super::super::super::static_select::loading(&state.read(), &input_key.block_id);
    let focus = super::super::super::focus::config(&state.read(), &input_key)
        .map(|v| v.to_string())
        .unwrap_or_default();
    rsx! {span{id:host.clone(),class:"home-time-control","data-home-native-mount":mount,"data-native-id":id,"data-label":label,"data-description":description,"data-required":required,"data-invalid":invalid,"data-locked":locked.to_string(),"data-home-focus":focus,
      onmounted:move |_|{let key=input_key.clone();let host=mounted_host.clone();dioxus::core::spawn_forever(async move{mount_control(state,visit,key,mount,host).await;});}
    }}
}
async fn mount_control(
    mut state: Signal<ShellState>,
    visit: actions::Key,
    key: InputKey,
    mount: u64,
    host: String,
) {
    let captured = {
        let mut s = state.write();
        if !snapshot_current(&s, visit) {
            return;
        }
        let Some(o) = owner(&s) else { return };
        let b = Binding {
            owner: s.app_home.surface.as_ref().unwrap().owner.clone(),
            form: o.clone(),
            visit,
            key: key.clone(),
            raw: form(&s).unwrap().raw_view().clone(),
        };
        let Some((selected, optional)) = configure(&mut s, &b) else {
            return;
        };
        let (locale, time24) = preferences(&s);
        let field = form(&s).unwrap().time_field(&key).unwrap();
        let placeholder = field.placeholder.clone().unwrap_or_else(|| "Time".into());
        let index = (o.clone(), key.clone());
        let epoch = s.core.account_epoch;
        let focus = super::super::super::focus::config(&s, &key);
        s.app_home
            .native
            .entries
            .retain(|(o, _), _| o.epoch == epoch);
        if !s.app_home.native.entries.contains_key(&index) && s.app_home.native.entries.len() >= 256
        {
            return;
        }
        let e = s
            .app_home
            .native
            .entries
            .entry(index)
            .or_insert_with(|| Entry {
                id: next(),
                mount,
                seq: 0,
                pending: true,
                invalid: false,
                value: String::new(),
            });
        e.mount = mount;
        e.seq = 0;
        e.pending = true;
        let config = json!({"host":host,"id":e.id,"epoch":epoch,"mount":mount,"selected":selected,"optional":optional,"locale":locale,"time24":time24,"placeholder":placeholder,"visit":visit.visit,"revision":visit.revision,"focus":focus});
        (b, config)
    };
    let (binding, config) = captured;
    let script = format!(
        "const timeDayjs={};timeDayjs.extend({});{}\n{}\n{}\n{}\nconst control=homeTimeNative({},dioxus);if(!control){{dioxus.send({{mount:{mount},seq:1,kind:'closed',value:'',valid:false,bad:true,selected:null,parsed:null,local_invalid:false,expanded:false,raw_dirty:false}});await dioxus.recv();}}else{{for(;;){{const ack=await dioxus.recv();control.ack(ack);if(ack.finish)break;}}}}",
        include_str!("vendor/dayjs.js"),
        include_str!("vendor/custom-parse-format.js"),
        include_str!("normalize.js"),
        include_str!("model.js"),
        include_str!("bridge.js"),
        super::super::super::focus::SCRIPT,
        config
    );
    let mut eval = dioxus::document::eval(&script);
    while let Ok(event) = eval.recv::<Event>().await {
        let closed = event.kind == "closed";
        let action = {
            let mut s = state.write();
            receive(&mut s, &binding, mount, &event)
        };
        let _ = eval.send(json!({"mount":mount,"seq":event.seq,"release":false,"finish":closed}));
        if let Some(action) = action {
            super::super::super::static_select::date_event(state, binding.clone(), mount, action);
        }
        if closed {
            break;
        }
    }
}
