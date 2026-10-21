use super::*;
use crate::media::MediaUri;

pub(in crate::app_home::about::management::permissions::finish_up) fn surface(
    state: Signal<ShellState>,
    visit: u64,
    id: u64,
    url: &str,
) -> Element {
    rsx! { CropImage{state,visit,owner:id,url:url.to_owned()} }
}

#[component]
fn CropImage(state: Signal<ShellState>, visit: u64, owner: u64, url: String) -> Element {
    let mut local_revision = use_signal(|| 0u64);
    let _ = local_revision();
    let s = state.read();
    let media = s.media.clone();
    let asset = media.register_image(
        super_platinum_core::MediaAssetKind::Attachment,
        &url,
        "image/png",
    );
    let (revision, unavailable) = media.image_state(&asset);
    let ready = media.is_ready(&asset);
    let geometry = local(&s, visit, owner).and_then(|(_, l)| l.avatar.cropper.geometry.clone());
    let bound = local(&s, visit, owner).is_some_and(|(_, l)| l.avatar.cropper.ready);
    drop(s);
    if unavailable {
        return rsx! {p{role:"status","The uploaded image could not be loaded. Go back to choose another image."}};
    }
    if !ready {
        return rsx! {p{role:"status","Loading image\u{2026}"}};
    }
    // Ready crop images use their actual asset revision, so an unrelated media
    // tick cannot replace this original decoded image's URI mid-gesture.
    let source = asset.webview_uri_at(revision.unwrap_or_default());
    let mount_source = source.clone();
    let load_source = source.clone();
    let selection = geometry.as_ref().map(|g| &g["selection"]);
    let n = |key: &str| selection.and_then(|v| v[key].as_f64()).unwrap_or_default();
    let selection_style = format!(
        "left:{}px;top:{}px;width:{}px;height:{}px;visibility:{};",
        n("x"),
        n("y"),
        n("width"),
        n("height"),
        if bound { "visible" } else { "hidden" }
    );
    rsx! {
      div{class:"workflow-finish-crop-editor",
        p{id:"workflow-finish-crop-help","Drag the square or its handles. Use arrow keys for precise adjustments."}
        div{id:"workflow-finish-crop-surface",class:"workflow-finish-crop-surface",
          img{id:"workflow-finish-crop-image",class:"workflow-finish-crop-image",src:source,alt:"Uploaded workflow icon",draggable:"false",
            onmounted:move |_|loaded(state,visit,owner,mount_source.clone()),
            onload:move |_|loaded(state,visit,owner,load_source.clone()),
            onerror:move |_|{if let Some(revision)=revision{if media.reject_image(&asset,revision)||media.image_state(&asset).1{local_revision+=1;}}}}
          div{id:"workflow-finish-crop-selection",class:"workflow-finish-crop-selection",style:selection_style,role:"group",tabindex:if bound{"0"}else{"-1"},"aria-label":"Use the arrow keys to move the crop selection area","aria-describedby":"workflow-finish-crop-help",
            for ord in ["nw","n","ne","e","se","s","sw","w"] {
              span{id:format!("workflow-finish-crop-handle-{ord}"),class:format!("workflow-finish-crop-handle ord-{ord}"),"data-crop-handle":ord,role:"button",tabindex:if bound&&ord.len()==2{"0"}else{"-1"},"aria-label":format!("Use the arrow keys to resize the {ord} crop handle")}
            }
          }
        }
        input{id:"workflow-finish-crop-event",r#type:"text",style:"display:none;",tabindex:"-1","aria-hidden":"true",oninput:move|e|event(state,visit,owner,e.value())}
      }
    }
}
