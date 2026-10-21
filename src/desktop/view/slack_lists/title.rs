//! Ordinary read-only heading fragments; canonical title remains in presentation.
use crate::{emoji_text::Part, state::ShellState, view::media_image::MediaImage};
use dioxus::prelude::*;

pub(super) fn content(state: Signal<ShellState>, parts: &[Part], epoch: u64) -> Element {
    rsx! {
        for (index,part) in parts.iter().enumerate() {
            {match part {
                Part::Text(text)=>rsx! {
                    span {"data-list-title-kind":"text","data-list-title-index":index,"{text}"}
                },
                Part::Emoji{text,title,image}=>{
                    let ready=image.as_ref().filter(|id|{
                        let s=state.peek();
                        s.media.is_ready(id)&&!s.media.image_state(id).1
                    });
                    match ready {
                        Some(id)=>{
                            let fallback=rsx!{span{class:"custom-emoji",title:title.clone(),"{text}"}};
                            rsx! {
                                span {"data-list-title-kind":"image","data-list-title-index":index,
                                    MediaImage {state,id:id.clone(),media_epoch:epoch,compact:true,class:"emoji-image",alt:text.clone(),title:title.clone().unwrap_or_default(),fallback}
                                }
                            }
                        },
                        None=>rsx! {
                            span {class:"custom-emoji","data-list-title-kind":"emoji","data-list-title-index":index,title:title.clone(),"{text}"}
                        },
                    }
                },
            }}
        }
    }
}
