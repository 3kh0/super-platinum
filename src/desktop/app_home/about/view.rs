use super::*;
use crate::media::MediaUri;
use serde_json::Value;
use super_platinum_core::MediaAssetKind;

pub(crate) fn panel(mut state: Signal<ShellState>) -> Element {
    let s = state.read();
    let Some(h) = s
        .app_home
        .about
        .surface
        .as_ref()
        .filter(|h| h.owner.current(&s))
    else {
        return rsx! {section{class:"app-home app-about",h2{"About"}button{onclick:move |_|{dioxus::core::spawn_forever(refresh(state));},"Load About"}}};
    };
    let key = Key {
        id: h.id,
        revision: h.revision,
    };
    let info = h.info.clone();
    let ai = h.info.as_ref().is_some_and(|i| {
        i.raw()["is_ai_app"] == true
            || i.raw()["is_agent_app"] == true
            || h.profile.as_ref().is_some_and(|p| p.is_ai(i))
    });
    let permissions = h
        .info
        .as_ref()
        .zip(h.profile.as_ref())
        .zip(h.flags)
        .is_some_and(|((i, p), f)| {
            p.show_permissions(
                i,
                f,
                h.owner
                    .ctx
                    .session
                    .enterprise_id
                    .as_deref()
                    .unwrap_or(&h.owner.ctx.session.team_id),
            )
        });
    let loading = h.loading;
    let notice = h.notice.clone();
    let enabled = actions::enabled(&s, key);
    let connection = h.connection.clone();
    let action_status = actions::status(&s);
    let cooldown = cooling(&s, &h.owner.ctx);
    let configuration = profile_link(&s, key, ProfileLink::Configuration).is_some();
    let report = profile_link(&s, key, ProfileLink::Report).is_some();
    let profile_links_disabled = loading || h.fixture || s.overlay.is_some();
    let epoch = s.media_epoch;
    let vip_user = h
        .info
        .as_ref()
        .and_then(|i| i.raw()["app_user"]["id"].as_str())
        .filter(|u| *u == h.owner.bot)
        .map(str::to_owned);
    let is_vip = vip_user.as_ref().is_some_and(|u| {
        s.core.workspaces[&h.owner.ctx.session.team_id]
            .vip_users
            .contains(u)
    });
    let vip_disabled = h.fixture
        || loading
        || s.overlay.is_some()
        || vip_user
            .as_ref()
            .is_some_and(|u| crate::profile_vip::busy(&s, u));
    let screenshots: Vec<_> = info
        .as_ref()
        .and_then(|i| i.raw()["screenshots"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| {
            let url = v.get("image_440").and_then(Value::as_str)?;
            let allowed = external(v.get("image_1600").and_then(Value::as_str)?)?;
            let thumbnail = external(url)?;
            let media = s
                .media
                .register_image(MediaAssetKind::Attachment, &thumbnail, "image/png");
            let ready = s.media.is_ready(&media);
            Some((allowed, media, ready))
        })
        .collect();
    let icon = info
        .as_ref()
        .and_then(|i| i.raw().get("icons"))
        .and_then(|v| v.get("image_72").or_else(|| v.get("image_48")))
        .and_then(Value::as_str)
        .and_then(external)
        .map(|url| {
            let media = s.media.register_icon(MediaAssetKind::Avatar, &url);
            let ready = s.media.is_ready(&media);
            (media, ready)
        });
    drop(s);
    let title = info
        .as_ref()
        .and_then(|i| i.text("name"))
        .unwrap_or("About this app")
        .to_owned();
    rsx! {section{key:"about-{key.id}",id:"app-about",class:"app-home app-about","data-visit":key.id,"data-revision":key.revision,"aria-label":"App About",
      div{class:"app-home-toolbar",
        if let Some((media,ready))=icon{if ready{img{class:"app-about-icon",src:media.webview_uri_at(epoch),alt:""}}}
        h2{tabindex:"-1","{title}"}span{class:"spacer"}
        button{disabled:loading||cooldown,onclick:move |_|{if current(&state.peek(),key){dioxus::core::spawn_forever(refresh(state));}},"Refresh"}
      }
      div{class:"app-home-content app-about-content","aria-busy":loading,
        if loading{p{role:"status","Loading About…"}}
        if let Some(info)=info{
          if let Some(connection)=connection{
            div{class:"app-about-connection",
              p{role:"status",{connection_label(connection.status())}}
              if connection.approval()==Some("pending"){p{"Connection approval is pending."}}
              if connection.approval()==Some("denied")&&!matches!(connection.status(),"connected"|"dwa_connected"){p{"Connection approval was denied."}}
              if connection.can_connect(true)||connection.can_manage(true){button{disabled:!enabled,onclick:move |_|actions::launch(state,key),if connection.can_manage(true){"Manage connection"}else{"Connect"}}}
            }
          }
          if let Some((id,unknown))=action_status{
            div{role:"status",class:"app-home-notice",if unknown{"The connection request is unconfirmed and will not be replayed."}else{"Waiting for the app…" button{onclick:move |_|actions::cancel(&mut state.write(),id),"Cancel"}}}
          }
          if ai { if let Some(body)=info.text("assistant_description"){section{h3{"What can this app do?"}{description::render(state,key,"assistant",body)}}}}
          if let Some(body)=info.text("long_description"){section{h3{"About this app"}{description::render(state,key,"about",body)}}}
          if info.text("long_description").is_none()&&(!ai||info.text("assistant_description").is_none()){p{"This app has no description."}}
          if let Some(user)=vip_user{button{disabled:vip_disabled,onclick:move |_|{
            let allowed=current(&state.peek(),key)&&state.peek().overlay.is_none(); if allowed{crate::bootstrap::toggle_profile_vip(state,user.clone());}
          },if is_vip{"Remove from VIP"}else{"Add to VIP"}}}
          {features(&info)}
          {mcp_tools(&info)}
          if permissions { for (optional,label) in [(false,"Required authorizations"),(true,"Optional authorizations")]{
            for kind in ["content","actions"]{if let Some(block)=info.permissions(kind,optional){
              section{h3{"{label}"}h4{"{block.heading}"}for group in block.groups{div{h4{"{group.heading}"}ul{for description in group.descriptions{li{"{description}"}}}}}}
            }}
          }
          }
          {security(&info)}
          if !screenshots.is_empty(){section{h3{"Screenshots"}div{class:"app-about-screenshots",for (url,media,ready) in screenshots{
            if ready{button{class:"app-about-screenshot",onclick:move |_|open_external(state,key,&url),img{src:media.webview_uri_at(epoch),alt:"App screenshot"}}}else{div{class:"app-about-screenshot-placeholder",role:"status","Loading screenshot…"}}
          }}}}
          section{class:"app-about-links",h3{"Learn more and support"}
            if configuration{button{disabled:profile_links_disabled,onclick:move |_|open_profile_link(state,key,ProfileLink::Configuration),"Configuration"}}
            for (field,label) in [("support_url","Get app support"),("home_url","Visit developer website"),("privacy_link","Privacy policy"),("youtube_url","Watch app video")]{
              if let Some(url)=info.text(field).and_then(external){button{onclick:move |_|open_external(state,key,&url),"{label}"}}
            }
            if let Some(email)=info.text("support_email"){p{"Support email: {email}"}}
            if let Some(user)=info.raw()["app_user"]["username"].as_str(){p{"Mention: @{user}"}}
            {string_list(info.raw(),"supported_languages","Supported languages")}
            if let Some(label)=pricing(info.raw()["pricing_info"].as_str()){p{"Pricing: {label}"}}
            if report{button{disabled:profile_links_disabled,onclick:move |_|open_profile_link(state,key,ProfileLink::Report),"Report this app"}}
          }
        }
        if let Some(notice)=notice{p{role:"status",class:"app-home-notice","{notice}"}}
      }
    }}
}
#[derive(Clone, Copy)]
enum ProfileLink {
    Configuration,
    Report,
}
fn profile_link(s: &ShellState, key: Key, action: ProfileLink) -> Option<String> {
    if !current(s, key) {
        return None;
    }
    let h = s.app_home.about.surface.as_ref()?;
    let ctx = &h.owner.ctx;
    match action {
        ProfileLink::Report => h.info.as_ref()?.report_url(&ctx.session.url),
        ProfileLink::Configuration => {
            let member = s
                .core
                .workspaces
                .get(&ctx.session.team_id)?
                .users
                .get(&ctx.session.user_id)?;
            h.profile.as_ref()?.configuration_url(
                &ctx.session.url,
                ctx.session
                    .enterprise_id
                    .as_deref()
                    .unwrap_or(&ctx.session.team_id),
                member.is_restricted,
            )
        }
    }
}
fn open_profile_link(state: Signal<ShellState>, key: Key, action: ProfileLink) {
    let url = {
        let s = state.peek();
        if s.overlay.is_some()
            || s.app_home
                .about
                .surface
                .as_ref()
                .is_none_or(|h| h.loading || h.fixture)
        {
            return;
        }
        profile_link(&s, key, action)
    };
    if let Some(url) = url {
        crate::slack_links::open(state, &url);
    }
}
fn open_external(state: Signal<ShellState>, key: Key, url: &str) {
    let allowed = current(&state.peek(), key) && state.peek().overlay.is_none();
    if allowed {
        crate::slack_links::open(state, url);
    }
}
fn external(value: &str) -> Option<String> {
    let u = url::Url::parse(value).ok()?;
    (matches!(u.scheme(), "http" | "https")
        && u.host_str().is_some()
        && u.username().is_empty()
        && u.password().is_none())
    .then(|| u.to_string())
}
fn connection_label(status: &str) -> &str {
    match status {
        "connected" | "dwa_connected" => "Connected",
        "disconnected" => "Not connected",
        "ema_configured" => "Connection managed by your organization",
        _ => "Connection status unavailable",
    }
}
fn string_list(raw: &Value, key: &str, label: &str) -> Element {
    let items: Vec<_> = match &raw[key] {
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
        Value::Object(o) => o.values().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    rsx! {if !items.is_empty(){div{h4{"{label}"}p{{items.join(", ")}}}}}
}
fn features(info: &Info) -> Element {
    let raw = info.raw();
    let items: Vec<(String, String)> = raw["shortcuts"]["global_shortcuts"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(
            raw["shortcuts"]["message_shortcuts"]
                .as_array()
                .into_iter()
                .flatten(),
        )
        .chain(raw["slash_commands"].as_array().into_iter().flatten())
        .filter_map(|v| {
            Some((
                v.get("name")?.as_str()?.into(),
                v.get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
            ))
        })
        .collect();
    rsx! {if !items.is_empty()||raw["is_calls_app"]==true||raw["is_org_ready_app"]==true{section{h3{"Features"}ul{for (name,description) in items{li{strong{"{name}"}p{"{description}"}}}if raw["is_calls_app"]==true{li{"Calls"}}if raw["is_org_ready_app"]==true{li{"Available across your organization"}}}}}}
}
fn security(info: &Info) -> Element {
    let rows: Vec<_> = info.raw()["security_compliance"]
        .as_object()
        .into_iter()
        .flat_map(|v| v.iter())
        .filter_map(|(key, v)| {
            let value = match v {
                Value::String(s) if !s.trim().is_empty() => s.clone(),
                Value::Bool(v) => v.to_string(),
                Value::Number(v) => v.to_string(),
                _ => return None,
            };
            Some((key.replace('_', " "), value))
        })
        .collect();
    let rotation = info.raw()["security_token_rotation_enabled"].as_bool();
    rsx! {if !rows.is_empty()||rotation.is_some(){section{h3{"Security and compliance"}dl{for(label,value)in rows{dt{"{label}"}dd{"{value}"}}if let Some(enabled)=rotation{dt{"Token rotation"}dd{if enabled{"Enabled"}else{"Not enabled"}}}}}}}
}

fn pricing(value: Option<&str>) -> Option<&'static str> {
    match value? {
        "free" => Some("Free"),
        "freemium" => Some("Free and paid plans available"),
        "paid_free_trial" => Some("Paid with free trial"),
        "paid" => Some("Paid"),
        "included_with_subscription" => Some("Included with service subscription"),
        _ => None,
    }
}

fn mcp_tools(info: &Info) -> Element {
    let servers: Vec<_> = info.raw()["mcp_servers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let name = s["name"].as_str()?;
            let tools: Vec<_> = s["tools_snapshot"]["tools"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| {
                    t["title"]
                        .as_str()
                        .or_else(|| t["annotations"]["title"].as_str())
                        .or_else(|| t["name"].as_str())
                })
                .collect();
            (!tools.is_empty()).then_some((name, tools))
        })
        .collect();
    rsx! {
        if !servers.is_empty() {
            section {
                h3 { "Tools" }
                for (name,tools) in servers {
                    h4 { "{name}" }
                    ul { for tool in tools { li { "{tool}" } } }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_links_recheck_current_member_and_exact_about_owner() {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install(&mut s, false);
        let h = s.app_home.about.surface.as_ref().unwrap();
        let key = Key {
            id: h.id,
            revision: h.revision,
        };
        let owner = h.owner.clone();
        assert!(profile_link(&s, key, ProfileLink::Report).is_some());
        assert!(profile_link(&s, key, ProfileLink::Configuration).is_some());
        s.core
            .workspaces
            .get_mut(&owner.ctx.session.team_id)
            .unwrap()
            .users
            .get_mut(&owner.ctx.session.user_id)
            .unwrap()
            .is_restricted = true;
        assert!(profile_link(&s, key, ProfileLink::Configuration).is_none());
        assert!(profile_link(&s, key, ProfileLink::Report).is_some());
        s.channel_generation += 1;
        assert!(profile_link(&s, key, ProfileLink::Configuration).is_none());
        assert!(profile_link(&s, key, ProfileLink::Report).is_none());
    }
}
