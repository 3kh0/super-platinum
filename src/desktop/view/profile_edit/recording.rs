use super::*;
use crate::profile_edit::recording as controller;
const BRIDGE: &str = include_str!("recording/bridge.js");
pub(super) fn view(state: Signal<ShellState>) -> Element {
    let Some(v) = controller::presentation(&state.read()) else {
        return rsx! {};
    };
    rsx! {RecordingEditor{key:"recording-{v.visit}-{v.serial}",state}}
}
#[component]
fn RecordingEditor(state: Signal<ShellState>) -> Element {
    let mut leaving = use_signal(|| false);
    let Some(v) = controller::presentation(&state.read()) else {
        return rsx! {};
    };
    let (visit, serial) = (v.visit, v.serial);
    let fixture = v.fixture;
    let device = v.device.clone();
    let capturing = matches!(
        v.phase.as_str(),
        "permission" | "recording" | "stopping" | "retaining"
    );
    let local = v.phase == "preview";
    let local_audio = (local && v.url.is_none()) || (v.phase == "uploaded" && v.url.is_none());
    let dirty = v.dirty || capturing;
    rsx! {
        div{class:"scrim",onclick:move |_|if !v.busy{if dirty{leaving.set(true);}else{controller::back(state,visit,serial);}}}
        section{id:"shell-overlay-dialog",class:"modal profile-edit",role:"dialog","aria-modal":"true","aria-labelledby":"profile-recording-title",
            tabindex:"-1","data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":serial,
            onkeydown:move |e|if e.key()==Key::Escape{e.prevent_default();e.stop_propagation();if !v.busy{if leaving(){leaving.set(false);}else if dirty{leaving.set(true);}else{controller::back(state,visit,serial);}}},
            header{h2{id:"profile-recording-title","Name recording"}}
            div{class:"profile-edit-body",id:"profile-recording-owner","data-recording-owner":serial,
                onmounted:move |_|if !fixture{mount(state,visit,serial);},
                p{class:"profile-edit-hint","Record how to say your name. Up to 10 seconds."}
                label{r#for:"profile-recording-microphone","Microphone"}
                select{id:"profile-recording-microphone",value:v.device.clone(),disabled:!v.can_record||leaving(),onchange:move |e|controller::choose_microphone(state,visit,serial,e.value()),
                    option{value:"","System default"}
                    if !v.device.is_empty()&&!v.devices.iter().any(|d|d.id==v.device){option{value:v.device.clone(),"Previously selected microphone"}}
                    for d in v.devices{option{key:"{d.id}",value:d.id,"{d.label}"}}
                }
                canvas{width:"300",height:"48",style:"width:100%;height:48px",role:"img","aria-label":"Microphone input waveform","data-recording-waveform":"true"}
                audio{id:"profile-recording-local",hidden:true,controls:false,preload:"metadata","data-recording-local":"true",onerror:move |_|controller::media_failed(state,visit,serial)}
                audio{id:"profile-recording-remote",hidden:true,controls:false,preload:"metadata",src:v.url.clone(),onerror:move |_|controller::media_failed(state,visit,serial)}
                button{r#type:"button",disabled:capturing||(!local&&!v.file),onclick:move |_|play(serial,if local_audio&&!fixture{"profile-recording-local"}else{"profile-recording-remote"}),"Play / pause recording"}
                if capturing{
                    p{role:"status",class:if v.elapsed>=5000{"profile-edit-notice"}else{"profile-edit-hint"},
                        if v.phase=="permission"{"Waiting for microphone permission…"}else{ "{v.elapsed / 1000} / 10 seconds" }}
                    button{r#type:"button",disabled:v.phase!="recording",onclick:move |_|command(serial,"stop"),"Stop recording"}
                    button{r#type:"button",onclick:move |_|command(serial,"cancel"),"Cancel recording"}
                }else{
                    button{id:"profile-recording-start",r#type:"button",disabled:!v.can_record||leaving(),onclick:move |_|{
                        if fixture{controller::fixture::preview(state,visit,serial);}else if controller::start_capture(state,visit,serial){start(serial,&device);}
                    },if local||v.file{"Record again"}else{"Record audio clip"}}
                    if local{
                        button{id:"profile-recording-upload",r#type:"button",disabled:!v.can_record||leaving(),onclick:move |_|{
                            if fixture{controller::fixture::upload(state,visit,serial);}else{controller::upload_retained(state,visit,serial);}
                        },"Use recording"}
                        button{r#type:"button",disabled:v.busy||v.uncertain,onclick:move |_|{command(serial,"cancel");controller::cancel_capture(state,visit,serial);},"Discard recording"}
                    }
                    if v.file{
                        button{id:"profile-recording-remove",r#type:"button",disabled:!v.can_remove||local,onclick:move |_|controller::remove(state,visit,serial),"Remove recording"}
                    }
                }
                if let Some(message)=v.message{p{class:"profile-edit-notice",role:"status","{message}"}}
                if v.storage_error{button{r#type:"button",disabled:v.busy,onclick:move |_|controller::storage::retry(state,visit,serial),"Retry local recovery storage"}}
                if v.can_cleanup{button{r#type:"button",onclick:move |_|if fixture{controller::fixture::cleanup(state,visit,serial);}else{controller::cleanup(state,visit,serial);},"Discard uploaded recording"}}
                if v.can_cancel_upload{button{r#type:"button",onclick:move |_|controller::cancel::request(state,visit,serial),"Cancel upload"}}
                if v.busy{p{role:"status","Checking your recording…"}}
                if leaving(){p{role:"alert","Close this editor? Your local recording and unfinished operation will be kept."}}
            }
            footer{
                if leaving(){
                    button{r#type:"button",onclick:move |_|leaving.set(false),"Keep editing"}
                    button{r#type:"button",onclick:move |_|{command(serial,"cancel");controller::back(state,visit,serial);},"Close editor"}
                }else{
                    button{id:"profile-recording-back",r#type:"button",disabled:v.busy,onclick:move |_|if dirty{leaving.set(true);}else{controller::back(state,visit,serial);},"Back"}
                    if v.can_keep{button{r#type:"button",onclick:move |_|{command(serial,"cancel");controller::keep(state,visit,serial);},"Keep current profile"}}
                    if v.uncertain{
                        button{id:"profile-recording-check",r#type:"button",disabled:!v.can_check,onclick:move |_|if fixture{controller::fixture::check(state,visit,serial);}else{controller::check(state,visit,serial);},"Check recording"}
                    }else{
                        button{id:"profile-recording-save",r#type:"button",class:"profile-edit-primary",disabled:!v.can_save,onclick:move |_|if fixture{controller::fixture::save(state,visit,serial);}else{controller::save(state,visit,serial);},"Save recording"}
                    }
                }
            }
        }
    }
}
fn mount(state: Signal<ShellState>, visit: u64, serial: u64) {
    spawn(async move {
        let mut eval = dioxus::document::eval(&format!(
            r#"
            const root=document.getElementById('profile-recording-owner');
            if(root?.dataset.recordingOwner==='{serial}'){{
              window.__spNameRecording?.api.dispose();
              const api=({BRIDGE})(root,'{serial}',event=>dioxus.send(event));
              window.__spNameRecording={{owner:'{serial}',root,api}};api.devices();
              await api.closed;
              if(window.__spNameRecording?.api===api)delete window.__spNameRecording;
            }}"#
        ));
        while let Ok(event) = eval.recv::<controller::CaptureEvent>().await {
            controller::capture_event(state, visit, serial, event);
        }
    });
}
fn command(serial: u64, command: &str) {
    if !matches!(command, "start" | "stop" | "cancel") {
        return;
    }
    dioxus::document::eval(&format!(
        "const r=window.__spNameRecording;if(r?.owner==='{serial}'&&r.root.isConnected){{r.api.{command}();}}"
    ));
}
fn capture(state: Signal<ShellState>, visit: u64, serial: u64) {
    spawn(async move {
        let mut eval = dioxus::document::eval(&format!(
            "const r=window.__spNameRecording;dioxus.send(r?.owner==='{serial}'?await r.api.capture():null);"
        ));
        if let Ok(Ok(Some(audio))) = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            eval.recv::<Option<controller::CapturedAudio>>(),
        )
        .await
        {
            controller::upload(state, visit, serial, audio);
        }
    });
}

fn play(serial: u64, id: &str) {
    dioxus::document::eval(&format!(
        "const root=document.getElementById('profile-recording-owner');if(root?.dataset.recordingOwner==='{serial}'){{const a=root.querySelector('#{id}');if(a){{if(a.paused){{a.play().catch(()=>a.dispatchEvent(new Event('error')));}}else{{a.pause();}}}}}}"
    ));
}

fn start(serial: u64, device: &str) {
    let device = serde_json::to_string(device).unwrap();
    dioxus::document::eval(&format!(
        "const r=window.__spNameRecording;if(r?.owner==='{serial}'&&r.root.isConnected){{r.api.start({device});}}"
    ));
}
