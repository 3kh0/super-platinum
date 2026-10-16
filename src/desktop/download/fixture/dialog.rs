//! Windows cancellation diagnostic. Real rfd, no destination or transfer substitution.
use super::*;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
static RETURNED_NONE: OnceLock<bool> = OnceLock::new();
pub(crate) fn enabled() -> bool {
    cfg!(target_os = "windows")
        && std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("original-download-save-dialog")
        && std::env::var("SUPER_PLATINUM_SAVE_DIALOG_CHECK").as_deref() == Ok("1")
}
fn unlinked(path: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "missing diagnostic path")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("reparse diagnostic path".into());
        }
    }
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("invalid diagnostic directory".into());
    }
    Ok(())
}
fn validate_root(root: &Path, nonce: &str) -> Result<(), String> {
    if !root.is_absolute()
        || root.file_name().and_then(|n| n.to_str())
            != Some(format!("original-save-dialog-{nonce}").as_str())
    {
        return Err("wrong diagnostic directory".into());
    }
    for parent in root.ancestors() {
        unlinked(parent)?;
    }
    unlinked(&root.join("downloads"))?;
    let expected = std::env::current_dir()
        .map_err(|_| "cwd")?
        .join("tmp")
        .canonicalize()
        .map_err(|_| "tmp")?;
    if root
        .parent()
        .ok_or("parent")?
        .canonicalize()
        .map_err(|_| "parent")?
        != expected
    {
        return Err("diagnostic root outside repository tmp".into());
    }
    Ok(())
}
impl Input {
    pub fn dialog_diagnostic(&self) -> bool {
        enabled() && self.run.alias == "original-download-save-dialog"
    }
    pub fn dialog_directory(&self) -> PathBuf {
        self.run.root.join("downloads")
    }
    pub fn begin_dialog(&self) -> bool {
        if !self.dialog_diagnostic() || !claim(&self.attempt, &self.run.issued) {
            return false;
        }
        eprintln!(
            "save-dialog-requested: {}",
            json!({"version":1,"nonce":self.run.nonce,"pid":std::process::id(),"attempt":1,"actualRfd":true})
        );
        true
    }
    pub fn dialog_returned(&self, none: bool) {
        let first = RETURNED_NONE.set(none).is_ok();
        eprintln!(
            "save-dialog-returned: {}",
            json!({"version":1,"nonce":self.run.nonce,"pid":std::process::id(),"attempt":1,"none":none,"first":first,"transferAllowed":false})
        );
    }
}
fn claim(attempt: &AtomicUsize, issued: &AtomicUsize) -> bool {
    attempt
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
        && issued
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_claim_is_one_use_across_input_clones_and_fresh_inputs() {
        let shared_attempt = Arc::new(AtomicUsize::new(0));
        let clone = shared_attempt.clone();
        let issued = AtomicUsize::new(0);
        assert!(claim(&shared_attempt, &issued));
        assert!(!claim(&clone, &issued));
        assert!(!claim(&AtomicUsize::new(0), &issued));
        assert_eq!(issued.load(Ordering::Acquire), 1);
    }
    #[test]
    fn diagnostic_never_consumes_a_used_substituted_run_budget() {
        for used in [1, 2, 3] {
            let issued = AtomicUsize::new(used);
            assert!(!claim(&AtomicUsize::new(0), &issued));
            assert_eq!(issued.load(Ordering::Acquire), used);
        }
    }
}
async fn ack(run: &Run, stage: &str) -> Result<(), String> {
    let expected = json!({"version":1,"nonce":run.nonce,"pid":std::process::id(),"fixture":run.alias,"stage":stage});
    for _ in 0..750 {
        let path = run.root.join(format!("ack-{stage}.json"));
        if let Ok(meta) = tokio::fs::symlink_metadata(&path).await {
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 4096 {
                return Err("invalid ack".into());
            }
            let value: Value =
                serde_json::from_slice(&tokio::fs::read(path).await.map_err(|_| "ack read")?)
                    .map_err(|_| "ack json")?;
            return if value == expected {
                Ok(())
            } else {
                Err("wrong exact ack".into())
            };
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("diagnostic ack expired".into())
}
async fn exercise(state: Signal<ShellState>) -> Result<Value, String> {
    if !enabled() {
        return Err("Windows exact diagnostic gate required".into());
    }
    let root = PathBuf::from(
        std::env::var("SUPER_PLATINUM_ORIGINAL_DOWNLOAD_DIR").map_err(|_| "directory")?,
    );
    let nonce = std::env::var("SUPER_PLATINUM_ORIGINAL_DOWNLOAD_NONCE").map_err(|_| "nonce")?;
    tokio::task::spawn_blocking(move || validate_root(&root, &nonce))
        .await
        .map_err(|_| "directory worker")??;
    let run = install(state).await?;
    let owner = json!({"version":1,"nonce":run.nonce,"pid":std::process::id(),"fixture":run.alias});
    eprintln!("save-dialog-waiting: {owner}");
    ack(&run, "ready").await?;
    let mut eval = document::eval(include_str!("dialog.js"));
    let initial: Value = eval.recv().await.map_err(|_| "initial DOM")?;
    if initial["ok"] != true || initial["stage"] != "initial" {
        return Err("initial DOM assertion".into());
    }
    eprintln!("save-dialog-initial: {initial}");
    ack(&run, "initial").await?;
    eval.send("open").map_err(|_| "open ack")?;
    let final_dom: Value = tokio::time::timeout(Duration::from_secs(200), eval.recv())
        .await
        .map_err(|_| "dialog deadline")?
        .map_err(|_| "final DOM")?;
    if final_dom["ok"] != true
        || final_dom["stage"] != "returned"
        || RETURNED_NONE.get() != Some(&true)
        || run.issued() != 1
        || !run.outcomes()?.is_empty()
    {
        return Err("actual cancellation diagnostic failed".into());
    }
    let shell = state.peek();
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || shell.core.active_account.is_some()
        || shell.toast.is_some()
    {
        return Err("diagnostic acquired state/toast".into());
    }
    Ok(
        json!({"ok":true,"owner":owner,"initial":initial,"final":final_dom,"dialogReturnedNone":true,"attempts":1,"transfers":0,"noSession":true,"noTransport":true,"noAccount":true,"actualRfd":true,"diagnosticOnly":true}),
    )
}
pub(crate) async fn run(state: Signal<ShellState>) {
    let result = match exercise(state).await {
        Ok(v) => v,
        Err(e) => json!({"ok":false,"error":e}),
    };
    eprintln!("save-dialog-check: {result}");
}
