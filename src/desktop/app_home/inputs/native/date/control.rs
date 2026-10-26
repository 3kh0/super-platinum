use super::{
    Binding, Entry, Event, InputKey, ShellState, actions, configure, form, next, owner, receive,
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
    let host = format!("home-date-{mount}");
    let mounted_host = host.clone();
    let focus = super::super::super::focus::config(&state.read(), &input_key);
    let focus_json = focus
        .as_ref()
        .map(serde_json::Value::to_string)
        .unwrap_or_default();
    let locked = locked
        || !super::super::super::current(&state.read(), visit)
        || super::super::super::static_select::loading(&state.read(), &input_key.block_id);
    rsx! {span{id:host,class:"home-date-control","data-home-native-mount":mount,
        "data-native-id":id,"data-label":label,"data-description":description,"data-required":required,
        "data-invalid":invalid,"data-locked":locked.to_string(),"data-home-focus":focus_json,
        onmounted:move |_|{let key=input_key.clone();let host=mounted_host.clone();dioxus::core::spawn_forever(async move{mount_control(state,visit,key,mount,host).await;});},
        button { r#type:"button",class:"home-date-popup-trigger","data-home-date-action":"open","aria-label":"Open calendar",
            {crate::icons::icon(crate::icons::Icon::CalendarMonth,"home-date-icon")}
        }
        span { hidden:true,"data-home-date-chevron":"true",
            {crate::icons::icon(crate::icons::Icon::ChevronRight,"icon")}
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
    let captured = {
        let mut s = state.write();
        if !super::super::snapshot_current(&s, visit) {
            return;
        }
        let Some(o) = owner(&s) else { return };
        let binding = Binding {
            owner: s.app_home.surface.as_ref().unwrap().owner.clone(),
            form: o.clone(),
            visit,
            key: key.clone(),
            raw: form(&s).unwrap().raw_view().clone(),
        };
        let Some((selected, optional)) = configure(&mut s, &binding) else {
            return;
        };
        let (locale, _) = super::super::time::preferences(&s);
        let focus = super::super::super::focus::config(&s, &key);
        let placeholder = form(&s)
            .unwrap()
            .date_field(&key)
            .unwrap()
            .placeholder
            .clone()
            .unwrap_or_else(|| "Select a date".into());
        let epoch = s.core.account_epoch;
        s.app_home
            .native
            .entries
            .retain(|(o, _), _| o.epoch == epoch);
        let index = (o, key);
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
        let config = json!({"host":host,"id":e.id,"epoch":epoch,"mount":mount,"selected":selected,"optional":optional,"locale":locale,"placeholder":placeholder,"focus":focus,"visit":visit.visit,"revision":visit.revision});
        (binding, config)
    };
    let (binding, config) = captured;
    let script = format!(
        "const timeDayjs={};timeDayjs.extend({});{}\nconst homeDateLocales={};\n{}\n{}\nconst homeDatePopupLocales={};\n{}\n{}\n{}\nconst control=await homeDateMount({},dioxus);if(!control){{dioxus.send({{mount:{mount},seq:1,kind:'closed',value:'[\"\",\"\"]',raw:'',picker:'',selected:null,parsed:null,focused:false,picker_status:'idle',buffer_valid:false,valid:false,bad:true}});await dioxus.recv();}}else{{for(;;){{const ack=await dioxus.recv();control.ack(ack);if(ack.finish)break;}}}}",
        include_str!("../time/vendor/dayjs.js"),
        include_str!("../time/vendor/custom-parse-format.js"),
        include_str!("../time/normalize.js"),
        include_str!("locales.json"),
        include_str!("model.js"),
        super::super::super::focus::SCRIPT,
        include_str!("popup-locales.json"),
        include_str!("popup-model.js"),
        include_str!("popup.js"),
        include_str!("bridge.js"),
        config
    );
    let mut eval = dioxus::document::eval(&script);
    while let Ok(event) = eval.recv::<Event>().await {
        let closed = event.kind == "closed";
        let action = {
            let mut s = state.write();
            receive(&mut s, &binding, mount, &event)
        };
        let _ = eval.send(json!({"mount":mount,"seq":event.seq,"finish":closed}));
        if let Some(action) = action {
            super::super::super::static_select::date_event(state, binding.clone(), mount, action);
        }
        if closed {
            break;
        }
    }
}
