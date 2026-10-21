use crate::media::MediaUri;
use crate::{
    icons::{Icon, icon},
    state::{ChannelTab, ShellState},
};
use dioxus::prelude::*;

pub fn tabs(state: Signal<ShellState>, selected: ChannelTab) -> Element {
    let custom_scope = format!(
        "{}-{:?}-{:?}-{}",
        state.read().core.account_epoch,
        state.read().core.active_team,
        state.read().core.active_channel,
        state.read().channel_generation
    );
    rsx! {
        nav {class:"channel-tabs","aria-label":"Conversation tabs",
            if crate::app_home::eligible(&state.read()) {
                button {class:if selected==ChannelTab::AppHome {"active"}else{""},"aria-pressed":selected==ChannelTab::AppHome,onclick:move |_|crate::channel_resources::select(state,ChannelTab::AppHome),"Home"}
                button {class:if selected==ChannelTab::AppAbout {"active"}else{""},"aria-pressed":selected==ChannelTab::AppAbout,onclick:move |_|crate::channel_resources::select(state,ChannelTab::AppAbout),"About"}
            }
            button {class:if selected == ChannelTab::Messages {"active"} else {""},"aria-pressed":selected==ChannelTab::Messages,
                onclick:move |_|crate::channel_resources::select(state,ChannelTab::Messages),"Messages"}
            if selected == ChannelTab::Workflows || crate::channel_workflows::available(&state.read()) {
                button {class:if selected==ChannelTab::Workflows {"active"}else{""},"aria-pressed":selected==ChannelTab::Workflows,
                    onclick:move |_|crate::channel_resources::select(state,ChannelTab::Workflows),"Workflows"}
            }
            button {class:if matches!(selected,ChannelTab::Files|ChannelTab::Links) {"active"} else {""},"aria-pressed":matches!(selected,ChannelTab::Files|ChannelTab::Links),
                onclick:move |_|crate::channel_resources::select(state,ChannelTab::Files),"Files & links"}
            button {class:if selected==ChannelTab::Bookmarks {"active"}else{""},"aria-pressed":selected==ChannelTab::Bookmarks,
                onclick:move |_|crate::channel_resources::select(state,ChannelTab::Bookmarks),"Bookmarks"}
            button {class:if selected==ChannelTab::Pins {"active"}else{""},"aria-pressed":selected==ChannelTab::Pins,
                onclick:move |_|crate::channel_resources::select(state,ChannelTab::Pins),"Pins"}
            super::channel_tabs::CustomTabs {key:"{custom_scope}",state}
        }
    }
}

#[component]
pub fn ChannelResources(state: Signal<ShellState>, channel: String) -> Element {
    let selected = state.read().channel_tab;
    rsx! {
        section {class:"channel-resources","aria-label":"Channel files and links",
            nav {class:"resource-tabs","aria-label":"Resource type",
                button {class:if selected==ChannelTab::Files {"active"} else {""},"aria-pressed":selected==ChannelTab::Files,
                    onclick:move |_|crate::channel_resources::select(state,ChannelTab::Files),"Files"}
                button {class:if selected==ChannelTab::Links {"active"} else {""},"aria-pressed":selected==ChannelTab::Links,
                    onclick:move |_|crate::channel_resources::select(state,ChannelTab::Links),"Links"}
                span {class:"spacer"}
                button {onclick:move |_|{dioxus::core::spawn_forever(crate::channel_resources::refresh(state));},"Refresh"}
            }
            if selected == ChannelTab::Files {
                super::files::FilesPanel {key:"files-{channel}",state,channel:Some(channel.clone())}
            } else {
                LinksPanel {key:"links-{channel}",state,channel}
            }
        }
    }
}

#[component]
fn LinksPanel(mut state: Signal<ShellState>, channel: String) -> Element {
    let scope = use_signal(|| channel);
    let mut text = use_signal(|| {
        crate::channel_resources::visible_links(&state.peek(), &scope.peek())
            .map(|l| l.query.clone())
            .unwrap_or_default()
    });
    let snapshot = state.read();
    let links = crate::channel_resources::visible_links(&snapshot, &scope.read());
    let loading = links.is_some_and(|l| l.loading);
    let query = links.map(|l| l.query.clone()).unwrap_or_default();
    let retry = query.clone();
    let previous = query.clone();
    let next = query.clone();
    let page = links.map(|l| l.page).unwrap_or(1).max(1);
    let pages = links.map(|l| l.page_count).unwrap_or(1).max(1);
    rsx! {
        section {class:"files-panel","aria-label":"Shared links",
            form {class:"files-search",onsubmit:move |event|{event.prevent_default();dioxus::core::spawn_forever(crate::channel_resources::load_links(state,scope(),text(),1));},
                {icon(Icon::Search,"icon")}
                input {type:"search","aria-label":"Search links",placeholder:"Search links in this conversation",value:"{text}",oninput:move |event|text.set(event.value())}
                button {type:"submit","Search"}
            }
            div {class:"files-results","aria-busy":loading,
                if let Some(error)=links.and_then(|l|l.error.as_ref()) {
                    div {class:"later-error",role:"alert",p {"{error}"}button {onclick:move |_|{dioxus::core::spawn_forever(crate::channel_resources::load_links(state,scope(),retry.clone(),page));},"Retry"}}
                }
                if loading {p {class:"files-status",role:"status","Loading links…"}}
                if let Some(links)=links {
                    if links.loaded && links.items.is_empty() && !loading {
                        div {class:"files-empty",h3 {"No links found"}p {"Links shared in this conversation appear here."}}
                    }
                    div {role:"list","aria-label":"Link results",
                        for link in &links.items {
                            {
                                let title=link.title.as_deref().filter(|t|!t.is_empty()).unwrap_or(&link.url).to_owned();
                                let url=link.url.clone();
                                let timestamp=link.timestamp.clone();
                                let date=super_platinum_core::state::date_key_for_ts(&timestamp).unwrap_or_default();
                                let key=format!("{}-{}",url,timestamp);
                                let thumbnail=link.thumb_url.as_ref().or(link.icon_url.as_ref()).map(|url|snapshot.media.register_image(super_platinum_core::MediaAssetKind::Attachment,url,"image/png")).filter(|id|snapshot.media.is_ready(id)).map(|id|id.webview_uri_at(snapshot.media_epoch));
                                rsx! {
                                    div {class:"channel-link-row",role:"listitem",key:"{key}",
                                        button {class:"files-row",title:"{url}",onclick:move |_|{if crate::media::open_external(&url).is_err(){state.write().show_toast("This link could not be opened.");}},
                                            if let Some(uri)=thumbnail {span {class:"files-thumbnail",img {src:"{uri}",alt:"",loading:"lazy"}}}
                                            span {class:"files-row-body",span {class:"files-title","{title}"}span {class:"files-meta","{link.url}"}}
                                            span {class:"files-date","{date}"}
                                        }
                                        button {class:"link-message",disabled:timestamp.is_empty(),"aria-label":"Show message containing {title}",
                                            onclick:move |_|{if state.write().open_search_result(&scope(),&timestamp){dioxus::core::spawn_forever(crate::bootstrap::refresh_selected_channel(state));}},
                                            {icon(Icon::Message,"icon")}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            footer {class:"files-pagination",
                span {role:"status",if let Some(links)=links {if links.loaded {"{links.total} " if links.total==1 {"link"} else {"links"} " · Page {page} of {pages}"}}}
                span {class:"spacer"}
                button {disabled:loading||page<=1,onclick:move |_|{dioxus::core::spawn_forever(crate::channel_resources::load_links(state,scope(),previous.clone(),page.saturating_sub(1)));},"Previous"}
                button {disabled:loading||page>=pages,onclick:move |_|{dioxus::core::spawn_forever(crate::channel_resources::load_links(state,scope(),next.clone(),page+1));},"Next"}
            }
        }
    }
}
