//! Exact opt-in disconnected completion timing. No ordinary-session instrumentation.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{borrow::Cow, path::PathBuf, sync::OnceLock, time::Duration};
use super_platinum_core::composer_completion::suggestions;

#[path = "../../../scripts/completion_baseline/corpus.rs"]
mod corpus;
mod owner;
mod setup;
use owner::Owner;

const FLAG: &str = "SUPER_PLATINUM_COMPLETION_BASELINE";
const PREFIX: &str = "completion-baseline: ";
pub(crate) fn size(name: &str) -> Option<usize> {
    match name.strip_suffix("-narrow").unwrap_or(name) {
        "completion-directory-2000" => Some(2_000),
        "completion-directory-10000" => Some(10_000),
        "completion-directory-64000" => Some(64_000),
        _ => None,
    }
}
fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var(FLAG).as_deref() == Ok("1")
            && std::env::var("SUPER_PLATINUM_FIXTURE")
                .ok()
                .as_deref()
                .and_then(size)
                .is_some()
    })
}

/// Canonical search directory is exactly the pure corpus. The bounded chrome is
/// retained separately so a large sidebar does not confound this query cohort.
pub(crate) fn install(shell: &mut ShellState, total: usize) {
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    let ws = shell
        .core
        .workspaces
        .get_mut("T1")
        .expect("offline workspace");
    corpus::populate(ws, total);
    ws.self_user_id = "UPERF000001".into();
    ws.last_active_channel = Some("CPERF000001".into());
    if let Some(bag) = ws.messages.remove("C2") {
        ws.messages.insert("CPERF000001".into(), bag);
    }
    shell.core.active_channel = Some("CPERF000001".into());
    shell.core.composer = Default::default();
    let mut channel = shell.channels[shell.active_channel].clone();
    channel.id = "CPERF000001".into();
    channel.name = "project-000001".into();
    channel.unread = false;
    channel.unread_count = 0;
    channel.mention_count = 0;
    shell.channels = vec![channel];
    shell.active_channel = 0;
    shell.sidebar_sections = vec![crate::model::SidebarSectionVm {
        id: "completion-fixture".into(),
        kind: "channels".into(),
        title: "Channels".into(),
        collapsed: false,
        channel_indices: vec![0],
    }];
    shell.surfaces.clear();
}

fn instrument(source: &str) -> Result<String, &'static str> {
    const OLD_QUERY: &str = "dioxus.send({kind:'query', seq, text:field.value, anchor:field.selectionStart, cursor:field.selectionEnd});";
    const DIRECTION_QUERY: &str = "dioxus.send({kind:'query', seq, text:field.value, anchor:field.selectionStart, cursor:field.selectionEnd, direction:field.selectionDirection});";
    let result = "popup.hidden = false; field.setAttribute('aria-expanded','true'); highlight();";
    let old_count = source.matches(OLD_QUERY).count();
    let direction_count = source.matches(DIRECTION_QUERY).count();
    if old_count + direction_count != 1 || source.matches(result).count() != 1 {
        return Err("Completion instrumentation anchors changed");
    }
    let (query, measured_snapshot) = if old_count == 1 {
        (OLD_QUERY, "snapshot()")
    } else {
        // The unchanged timing probe correlates full text and UTF16 offsets.
        // The actual controller's stillCurrent guard below retains direction too.
        (
            DIRECTION_QUERY,
            "JSON.stringify([field.value,field.selectionStart,field.selectionEnd])",
        )
    };
    Ok(source.replace(query,&format!("window.__spCompletionBaseline?.query(field,seq,{measured_snapshot});\n    {query}"))
        .replace(result,&format!("{result}\n      window.__spCompletionBaseline?.result(field,result.seq,result,()=>result.seq===seq && source===snapshot() && document.activeElement===field && !composing);")))
}
pub(crate) fn controller_script() -> Cow<'static, str> {
    let source = include_str!("../composer_completion.js");
    if super::emoji_retirement::enabled() {
        Cow::Owned(
            super::emoji_retirement::instrument(source)
                .expect("exact retirement diagnostic anchors"),
        )
    } else if super::emoji_lifecycle::enabled() {
        Cow::Owned(
            super::emoji_lifecycle::instrument(source).expect("exact emoji cold instrumentation"),
        )
    } else if enabled() {
        Cow::Owned(instrument(source).expect("exact fixture instrumentation"))
    } else {
        Cow::Borrowed(source)
    }
}

struct Config {
    fixture: String,
    total: usize,
    nonce: String,
    directory: PathBuf,
    rounds: usize,
}
impl Config {
    fn load() -> Result<Self, String> {
        let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
        let total = size(&fixture).ok_or("Unknown fixture")?;
        let nonce = std::env::var("SUPER_PLATINUM_COMPLETION_BASELINE_NONCE")
            .map_err(|_| "Missing nonce")?;
        if nonce.len() != 32
            || !nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Invalid nonce".into());
        }
        let rounds = std::env::var("SUPER_PLATINUM_COMPLETION_BASELINE_ROUNDS")
            .ok()
            .and_then(|n| n.parse().ok())
            .filter(|n| (1..=12).contains(n))
            .ok_or("Invalid rounds")?;
        let directory = PathBuf::from(
            std::env::var("SUPER_PLATINUM_COMPLETION_BASELINE_DIR")
                .map_err(|_| "Missing directory")?,
        );
        let expected_directory = format!("completion-baseline-{nonce}");
        if !directory.is_absolute()
            || directory.file_name().and_then(|s| s.to_str()) != Some(expected_directory.as_str())
        {
            return Err("Unowned directory".into());
        }
        Ok(Self {
            fixture,
            total,
            nonce,
            directory,
            rounds,
        })
    }
    fn identity(&self) -> Value {
        json!({"version":1,"fixture":self.fixture,"nonce":self.nonce,"pid":std::process::id(),"rounds":self.rounds,"total":self.total})
    }
}

async fn read_owned(config: &Config, name: &str) -> Result<Option<Value>, String> {
    let metadata = tokio::fs::symlink_metadata(&config.directory)
        .await
        .map_err(|_| "Owned directory unavailable")?;
    if metadata.file_type().is_symlink() {
        return Err("Directory symlink refused".into());
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Directory reparse refused".into());
        }
    }
    let path = config.directory.join(name);
    let metadata = match tokio::fs::symlink_metadata(&path).await {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Marker metadata unavailable".into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 2048 {
        return Err("Invalid marker type/size".into());
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Marker reparse refused".into());
        }
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| "Marker read failed")?;
    if bytes.len() > 2048 {
        return Err("Oversized marker".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "Malformed marker".into())
}
async fn wait_marker(config: &Config, name: &str, expected: &Value) -> Result<(), String> {
    for _ in 0..300 {
        if let Some(value) = read_owned(config, name).await? {
            return if &value == expected {
                Ok(())
            } else {
                Err("Wrong marker identity".into())
            };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Marker deadline exceeded".into())
}

pub(crate) async fn run(state: Signal<ShellState>) {
    if !enabled() {
        return;
    }
    let result = execute(state).await;
    if let Err(error) = result {
        eprintln!(
            "{PREFIX}{}",
            json!({"ok":false,"error":error,"pid":std::process::id()})
        );
    }
}
async fn execute(state: Signal<ShellState>) -> Result<(), String> {
    let config = Config::load()?;
    let marker = json!({"version":1,"nonce":config.nonce,"fixture":config.fixture,"rounds":config.rounds,"total":config.total});
    if read_owned(&config, "owner.json").await? != Some(marker) {
        return Err("Owner marker mismatch".into());
    }
    let owner = Owner::capture(&state.peek());
    if !owner.current(&state.peek()) {
        return Err("Connected or mismatched fixture".into());
    }
    eprintln!("completion-baseline-waiting: {}", config.identity());
    wait_marker(&config, "ready.json", &config.identity()).await?;
    if !owner.current(&state.peek()) {
        return Err("Owner changed before query setup".into());
    }
    // Untimed fixture oracle setup must not monopolize the native event loop.
    // Scope the canonical borrow to one actual query, then give OS events a turn.
    let cases = setup::collect(
        corpus::QUERIES.len(),
        |index| {
            let query = corpus::QUERIES[index];
            let shell = state.peek();
            let ws = &shell.core.workspaces["T1"];
            let choices: Vec<_> = suggestions(ws, &corpus::trigger(query))
                .into_iter()
                .map(|s| json!({"token":s.target.token().expect("valid token"),"label":s.label,"detail":s.detail}))
                .collect();
            json!({"query":query,"choices":choices})
        },
        || owner.current(&state.peek()),
    )
    .await?;
    let counts = corpus::counts(&state.peek().core.workspaces["T1"]);
    let script = format!(
        "const config={};\n{}\n{}",
        json!({"nonce":config.nonce,"rounds":config.rounds,"cases":cases}),
        include_str!("completion/probe.js"),
        include_str!("completion/check.js")
    );
    let mut bridge = document::eval(&script);
    let mut samples = Vec::new();
    let mut captures = Vec::new();
    let mut armed = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        let value = tokio::time::timeout_at(deadline, bridge.recv::<Value>())
            .await
            .map_err(|_| "Native deadline exceeded")?
            .map_err(|_| "Native evaluator ended")?;
        if value["nonce"] != config.nonce || !owner.current(&state.peek()) {
            return Err("Native owner/directory changed".into());
        }
        let kind = value["kind"].as_str().ok_or("Missing kind")?;
        match kind {
            "arm" => {
                let repeat = samples.len() / 24;
                let index = ((samples.len() % 24) * 7 + repeat) % 24;
                if armed.is_some()
                    || value["repeat"] != repeat
                    || value["case"] != index
                    || value["query"] != corpus::QUERIES[index]
                {
                    return Err("Unordered sample arm".into());
                }
                armed = Some((repeat, index));
            }
            "sample" => {
                let (repeat, index) = armed.take().ok_or("Unarmed result")?;
                let sample = &value["sample"];
                if sample["repeat"] != repeat
                    || sample["case"] != index
                    || sample["query"] != corpus::QUERIES[index]
                    || sample["choices"] != cases[index]["choices"]
                    || sample["inputTrusted"] != false
                    || sample["focused"] != true
                    || sample["connected"] != true
                    || sample["field"] != "channel-composer"
                    || state.peek().core.composer.text != corpus::QUERIES[index]
                {
                    return Err("Result/model mismatch".into());
                }
                let keys = [
                    "eventEntryMs",
                    "queryDispatchMs",
                    "acceptedResultMs",
                    "raf1Ms",
                    "raf2Ms",
                ];
                let times: Option<Vec<_>> = keys
                    .iter()
                    .map(|key| sample[*key].as_f64().filter(|n| n.is_finite() && *n >= 0.))
                    .collect();
                let times = times.ok_or("Invalid timings")?;
                if !times.windows(2).all(|v| v[0] <= v[1]) || times[4] - times[0] > 5000. {
                    return Err("Non-monotonic/expired sample".into());
                }
                samples.push(sample.clone());
            }
            "capture" => {
                let stage = value["stage"].as_str().ok_or("Missing capture stage")?;
                let expected = ["initial", "people", "empty", "unicode"];
                if armed.is_some()
                    || captures.len() >= expected.len()
                    || stage != expected[captures.len()]
                {
                    return Err("Wrong capture order".into());
                }
                let edge = json!({"nonce":config.nonce,"pid":std::process::id(),"stage":stage});
                eprintln!("completion-baseline-capture: {edge}");
                wait_marker(&config, &format!("capture-{stage}.json"), &edge).await?;
                captures.push(stage.to_owned());
            }
            "done" => {
                if armed.is_some()
                    || samples.len() != config.rounds * 24
                    || value["count"] != samples.len()
                    || captures.len() != 4
                {
                    return Err("Incomplete workload".into());
                }
                eprintln!(
                    "{PREFIX}{}",
                    json!({"ok":true,"identity":config.identity(),"corpus":corpus::VERSION,"counts":counts,"samples":samples,"captures":captures,"noSession":true,"noTransport":true})
                );
                return Ok(());
            }
            "error" => return Err(value["error"].as_str().unwrap_or("Native error").to_owned()),
            _ => return Err("Unknown native message".into()),
        }
        bridge
            .send(json!({"ok":true,"nonce":config.nonce,"kind":kind}))
            .map_err(|_| "Bridge reply failed")?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instrumentation_refuses_changed_or_duplicate_controller_anchors() {
        let source = include_str!("../composer_completion.js");
        assert!(
            instrument(source)
                .unwrap()
                .contains("__spCompletionBaseline?.result")
        );
        assert!(instrument("").is_err());
        assert!(instrument(&format!("{source}{source}")).is_err());
    }
    #[test]
    fn direction_query_hook_preserves_probe_shape_and_rejects_both_or_duplicate_variants() {
        let old = include_str!("../composer_completion.js");
        let original = "cursor:field.selectionEnd});";
        let replacement = "cursor:field.selectionEnd, direction:field.selectionDirection});";
        // This regression fixture represents either current source variant.
        let old = old.replace(replacement, original);
        let direction = old.replace(original, replacement);
        let instrumented = instrument(&direction).unwrap();
        assert!(instrumented.contains(
            "query(field,seq,JSON.stringify([field.value,field.selectionStart,field.selectionEnd]))"
        ));
        assert!(instrumented.contains("source===snapshot()"));
        assert!(instrument(&format!("{old}{direction}")).is_err());
        assert!(instrument(&format!("{direction}{direction}")).is_err());
        assert!(
            instrument(
                &direction.replace(replacement, "cursor:field.selectionEnd, unknown:true});")
            )
            .is_err()
        );
    }
    #[test]
    fn fixture_names_are_exact_and_bounded() {
        assert_eq!(size("completion-directory-64000-narrow"), Some(64000));
        for name in [
            "channel",
            "completion-directory-64001",
            "completion-directory-64000-extra",
            "completion-directory-64000-narrow-narrow",
        ] {
            assert_eq!(size(name), None);
        }
    }
}
