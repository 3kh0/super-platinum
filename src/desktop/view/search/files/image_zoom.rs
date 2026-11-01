//! Local presentation of an already admitted, ready opaque PNG. No file actions.
use crate::media::MediaUri;
use dioxus::prelude::*;
use super_platinum_core::MediaAssetId;

pub(super) fn render(key: &str, image: &MediaAssetId, epoch: u64) -> Element {
    let src = image.webview_uri_at(epoch);
    let base = src.split('?').next().unwrap_or_default().to_owned();
    let mount_key = key.to_owned();
    rsx! {div {
        key:"{key}",id:"{key}",class:"image-zoom","data-image-zoom-source":base,
        onmounted:move |_| {
            let id=serde_json::to_string(&mount_key).unwrap();
            let _=dioxus::document::eval(&format!("{}\ninstallNativeImageZoom({id});",include_str!("image_zoom.js")));
        },
        div {class:"image-zoom-viewport","data-image-zoom-viewport":"true",
            div {class:"image-zoom-canvas","data-image-zoom-canvas":"true",
                img {src,alt:"File image preview",draggable:"false","data-image-zoom-image":"true"}
            }
        }
        div {class:"image-zoom-controls",
            button {r#type:"button",class:"search-message-more image-zoom-step","data-image-zoom-minus":"true",disabled:true,
                "aria-label":"Zoom out",title:"Zoom out",{crate::icons::icon(crate::icons::Icon::Remove,"icon")}}
            button {r#type:"button",class:"search-message-more","data-image-zoom-fit":"true",disabled:true,
                "aria-label":"Fit image to preview","Fit"}
            button {r#type:"button",class:"search-message-more image-zoom-step","data-image-zoom-plus":"true",disabled:true,
                "aria-label":"Zoom in",title:"Zoom in",{crate::icons::icon(crate::icons::Icon::Add,"icon")}}
            input {r#type:"range",min:"0",max:"1",step:"0.01",value:"0",disabled:true,
                "data-image-zoom-slider":"true","aria-label":"Image zoom"}
            output {"data-image-zoom-label":"true","aria-label":"Image scale","Fit"}
        }
    }}
}
