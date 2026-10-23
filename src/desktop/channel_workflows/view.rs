use super::*;
use crate::{
    icons::{Icon, icon},
    media::MediaUri,
};
use super_platinum_core::channel_workflows::PreviewKind;

pub(super) fn kind(row: &Row) -> &'static str {
    match row.preview().kind() {
        PreviewKind::Available => "available",
        PreviewKind::AccessDenied => "access-denied",
        PreviewKind::Error(_) => "error",
        PreviewKind::Unknown => "unknown",
        PreviewKind::NonExistent => "nonexistent",
        PreviewKind::Missing => "missing",
    }
}
pub(super) fn title(row: &Row) -> &str {
    if matches!(row.preview().kind(), PreviewKind::AccessDenied) {
        row.title().unwrap_or("Private workflow")
    } else {
        row.title().unwrap_or_default()
    }
}
pub(super) fn description(row: &Row) -> &str {
    if matches!(row.preview().kind(), PreviewKind::AccessDenied) {
        "Only certain people can use this workflow"
    } else {
        row.preview().description().unwrap_or_default()
    }
}

/// 35Tw/ATQ+: smallest provided icon at least 36px, otherwise largest.
/// Only this actual preview's app participates; no invented canonical app facts.
pub(super) fn image_url(app: &Value) -> Option<&str> {
    let icons = app.get("icons")?.as_object()?;
    let mut sizes: Vec<u32> = icons
        .keys()
        .filter_map(|k| {
            k.strip_prefix("image_")
                .or_else(|| k.strip_prefix("image"))?
                .parse()
                .ok()
        })
        .filter(|v| *v > 0)
        .collect();
    sizes.sort_unstable();
    let size = *sizes.iter().find(|v| **v >= 36).or_else(|| sizes.last())?;
    [
        format!("image_{size}"),
        format!("image__{size}"),
        format!("image{size}"),
    ]
    .into_iter()
    .find_map(|k| icons.get(&k)?.as_str().filter(|s| !s.is_empty()))
}

pub(super) fn panel(state: Signal<ShellState>, browse: Signal<Browse>) -> Element {
    let s = state.read();
    let b = browse.read();
    let current = b.current(&s);
    let cooldown = cooling(&s);
    let loading = current && b.status == "loading";
    let ready = current && matches!(b.status, "ready" | "partial");
    let notice = if cooldown.blocked {
        Some("Slack asked us to wait before refreshing workflows.")
    } else if !current {
        Some("Channel details changed. Refresh workflows to continue.")
    } else if b.status == "idle" {
        Some("Retry to load workflows.")
    } else {
        b.notice
    };
    let retry = current && matches!(b.status, "idle" | "failed" | "partial");
    let status = if current { b.status } else { "retired" };
    let probe = (fixture::enabled(&s) || crate::app_home::about::management::fixture::first_publish::confirmation::feature::enabled(&s)).then(|| b.snapshot(&s).to_string());
    let feature_notice = s
        .channel_workflows
        .feature_notice
        .as_ref()
        .filter(|notice| {
            s.core.active_account == notice.account
                && s.core.account_epoch == notice.epoch
                && s.core.active_team.as_ref() == Some(&notice.team)
                && s.core.active_channel.as_ref() == Some(&notice.channel)
        })
        .map(|notice| notice.message);
    let owner = b.owner.as_ref().map(|o| o.id).unwrap_or(0);
    let featured = if ready {
        b.rows.as_ref().map(|r| r.featured()).unwrap_or_default()
    } else {
        &[]
    };
    let bookmarked = if ready {
        b.rows.as_ref().map(|r| r.bookmarked()).unwrap_or_default()
    } else {
        &[]
    };
    rsx! {
        section {id:"channel-workflows-panel",class:"channel-workflows", "aria-label":"Channel workflows",
            "data-owner":owner,"data-status":status,"data-serial":b.serial,
            "data-current":current.to_string(),"data-fixture-readback":probe,
            onmounted:move |_|read::start(state,browse,true),
            header {class:"channel-workflows-heading",
                h2 {"Workflows"}
                button {id:"channel-workflows-refresh",disabled:loading||cooldown.blocked,
                    onclick:move |_|read::start(state,browse,false),"Refresh"}
            }
            if loading {p {id:"channel-workflows-loading",role:"status","Loading workflows\u{2026}"}}
            if let Some(message)=feature_notice {p {id:"channel-workflows-feature-status",role:"status","{message}"}}
            if let Some(notice)=notice {
                div {class:"channel-workflows-notice",role:"status",
                    p {"{notice}"}
                    if retry {button {id:"channel-workflows-retry",disabled:cooldown.blocked,onclick:move |_|read::start(state,browse,false),"Retry"}}
                }
            }
            if ready {
                {collection(state,browse,&s,"featured","Featured workflows","Feature up to 15 workflows, so people can use them instead of sending a message.",featured)}
                {collection(state,browse,&s,"bookmarked","Bookmarked workflows","Help people find and use workflows relevant to this channel.",bookmarked)}
            }
        }
    }
}
fn collection(
    state: Signal<ShellState>,
    browse: Signal<Browse>,
    s: &ShellState,
    key: &str,
    heading: &str,
    subtitle: &str,
    rows: &[Row],
) -> Element {
    let displayed: Vec<_> = rows.iter().filter(|r| r.preview().displayable()).collect();
    let empty = rows.is_empty();
    let unavailable = !empty && displayed.is_empty();
    rsx! {
        section {class:"channel-workflows-collection","aria-labelledby":"channel-workflows-{key}-heading",
            h3 {id:"channel-workflows-{key}-heading","{heading}"}
            p {class:"channel-workflows-subtitle","{subtitle}"}
            div {id:"channel-workflows-{key}",role:"list","aria-label":"{heading}",
                for (index,row) in displayed.into_iter().enumerate() {
                    {item(state,browse,s,key,index,row)}
                }
            }
            if empty {p {class:"channel-workflows-empty","No {key} workflows in this channel."}}
            if unavailable {p {class:"channel-workflows-empty","Workflow details are unavailable."}}
        }
    }
}
fn item(
    state: Signal<ShellState>,
    browse: Signal<Browse>,
    s: &ShellState,
    group: &str,
    index: usize,
    row: &Row,
) -> Element {
    let id = row.preview().id();
    let status = kind(row);
    let label = title(row);
    let detail = description(row);
    let private = status == "access-denied";
    let media = row
        .preview()
        .app()
        .and_then(image_url)
        .map(|url| {
            s.media
                .register_icon(super_platinum_core::MediaAssetKind::Avatar, url)
        })
        .filter(|id| s.media.is_ready(id))
        .map(|id| id.webview_uri_at(s.media_epoch));
    let body = rsx! {
            span {class:"channel-workflow-icon","aria-hidden":"true",
                if private {{icon(Icon::Lock,"icon")}}
                else if let Some(uri)=media {img {src:"{uri}",alt:""}}
                else {{icon(Icon::Play,"icon")}}
            }
            span {class:"channel-workflow-text",
                span {class:"channel-workflow-title",title:label,"{label}"}
                span {class:"channel-workflow-description",title:detail,"{detail}"}
            }
    };
    let group_owned = group.to_owned();
    rsx! {
        div {key:"{group}-{index}-{id}",class:"channel-workflow-row",role:"listitem",
            "data-trigger-id":id,"data-status":status,
            if status=="available" {
                button {id:"channel-workflow-open-{group}-{index}",class:"channel-workflow-open",r#type:"button",
                    onclick:move |_|details::open(state,browse,group_owned.clone(),index),{body}}
            }else{{body}}
        }
    }
}
