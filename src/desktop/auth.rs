use std::collections::BTreeMap;
use std::sync::mpsc;

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::window::WindowBuilder;
use wry::http::Request;
use wry::{WebContext, WebViewBuilder};

use crate::config::{self, Session, WorkspaceSession};
const LOGIN_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";
const SLACK_WEB_URL: &str = "https://app.slack.com/";

enum LoginEvent {
    LocalConfig(String),
    MagicLoginAccepted,
    MagicLoginFailed(String),
}

const SCR: &str = r#"
(function () {
  if (window.__super-platinumGrab) return;
  window.__super-platinumGrab = true;
  function ipc(m) { try { window.ipc.postMessage(m); } catch (e) {} }
  ipc('LOG: loaded ' + location.origin + location.pathname);
  if (location.pathname.indexOf('/api/auth.loginMagicBulk') !== -1) {
    function readMagicResponse() {
      try {
        var response = JSON.parse(document.body.innerText);
        ipc(response.ok ? 'MAGIC_OK' : 'MAGIC_ERR:' + (response.error || 'unknown error'));
      } catch (e) { ipc('MAGIC_ERR:invalid response'); }
    }
    if (document.readyState === 'loading') addEventListener('DOMContentLoaded', readMagicResponse);
    else readMagicResponse();
    return;
  }
  var tries = 0;
  var iv = setInterval(function () {
    tries++;
    try {
      var lc = localStorage.getItem('localConfig_v2');
      if (lc && lc.indexOf('xoxc-') !== -1) { clearInterval(iv); ipc('CFG:' + lc); return; }
    } catch (e) { ipc('LOG: err ' + e); }
    if (tries > 240) {
      clearInterval(iv);
      ipc('MAGIC_ERR:timed out waiting for Slack session');
    }
  }, 700);
})();
"#;

pub fn login(fresh_profile: bool) -> Result<Session, String> {
    drive_login(fresh_profile, SLACK_WEB_URL.to_owned(), true)
}

pub fn login_magic(url: &str) -> Result<Session, String> {
    let request_url = magic_login_request_url(url)?;
    drive_login(true, request_url, false)
}

pub fn is_magic_login_url(url: &str) -> bool {
    magic_login_request_url(url).is_ok()
}

#[allow(dead_code)]
const CAPTURE_SCR: &str = r#"
(function () {
  if (window.__super-platinumCap) return;
  window.__super-platinumCap = true;
  function ipc(m) { try { window.ipc.postMessage(m); } catch (e) {} }
  var RE = /\/api\/(rooms|screenhero|huddles|calls)\./;
  function emit(url, req, res) {
    if (!RE.test(url)) return;
    try {
      ipc('CAP:' + JSON.stringify({
        url: String(url),
        req: String(req || '').slice(0, 2000),
        res: String(res || '').slice(0, 6000),
      }));
    } catch (e) {}
  }
  var of = window.fetch;
  window.fetch = function (input, init) {
    var url = (typeof input === 'string') ? input : (input && input.url) || '';
    var body = (init && init.body) || (input && input.body) || '';
    return of.apply(this, arguments).then(function (res) {
      try { res.clone().text().then(function (t) { emit(url, body, t); }); } catch (e) {}
      return res;
    });
  };
  var oOpen = XMLHttpRequest.prototype.open;
  var oSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function (m, url) { this.__u = url; return oOpen.apply(this, arguments); };
  XMLHttpRequest.prototype.send = function (body) {
    var xhr = this;
    xhr.addEventListener('load', function () { emit(xhr.__u, body, xhr.responseText); });
    return oSend.apply(this, arguments);
  };
  var OW = window.WebSocket;
  window.WebSocket = function (url, protocols) {
    ipc('WS:' + url);
    return protocols === undefined ? new OW(url) : new OW(url, protocols);
  };
  window.WebSocket.prototype = OW.prototype;
  ipc('LOG: capture hooks installed on ' + location.href);
})();
"#;

#[allow(dead_code)]
pub fn capture_huddle_webview() -> Result<(), String> {
    let mut event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("super-platinum huddle capture — join/leave a huddle, then close")
        .with_inner_size(LogicalSize::new(1100.0, 800.0))
        .build(&event_loop)
        .map_err(|e| format!("window: {e}"))?;

    let data_dir = config::config_dir()
        .map_err(|e| format!("config dir: {e}"))?
        .join("webview");
    let _ = std::fs::create_dir_all(&data_dir);
    let mut web_context = WebContext::new(Some(data_dir));

    let _webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url("https://app.slack.com/")
        .with_user_agent(LOGIN_USER_AGENT)
        .with_initialization_script(CAPTURE_SCR)
        .with_ipc_handler(|req: Request<String>| {
            let body = req.into_body();
            if let Some(cap) = body.strip_prefix("CAP:") {
                println!("===== huddle api call =====");
                println!("{}", crate::slack::client::redact_secrets(cap));
                println!("===== end =====");
            } else if let Some(ws) = body.strip_prefix("WS:") {
                println!("[websocket] {}", crate::slack::client::redact_secrets(ws));
            } else if let Some(log) = body.strip_prefix("LOG:") {
                eprintln!("super-platinum huddle-webview [webview]:{log}");
            }
        })
        .with_navigation_handler(|url| !url.starts_with("slack://"))
        .with_devtools(true)
        .build(&window)
        .map_err(|e| format!("webview: {e}"))?;

    eprintln!(
        "super-platinum huddle-webview: open a channel, start/join a huddle, then leave and \
         close this window. Huddle API calls + the media socket URL will print here."
    );

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
    Ok(())
}

fn drive_login(fresh_profile: bool, initial_url: String, visible: bool) -> Result<Session, String> {
    let mut event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Sign in to Slack")
        .with_inner_size(LogicalSize::new(520.0, 720.0))
        .with_visible(visible)
        .build(&event_loop)
        .map_err(|e| format!("window: {e}"))?;

    let data_dir = config::config_dir()
        .map_err(|e| format!("config dir: {e}"))?
        .join(if fresh_profile {
            "webview-add-account"
        } else {
            "webview"
        });
    if fresh_profile {
        match std::fs::remove_dir_all(&data_dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("reset add-account webview: {error}")),
        }
    }
    std::fs::create_dir_all(&data_dir).map_err(|error| format!("create webview data: {error}"))?;
    let mut web_context = WebContext::new(Some(data_dir));

    let (tx, rx) = mpsc::channel::<LoginEvent>();
    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(&initial_url)
        .with_user_agent(LOGIN_USER_AGENT)
        .with_initialization_script(SCR)
        .with_ipc_handler(move |req: Request<String>| {
            let body = req.into_body();
            if let Some(cfg) = body.strip_prefix("CFG:") {
                let _ = tx.send(LoginEvent::LocalConfig(cfg.to_owned()));
            } else if body == "MAGIC_OK" {
                let _ = tx.send(LoginEvent::MagicLoginAccepted);
            } else if let Some(error) = body.strip_prefix("MAGIC_ERR:") {
                let _ = tx.send(LoginEvent::MagicLoginFailed(error.to_owned()));
            } else if let Some(log) = body.strip_prefix("LOG:") {
                eprintln!("super-platinum auth [webview]:{log}");
            }
        })
        .with_navigation_handler(|url| !url.starts_with("slack://"))
        .with_devtools(true)
        .build(&window)
        .map_err(|e| format!("webview: {e}"))?;

    eprintln!("super-platinum auth: waiting for the Slack session to boot…");

    let mut result: Option<Result<Session, String>> = None;

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Ok(event) = rx.try_recv() {
            match event {
                LoginEvent::MagicLoginAccepted => {
                    eprintln!("super-platinum auth: magic login accepted; loading workspace…");
                    if let Err(error) = webview.load_url(SLACK_WEB_URL) {
                        result = Some(Err(format!("load signed-in Slack workspace: {error}")));
                        *control_flow = ControlFlow::Exit;
                    }
                }
                LoginEvent::MagicLoginFailed(error) => {
                    result = Some(Err(format!("Slack rejected magic login: {error}")));
                    *control_flow = ControlFlow::Exit;
                }
                LoginEvent::LocalConfig(local_config) => {
                    eprintln!(
                        "super-platinum auth: received localConfig ({} bytes)",
                        local_config.len()
                    );
                    let d_cookie = harvest_d_cookie(&webview);
                    result = Some(match d_cookie {
                        Some(d) => session_from_localconfig(&local_config, d),
                        None => Err(
                            "no `d` cookie found in the webview (is the client fully signed in?)"
                                .to_owned(),
                        ),
                    });
                    *control_flow = ControlFlow::Exit;
                }
            }
        }

        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            if result.is_none() {
                result = Some(Err(
                    "login window closed before completing sign-in".to_owned()
                ));
            }
            *control_flow = ControlFlow::Exit;
        }
    });

    result.unwrap_or_else(|| Err("login window closed before completing sign-in".to_owned()))
}

fn magic_login_request_url(deep_link: &str) -> Result<String, String> {
    let url = url::Url::parse(deep_link).map_err(|_| "invalid Slack sign-in link".to_owned())?;
    let path = url
        .path_segments()
        .map(Iterator::collect::<Vec<_>>)
        .unwrap_or_default();
    if url.scheme() != "slack" || path.len() != 2 || path.first() != Some(&"magic-login") {
        return Err("unsupported Slack link".to_owned());
    }
    let team = url
        .host_str()
        .filter(|team| {
            team.len() >= 9
                && team
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
        .ok_or_else(|| "Slack sign-in link has no valid workspace".to_owned())?
        .to_ascii_uppercase();
    let key = path
        .get(1)
        .copied()
        .filter(|key| {
            !key.is_empty()
                && key
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
        .ok_or_else(|| "Slack sign-in link has no valid key".to_owned())?;
    let host = url
        .query_pairs()
        .find_map(|(name, value)| (name == "host").then(|| value.into_owned()))
        .unwrap_or_else(|| "slack.com".to_owned());
    let host = match url::Host::parse(&host) {
        Ok(url::Host::Domain(host)) if host == "slack.com" || host.ends_with(".slack.com") => host,
        _ => return Err("Slack sign-in link has an invalid host".to_owned()),
    };
    let token = format!("z-app-{team}-{key}");
    let mut request = url::Url::parse("https://slack.com/api/auth.loginMagicBulk")
        .expect("static Slack URL must parse");
    if request.set_host(Some(&host)).is_err() {
        return Err("Slack sign-in link has an invalid host".to_owned());
    }
    request
        .query_pairs_mut()
        .append_pair("magic_tokens", &token)
        .append_pair("ssb", "1");
    Ok(request.into())
}

fn harvest_d_cookie(webview: &wry::WebView) -> Option<String> {
    let urls = ["https://app.slack.com/", "https://slack.com/"];
    for attempt in 0..10 {
        let mut names = Vec::new();
        for url in urls {
            if let Ok(cookies) = webview.cookies_for_url(url)
                && let Some(d) = find_d(cookies, &mut names)
            {
                return Some(d);
            }
        }
        if let Ok(cookies) = webview.cookies()
            && let Some(d) = find_d(cookies, &mut names)
        {
            return Some(d);
        }
        eprintln!("super-platinum auth: cookie names (attempt {attempt}): {names:?}");
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    None
}

fn find_d(cookies: Vec<wry::cookie::Cookie<'static>>, names: &mut Vec<String>) -> Option<String> {
    cookies.into_iter().find_map(|c| {
        names.push(c.name().to_owned());
        (c.name() == "d").then(|| c.value().to_owned())
    })
}

fn jstr<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

fn session_from_localconfig(local_config: &str, d_cookie: String) -> Result<Session, String> {
    let value: serde_json::Value =
        serde_json::from_str(local_config).map_err(|e| format!("parse localConfig: {e}"))?;
    let teams = value
        .get("teams")
        .and_then(|t| t.as_object())
        .ok_or("localConfig has no `teams`")?;

    let mut workspaces = BTreeMap::new();
    for (key, team) in teams {
        let token = jstr(team, "token").unwrap_or("");
        if !token.starts_with("xoxc-") {
            continue;
        }
        let team_id = jstr(team, "id").unwrap_or(key).to_owned();
        let url = jstr(team, "url")
            .map(|u| u.trim_end_matches('/').to_owned())
            .unwrap_or_else(|| format!("https://{team_id}.slack.com"));
        let name = jstr(team, "name").unwrap_or(&team_id).to_owned();
        let enterprise_id = jstr(team, "enterprise_id").map(str::to_owned);
        let user_id = jstr(team, "user_id")
            .or_else(|| jstr(team.get("self")?, "id"))
            .unwrap_or_default()
            .to_owned();

        eprintln!(
            "super-platinum auth: team={team_id} name={name:?} url={url} enterprise={enterprise_id:?} token_present=true"
        );

        workspaces.insert(
            team_id.clone(),
            WorkspaceSession {
                team_id,
                enterprise_id,
                user_id,
                name,
                url,
                token: token.to_owned(),
            },
        );
    }

    if workspaces.is_empty() {
        return Err("no workspace with an xoxc token in localConfig".to_owned());
    }

    let has_enterprise = workspaces.keys().any(|id| id.starts_with('E'));
    if has_enterprise {
        workspaces.retain(|id, _| id.starts_with('E'));
    }

    Ok(Session {
        d_cookie,
        workspaces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_session_from_localconfig() {
        let lc = r#"{
            "teams": {
                "T0266FRGM": {
                    "id": "T0266FRGM",
                    "name": "Hack Club",
                    "url": "https://hackclub.slack.com/",
                    "token": "xoxc-abc-def",
                    "user_id": "U080A3QP42C",
                    "enterprise_id": "E09V59WQY1E"
                }
            }
        }"#;
        let session = session_from_localconfig(lc, "xoxd-cookie".into()).unwrap();
        assert_eq!(session.d_cookie, "xoxd-cookie");
        let ws = session.workspaces.get("T0266FRGM").unwrap();
        assert_eq!(ws.token, "xoxc-abc-def");
        assert_eq!(ws.url, "https://hackclub.slack.com");
        assert_eq!(ws.user_id, "U080A3QP42C");
        assert_eq!(ws.enterprise_id.as_deref(), Some("E09V59WQY1E"));
    }

    #[test]
    fn enterprise_grid_keeps_only_enterprise() {
        let lc = r#"{
            "teams": {
                "E09V59WQY1E": {"id":"E09V59WQY1E","name":"HC Ent","url":"https://hackclub.enterprise.slack.com","token":"xoxc-ent"},
                "T0266FRGM": {"id":"T0266FRGM","name":"Hack Club","url":"https://hackclub.slack.com","token":"xoxc-child","enterprise_id":"E09V59WQY1E"}
            }
        }"#;
        let session = session_from_localconfig(lc, "d".into()).unwrap();
        assert_eq!(session.workspaces.len(), 1);
        assert!(session.workspaces.contains_key("E09V59WQY1E"));
    }

    #[test]
    fn skips_teams_without_xoxc() {
        let lc = r#"{"teams":{"T1":{"id":"T1","token":"","url":"https://x.slack.com"}}}"#;
        assert!(session_from_localconfig(lc, "d".into()).is_err());
    }

    #[test]
    fn errors_on_missing_teams() {
        assert!(session_from_localconfig("{}", "d".into()).is_err());
        assert!(session_from_localconfig("not json", "d".into()).is_err());
    }

    #[test]
    fn builds_magic_login_request_from_protocol_url() {
        let request = magic_login_request_url(
            "slack://t0266frgm/magic-login/abc-123?host=hackclub.enterprise.slack.com",
        )
        .unwrap();
        let parsed = url::Url::parse(&request).unwrap();
        assert_eq!(parsed.host_str(), Some("hackclub.enterprise.slack.com"));
        assert_eq!(parsed.path(), "/api/auth.loginMagicBulk");
        assert_eq!(
            parsed
                .query_pairs()
                .find_map(|(name, value)| (name == "magic_tokens").then(|| value.into_owned())),
            Some("z-app-T0266FRGM-abc-123".to_owned())
        );
    }

    #[test]
    fn rejects_non_login_and_untrusted_magic_login_urls() {
        assert!(magic_login_request_url("slack://open?team=T0266FRGM").is_err());
        assert!(magic_login_request_url("slack://T0266FRGM/magic-login/").is_err());
        assert!(magic_login_request_url("slack://T0266FRGM/magic-login/abc/ignored").is_err());
        assert!(
            magic_login_request_url("slack://T0266FRGM/magic-login/abc?host=attacker.example.com")
                .is_err()
        );
        assert!(
            magic_login_request_url(
                "slack://T0266FRGM/magic-login/abc?host=attacker.example/api?x=.slack.com"
            )
            .is_err()
        );
    }
}
