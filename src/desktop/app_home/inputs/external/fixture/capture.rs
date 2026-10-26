use super::*;
use dioxus::desktop::DesktopContext;
use std::time::Duration;

pub(crate) async fn dom(script: &str) -> Result<Value, String> {
    let mut eval =
        dioxus::document::eval(&format!("dioxus.send(({}));await dioxus.recv();", script));
    let received = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>()).await;
    let release = eval.send(());
    let value = received
        .map_err(|_| "Home DOM timeout")?
        .map_err(|_| "Home DOM reply missing")?;
    release.map_err(|_| "Home DOM acknowledgement failed")?;
    if value["ok"] != true {
        return Err(format!("Home DOM rejected: {value}"));
    }
    Ok(value)
}
pub(crate) const ACTION_GUARD: &str = r#"const owned=e=>{if(!e||e.disabled)return false;e.scrollIntoView({block:'center'});const r=e.getBoundingClientRect(),s=getComputedStyle(e);if(!r.width||!r.height||r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1||s.visibility!=='visible'||s.display==='none')return false;for(let p=e.parentElement;p;p=p.parentElement){const ps=getComputedStyle(p),pr=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(ps.overflowY)&&(r.top<pr.top-1||r.bottom>pr.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(ps.overflowX)&&(r.left<pr.left-1||r.right>pr.right+1))return false;}return [[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const h=document.elementFromPoint(x,y);return h===e||e.contains(h);});};"#;
pub(crate) async fn click(selector: &str) -> Result<Value, String> {
    dom(&format!(r#"(()=>{{{ACTION_GUARD}const e=document.querySelector({});if(!owned(e))return{{ok:false}};e.focus({{preventScroll:true}});e.click();return{{ok:true,text:e.textContent,visibleHitTested:true}};}})()"#,json!(selector))).await
}
pub(crate) async fn type_query(text: &str) -> Result<Value, String> {
    dom(&format!(r#"(()=>{{{ACTION_GUARD}const e=document.querySelector('[data-home-external] input[type=search]');if(!owned(e))return{{ok:false}};e.focus({{preventScroll:true}});Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true,value:e.value,visibleHitTested:true}};}})()"#,json!(text))).await
}
pub(crate) async fn wait(desktop: &DesktopContext, expression: &str) -> Result<Value, String> {
    crate::slack_links_check::wait_dom(
        desktop,
        &format!("(()=>{{const ok=({expression});return{{ok}};}})()"),
    )
    .await
}
pub(crate) async fn shot(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let layout = include_str!("layout.js").replace("__SP_FOCUS__", "null");
    let mut geometry = crate::slack_links_check::wait_dom(desktop, &layout).await?;
    let inventory = geometry["inventory"].clone();
    let before_model = snapshot(&state.peek());
    let required = geometry["requiredViews"]
        .as_array()
        .ok_or("Missing scroll coverage inventory")?
        .clone();
    capture_image(desktop, name, stage, &mut geometry, captures).await?;
    let mut scroll_views = Vec::new();
    for key in required {
        let key = key.as_str().ok_or("Invalid scroll coverage key")?;
        let layout = include_str!("layout.js").replace("__SP_FOCUS__", &json!(key).to_string());
        let mut view = crate::slack_links_check::wait_dom(desktop, &layout).await?;
        if view["inventory"] != inventory || view["focusKey"] != key || view["focusVisible"] != true
        {
            return Err(
                "Scroll capture changed result/control inventory or lost target visibility".into(),
            );
        }
        let capture_key = format!("{stage}-{key}");
        capture_image(desktop, name, &capture_key, &mut view, captures).await?;
        let row_index = key
            .strip_prefix("row-")
            .and_then(|i| i.parse::<usize>().ok());
        let option = row_index
            .and_then(|i| before_model["query"]["rows"].get(i))
            .map(|row| row["option"].clone());
        if row_index.is_some() && option.is_none() {
            return Err("Scroll row has no current source option".into());
        }
        scroll_views.push(json!({"key":key,"captureKey":capture_key,"rowIndex":row_index,"option":option,"geometry":view}));
    }
    let after_model = snapshot(&state.peek());
    if before_model["query"]["rows"] != after_model["query"]["rows"]
        || before_model["formState"] != after_model["formState"]
    {
        return Err("Scrolling changed current result inventory or selected form state".into());
    }
    stages.push(json!({"stage":stage,"geometry":geometry,"scrollViews":scroll_views,"model":after_model,"proof":proof}));
    Ok(())
}
pub(crate) async fn capture_image(
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    geometry: &mut Value,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute capture directory required")?;
    let path = directory.join(format!("{name}-{stage}.png"));
    let inspect = path.clone();
    if tokio::task::spawn_blocking(move || inspect.try_exists())
        .await
        .map_err(|_| "Capture path worker failed")?
        .map_err(|_| "Capture path failed")?
    {
        return Err("Capture must be fresh".into());
    }
    eprintln!(
        "app-home-external-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    eprintln!(
        "app-home-external-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
pub(crate) async fn readiness(name: &str) -> Result<Value, String> {
    let nonce = std::env::var("SUPER_PLATINUM_APP_HOME_EXTERNAL_NONCE")
        .map_err(|_| "Missing Home external readiness nonce")?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid Home external readiness nonce".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_APP_HOME_EXTERNAL_READY_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute Home external readiness directory required")?;
    if directory.file_name().and_then(|s| s.to_str())
        != Some(format!("app-home-external-{nonce}").as_str())
    {
        return Err("Wrong Home external readiness directory".into());
    }
    let expected = json!({"version":1,"fixture":name,"nonce":nonce,"pid":std::process::id()});
    let inspect = |directory: std::path::PathBuf| async move {
        tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let metadata = std::fs::symlink_metadata(&directory)
                .map_err(|_| "Inspect Home external readiness directory")?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("Invalid Home external readiness directory".into());
            }
            let path = directory.join("ready.json");
            let metadata = match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                result => result.map_err(|_| "Inspect Home external readiness marker")?,
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
                return Err("Invalid Home external readiness marker".into());
            }
            let bytes = std::fs::read(path).map_err(|_| "Read Home external readiness marker")?;
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Parse Home external readiness marker".into())
        })
        .await
        .map_err(|_| "Home external readiness worker failed")?
    };
    if inspect(directory.clone()).await?.is_some() {
        return Err("Home external readiness marker must be fresh".into());
    }
    eprintln!("app-home-external-waiting: {expected}");
    for _ in 0..300 {
        if let Some(actual) = inspect(directory.clone()).await? {
            return if actual == expected {
                Ok(expected)
            } else {
                Err("Wrong Home external readiness owner".into())
            };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Home external readiness timed out".into())
}
