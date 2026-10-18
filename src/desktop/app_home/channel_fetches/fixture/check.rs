//! Offline acceptance of the actual shared channel initialization controller.
use super::*;
use crate::slack_links_check::wait_dom;
use dioxus::desktop::DesktopContext;
use std::time::Duration;

const FLAG: &str = "SUPER_PLATINUM_CHANNEL_INITIALIZATION_CHECK";
const STAGES: [&str; 6] = [
    "warm",
    "init",
    "channels",
    "rejected",
    "socket",
    "replacement",
];

async fn readiness(name: &str) -> Result<Value, String> {
    let nonce = std::env::var("SUPER_PLATINUM_CHANNEL_INITIALIZATION_NONCE")
        .map_err(|_| "Missing channel initialization readiness nonce")?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid channel initialization readiness nonce".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_CHANNEL_INITIALIZATION_READY_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute channel initialization readiness directory required")?;
    if directory.file_name().and_then(|s| s.to_str())
        != Some(format!("channel-initialization-{nonce}").as_str())
    {
        return Err("Wrong channel initialization readiness directory".into());
    }
    let expected = json!({"version":1,"fixture":name,"nonce":nonce,"pid":std::process::id()});
    let inspect = |directory: std::path::PathBuf| async move {
        tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let metadata = std::fs::symlink_metadata(&directory)
                .map_err(|_| "Inspect channel initialization readiness directory")?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("Invalid channel initialization readiness directory".into());
            }
            let path = directory.join("ready.json");
            let metadata = match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                result => result.map_err(|_| "Inspect channel initialization readiness marker")?,
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
                return Err("Invalid channel initialization readiness marker".into());
            }
            let bytes =
                std::fs::read(path).map_err(|_| "Read channel initialization readiness marker")?;
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Parse channel initialization readiness marker".into())
        })
        .await
        .map_err(|_| "Channel initialization readiness worker failed")?
    };
    if inspect(directory.clone()).await?.is_some() {
        return Err("Channel initialization readiness marker must be fresh".into());
    }
    eprintln!("channel-initialization-waiting: {expected}");
    for _ in 0..300 {
        if let Some(actual) = inspect(directory.clone()).await? {
            return if actual == expected {
                Ok(expected)
            } else {
                Err("Wrong channel initialization readiness owner".into())
            };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Channel initialization readiness timed out".into())
}

pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !crate::fixture::is_fixture() || std::env::var(FLAG).as_deref() != Ok("1") {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    if !matches!(
        name.as_str(),
        "channel-initialization" | "channel-initialization-narrow"
    ) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => json!({"ok":false,"fixture":name,"error":error}),
            Err(_) => {
                json!({"ok":false,"fixture":name,"error":"Channel initialization check timed out"})
            }
        };
        eprintln!("channel-initialization-check: {result}");
    });
}

async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    for stage in STAGES {
        let proof = if stage == "warm" {
            Value::Null
        } else {
            exercise(state, stage)
                .await
                .map_err(|error| format!("Exercise {stage}: {error}"))?
        };
        capture_stage(
            state,
            desktop,
            name,
            stage,
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(json!({"ok":true,"fixture":name,"readiness":readiness,
        "viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,
        "final":snapshot(&state.peek()),"captureChecksPassed":true,
        "visualReview":"PNG inspection required separately"}))
}

async fn capture_stage(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let (model, expected) = {
        let shell = state.peek();
        let rows = shell
            .sidebar_sections
            .iter()
            .filter(|section| !section.collapsed)
            .flat_map(|section| {
                section.channel_indices.iter().filter_map(|index| {
                    shell
                        .channels
                        .get(*index)
                        .map(|channel| json!({"name":channel.name,"section":section.title}))
                })
            })
            .collect::<Vec<_>>();
        (snapshot(&shell), rows)
    };
    if expected.is_empty() {
        return Err(format!("Stage {stage} has no sidebar projection"));
    }
    let mut geometry = wait_layout(desktop,stage,&format!(r#"(()=>{{
      const list=document.querySelector('.sidebar .channel-list');if(!list)return {{ok:false}};
      const expected={},focusName={},buttons=[...list.querySelectorAll('button.channel')];
      const row=e=>{{let s=e.previousElementSibling;while(s&&!s.matches('.section-toggle'))s=s.previousElementSibling;return {{name:e.querySelector('.channel-name')?.textContent,section:s?.querySelector('span')?.textContent}};}};
      const actual=buttons.map(row),focus=buttons.find(e=>row(e).name===focusName);if(!focus)return {{ok:false,actualRows:actual}};
      focus.scrollIntoView({{block:'nearest'}});
      let heading=focus.previousElementSibling;while(heading&&!heading.matches('.section-toggle'))heading=heading.previousElementSibling;
      if(heading)heading.scrollIntoView({{block:'nearest'}});
      const rect=e=>{{const r=e.getBoundingClientRect();return {{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}}};
      // Edge midpoints stay inside rounded buttons; bounding-box corners do not.
      const hitTest=e=>{{const r=rect(e),cx=r.left+r.width/2,cy=r.top+r.height/2;return [[cx,cy],[r.left+1,cy],[r.right-1,cy],[cx,r.top+1],[cx,r.bottom-1]].map(([x,y])=>{{const a=document.elementFromPoint(x,y);return {{x,y,owned:a===e||e.contains(a),tag:a?.tagName,class:a?.getAttribute('class')}}}})}};
      const visible=e=>{{const r=rect(e),s=getComputedStyle(e);return r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&hitTest(e).every(p=>p.owned)}};
      const labelsFit=[...focus.querySelectorAll('.channel-name')].every(e=>e.scrollWidth<=e.clientWidth+1);
      const each=JSON.stringify(actual)===JSON.stringify(expected),noOverflow=document.documentElement.scrollWidth<=innerWidth+1;
      return {{ok:each&&noOverflow&&visible(focus)&&!!heading&&visible(heading)&&labelsFit,
        viewport:[innerWidth,innerHeight],noOverflow,expectedRows:expected,actualRows:actual,
        focus:{{...row(focus),visible:visible(focus),labelsFit,rect:rect(focus),hitTests:hitTest(focus)}},
        heading:heading?{{text:heading.querySelector('span')?.textContent,visible:visible(heading),rect:rect(heading),hitTests:hitTest(heading)}}:null}};
    }})()"#,json!(expected),model["focusChannel"])).await?;
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or("Absolute capture directory required")?;
    let path = directory.join(format!("{name}-{stage}.png"));
    let inspect = path.clone();
    if tokio::task::spawn_blocking(move || inspect.try_exists())
        .await
        .map_err(|_| "Inspect capture path failed")?
        .map_err(|_| "Inspect capture path failed")?
    {
        return Err("Capture must be fresh".into());
    }
    eprintln!(
        "channel-initialization-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":stage,"model":model,"proof":proof,"geometry":geometry}));
    eprintln!(
        "channel-initialization-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}

/// Retain the last measured layout rather than discarding it on a poll timeout.
async fn wait_layout(desktop: &DesktopContext, stage: &str, script: &str) -> Result<Value, String> {
    let wrapped = format!("(()=>({{ok:true,measured:({script})}}))()");
    let mut last = Value::Null;
    for _ in 0..60 {
        let value = wait_dom(desktop, &wrapped)
            .await
            .map_err(|error| format!("Layout {stage}: {error}; last measurement: {last}"))?;
        last = value["measured"].clone();
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    eprintln!(
        "channel-initialization-layout: {}",
        json!({"stage":stage,"last":last})
    );
    Err(format!(
        "Layout {stage} timed out; last measurement: {last}"
    ))
}
