use super::*;
use crate::profile_edit::photo;
pub(super) fn view(state: Signal<ShellState>) -> Element {
    let Some(v) = photo::presentation(&state.read()) else {
        return rsx! {};
    };
    rsx! {PhotoEditor{key:"photo-{v.serial}",state}}
}
#[component]
fn PhotoEditor(state: Signal<ShellState>) -> Element {
    let Some(v) = photo::presentation(&state.read()) else {
        return rsx! {};
    };
    let visit = v.visit;
    let serial = v.serial;
    let preview = v.crop.zip(v.dimensions).map(|(c, (w, h))| {
        let scale = 240.0 / w.max(h) as f64;
        (
            format!("width:{}px;height:{}px", w as f64 * scale, h as f64 * scale),
            format!(
                "left:{}px;top:{}px;width:{}px;height:{}px",
                c.x() as f64 * scale,
                c.y() as f64 * scale,
                c.size() as f64 * scale,
                c.size() as f64 * scale
            ),
            format!(
                "width:{}px;height:{}px;left:-{}px;top:-{}px",
                w as f64 * 72.0 / c.size() as f64,
                h as f64 * 72.0 / c.size() as f64,
                c.x() as f64 * 72.0 / c.size() as f64,
                c.y() as f64 * 72.0 / c.size() as f64
            ),
        )
    });
    rsx! {
        div{class:"scrim",onclick:move |_|photo::cancel(state,visit,serial)}
        section{id:"shell-overlay-dialog",class:"modal profile-edit profile-photo",role:"dialog","aria-modal":"true","aria-labelledby":"profile-photo-title",tabindex:"-1","data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":serial,
            onkeydown:move |e|if e.key()==Key::Escape{e.prevent_default();e.stop_propagation();photo::cancel(state,visit,serial);},
            header{h2{id:"profile-photo-title",if v.remove{"Remove profile photo"}else{"Profile photo"}}
                button{r#type:"button",class:"icon-btn",disabled:v.busy,"aria-label":"Close photo editor",onclick:move |_|photo::cancel(state,visit,serial),{icon(Icon::Close,"icon")}}}
            div{class:"profile-edit-body profile-photo-body",
                if let Some(url)=v.url {
                    if let Some((image_style,crop_style,preview_style))=preview{
                        div{class:"profile-photo-source",style:"{image_style}",
                            img{key:"photo-image-{v.image_key}",src:"{url}",alt:"Selected profile photo",style:"{image_style}",
                                onload:{let key=v.image_key.clone();move |_|photo::decoded(state,visit,serial,&key,true)},
                                onerror:{let key=v.image_key.clone();move |_|photo::decoded(state,visit,serial,&key,false)}}
                            div{class:"profile-photo-crop",style:"{crop_style}","aria-hidden":"true"}
                        }
                        div{class:"profile-photo-preview",img{src:"{url}",style:"{preview_style}",alt:"Square photo preview"}}
                        p{class:"profile-photo-caption","Preview"}
                    }else{img{class:"profile-photo-current",src:"{url}",alt:"Current profile photo"}}
                }
                if v.crop_controls {if let Some(c)=v.crop{if let Some((w,h))=v.dimensions{
                    label{r#for:"profile-photo-size","Crop size"}
                    input{id:"profile-photo-size",r#type:"range",min:"1",max:"{w.min(h)}",value:"{c.size()}",disabled:!v.can_crop,
                        oninput:move |e|photo::crop(state,visit,serial,2,e.value())}
                    label{r#for:"profile-photo-x","Horizontal position"}
                    input{id:"profile-photo-x",r#type:"range",min:"0",max:"{w-c.size()}",value:"{c.x()}",disabled:!v.can_crop,
                        oninput:move |e|photo::crop(state,visit,serial,0,e.value())}
                    label{r#for:"profile-photo-y","Vertical position"}
                    input{id:"profile-photo-y",r#type:"range",min:"0",max:"{h-c.size()}",value:"{c.y()}",disabled:!v.can_crop,
                        oninput:move |e|photo::crop(state,visit,serial,1,e.value())}
                }}}
                if v.busy{p{role:"status","Working on your photo…"}}
                if let Some(message)=v.message{p{id:"profile-photo-notice",class:"profile-edit-notice",role:"status","{message}"}}
                if !v.uncertain&&!v.remove{div{class:"profile-photo-actions",
                    button{id:"profile-photo-choose",r#type:"button",disabled:!v.can_choose,onclick:move |_|photo::choose(state,visit,serial),"Choose photo"}
                    if v.can_remove{button{id:"profile-photo-remove",r#type:"button",onclick:move |_|photo::remove(state,visit,serial),"Remove photo"}}
                }}
            }
            footer{
                if v.can_keep{button{id:"profile-photo-keep",r#type:"button",onclick:move |_|photo::keep(state,visit,serial),"Keep current photo"}}
                button{id:"profile-photo-cancel",r#type:"button",disabled:v.busy,onclick:move |_|photo::cancel(state,visit,serial),if v.uncertain{"Close"}else{"Cancel"}}
                if v.uncertain{button{id:"profile-photo-check",r#type:"button",class:"profile-edit-primary",disabled:!v.can_check,onclick:move |_|photo::check(state,visit,serial),"Check photo"}}
                else if v.can_prepare{button{id:"profile-photo-prepare",r#type:"button",class:"profile-edit-primary",onclick:move |_|photo::prepare(state,visit,serial),"Continue"}}
                else{button{id:"profile-photo-save",r#type:"button",class:"profile-edit-primary",disabled:!v.can_save,onclick:move |_|photo::save(state,visit,serial),if v.remove{"Remove photo"}else{"Save photo"}}}
            }
        }
    }
}
