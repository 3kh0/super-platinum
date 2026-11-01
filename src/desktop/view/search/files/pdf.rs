//! PDF-labelled metadata is a cue only. Original purpose + fresh data gate IO.
use super::*;
use crate::bootstrap::search_file_pages::pdf::{self, Display};
pub(super) fn render(state: Signal<ShellState>, s: &ShellState, index: usize) -> Element {
    let source = pdf::Source::capture(s, index);
    let display = pdf::display(s, index);
    if source.is_none() && display.is_none() && pdf::layout(s, index) {
        return rsx! {
            section{class:"search-file-image search-file-pdf",role:"region","aria-label":"Read-only PDF preview",
                crate::pdf_preview::IsolatedPdfLayout{}
                button{r#type:"button",class:"search-message-more",disabled:true,"Close PDF preview"}
            }
        };
    }
    let Some(display) = display else {
        return source.map(|source|rsx!{
        button{r#type:"button",class:"search-message-more",id:"file-pdf-preview-{index}",disabled:pdf::cooling(s),onclick:move |_|pdf::open(state,source.clone()),"Read PDF"}
    }).unwrap_or_else(||rsx!{});
    };
    let close = source.clone();
    let retry = source.clone();
    let admit = source.clone();
    let failed = source.clone();
    let busy = matches!(
        display,
        Display::Loading | Display::Document { ready: false, .. }
    );
    let can_retry = matches!(display, Display::Failed | Display::Unavailable);
    rsx! {section{class:"search-file-image search-file-pdf",role:"region","aria-label":"Read-only PDF preview","aria-busy":busy,
        match display{
            Display::Loading=>rsx!{p{role:"status","Loading PDF preview…"}},
            Display::Unavailable=>rsx!{p{role:"status","This PDF is not available to preview."}},
            Display::Failed=>rsx!{p{role:"status","PDF preview couldn't load. Your file results are retained."}},
            Display::Document{id,nonce,..}=>rsx!{crate::pdf_preview::IsolatedPdf{
                key:"{nonce}",id,nonce,
                admit:move|decoded:crate::pdf_preview::Decoded|admit.clone().is_some_and(|source|pdf::decoded(state,source,decoded.pages,decoded.native)),
                failed:move |_|{if let Some(source)=failed.clone(){pdf::failed(state,source);}}
            }},
        }
        if can_retry{
            button{r#type:"button",class:"search-message-more",id:"file-pdf-retry-{index}",disabled:retry.is_none()||pdf::cooling(s),onclick:move |_|{if let Some(source)=retry.clone(){pdf::open(state,source);}},"Retry PDF preview"}
        }
        button{r#type:"button",class:"search-message-more",id:"file-pdf-close-{index}",disabled:close.is_none(),onclick:move |_|{if let Some(source)=close.clone(){pdf::close(state,source);}},"Close PDF preview"}
    }}
}
