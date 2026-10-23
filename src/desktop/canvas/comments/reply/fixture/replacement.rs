//! Protected disconnected shell replacement. Passive custody never grants a binding.
use super::*;
use std::sync::Arc;

pub(in crate::canvas) const B: &str = "513bac6d-2702-4634-a2d9-f435a4c9ee62";
const B_USER: &str = "UREPLACEB";
const B_FILE: &str = "FreplacementB";
const B_CAPTURE: &str = "fe3f849e-efb7-4e6b-a442-4d8e03806f60";

pub(in crate::canvas) fn requested() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_CANVAS_REPLACEMENT_CUSTODY_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-canonical-native-canvas" | "canvas-canonical-native-canvas-narrow")
        )
}
fn guarded(shell: &ShellState) -> bool {
    requested()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.cache.is_none()
        && matches!(shell.core.active_account.as_deref(), Some(ACCOUNT | B))
}

pub(in crate::canvas) struct Retained {
    id: String,
    disk: Disk,
    root: String,
    owner: Owner,
    completion: Arc<tokio::sync::Notify>,
}
impl Retained {
    pub(in crate::canvas) fn capture(state: Signal<ShellState>) -> Result<Self, String> {
        let s = state.peek();
        if !guarded(&s) || !eligible(&s) {
            return Err("Original replacement fixture unavailable".into());
        }
        let private = path(&s).ok_or("Original private database unavailable")?;
        let e = s
            .canvas_checklist
            .comment_replies
            .entries
            .iter()
            .find(|e| {
                e.disk.account == ACCOUNT
                    && e.disk.path.as_ref() == Some(&private)
                    && e.disk.scope.file == "Ffixture"
                    && e.root == TS
            })
            .ok_or("Original retained entry missing")?;
        Ok(Self {
            id: e.id.clone(),
            disk: e.disk.clone(),
            root: e.root.clone(),
            owner: Owner::capture(&s).ok_or("Original owner missing")?,
            completion: e
                .native
                .as_ref()
                .ok_or("Original native mount missing")?
                .fixture_completion(),
        })
    }
    fn entry<'a>(&self, s: &'a ShellState) -> Result<&'a Entry, String> {
        if !guarded(s) {
            return Err("Replacement fixture retired".into());
        }
        entry(s, &self.id)
            .filter(|e| e.disk == self.disk && e.root == self.root)
            .ok_or_else(|| "Original retained entry replaced".into())
    }
    pub(in crate::canvas) fn runtime(&self, state: Signal<ShellState>) -> Result<Value, String> {
        let s = state.peek();
        let e = self.entry(&s)?;
        Ok(
            json!({"id":e.id,"account":e.disk.account,"workspace":e.disk.scope.workspace,
            "user":e.disk.scope.user,"file":e.disk.scope.file,"root":e.root,
            "text":e.text,"blocks":e.blocks,"capture":e.capture,"serial":e.serial,
            "saving":e.saving,"loading":e.loading,"sending":e.sending,"error":e.error,
            "nativePresent":e.native.is_some(),"nativeBusy":e.native_busy,
            "originalOwnerCurrent":self.owner.current(&s),
            "owner":e.native.as_ref().map(|n|n.fixture_owner(Some(&self.completion)))}),
        )
    }
    pub(in crate::canvas) async fn snapshot(
        &self,
        state: Signal<ShellState>,
    ) -> Result<Value, String> {
        let runtime = self.runtime(state)?;
        let journal = journal(&self.disk, &self.root).await?;
        self.entry(&state.peek())?;
        Ok(json!({"runtime":runtime,"journal":journal}))
    }
}
async fn journal(disk: &Disk, root: &str) -> Result<Value, String> {
    let scope = disk.scope.clone();
    let root = root.to_owned();
    disk.work(move |j|{let i=j.reply_inventory(&scope,&root)?;let h=j.reply_history(&scope,None)?;
        Ok(json!({"drafts":i.drafts(),"attempt":i.attempt(),
            "history":h.iter().map(|v|json!({"receipt":v.receipt,"status":v.status,"observed":v.observed})).collect::<Vec<_>>()}))})
        .await.map_err(|_|"Replacement private journal read failed".into())
}

pub(in crate::canvas) struct Replacement {
    b: Option<ShellState>,
    b_disk: Disk,
    epoch: u64,
    original: crate::model::CanvasTarget,
}
impl Replacement {
    pub(in crate::canvas) async fn prepare(state: Signal<ShellState>) -> Result<Self, String> {
        let (base, epoch, original) = {
            let s = state.peek();
            if !guarded(&s) || s.core.active_account.as_deref() != Some(ACCOUNT) {
                return Err("Original A missing".into());
            }
            (
                path(&s)
                    .ok_or("Original path missing")?
                    .parent()
                    .ok_or("Private directory missing")?
                    .to_owned(),
                s.core.account_epoch,
                s.canvas.clone().ok_or("Original Canvas missing")?,
            )
        };
        let private = std::env::var_os("SUPER_PLATINUM_CANVAS_REPLACEMENT_REPLY_DB")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && *p == base.join("replacement-b.sqlite"))
            .ok_or("Independent B database path missing")?;
        let disk = Disk {
            account: B.into(),
            scope: DiskScope {
                workspace: "T1".into(),
                user: B_USER.into(),
                file: B_FILE.into(),
            },
            path: Some(private),
        };
        let scope = disk.scope.clone();
        let inventory=disk.work(move|j|{
            let blocks=vec![json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Independent B reply stays here.","style":{"italic":true}}]}]})];
            j.save_rich_reply_draft(&scope,TS,None,B_CAPTURE,blocks)?;
            j.reply_inventory(&scope,TS)
        }).await.map_err(|_|"Independent B seed failed")?;
        let draft = inventory
            .drafts()
            .first()
            .ok_or("Independent B draft absent")?;
        let mut b = ShellState::fixture(crate::media::MediaRegistry::default());
        b.core.active_account = Some(B.into());
        b.core.account_epoch = epoch.checked_add(1).ok_or("Epoch exhausted")?;
        b.self_account.user_id = B_USER.into();
        b.core
            .workspaces
            .get_mut("T1")
            .ok_or("B workspace absent")?
            .self_user_id = B_USER.into();
        b.canvas = None;
        b.overlay = None;
        b.canvas_checklist.comment_replies.entries.push(Entry {
            id: "replacement-b-entry".into(),
            disk: disk.clone(),
            root: TS.into(),
            text: draft.text().into(),
            blocks: draft.blocks().map(<[_]>::to_vec),
            capture: draft.capture().into(),
            serial: 0,
            inventory: Some(inventory),
            native: None,
            loading: false,
            saving: false,
            sending: false,
            composing: false,
            native_busy: false,
            error: None,
            reviewed: false,
            outcome: None,
            locked: None,
            history: None,
            retired_text: None,
            retired_blocks: None,
        });
        Ok(Self {
            b: Some(b),
            b_disk: disk,
            epoch,
            original,
        })
    }
    pub(in crate::canvas) fn install_b(&mut self, state: Signal<ShellState>) -> Result<(), String> {
        {
            let s = state.peek();
            if !guarded(&s)
                || s.core.active_account.as_deref() != Some(ACCOUNT)
                || s.core.account_epoch != self.epoch
            {
                return Err("Original replacement A retired".into());
            }
        }
        let b = self.b.take().ok_or("B already installed")?;
        crate::bootstrap::install_loaded(state, b);
        Ok(())
    }
    pub(in crate::canvas) fn return_a(&self, state: Signal<ShellState>) -> Result<(), String> {
        {
            let s = state.peek();
            if !guarded(&s)
                || s.core.active_account.as_deref() != Some(B)
                || s.core.account_epoch != self.epoch + 1
            {
                return Err("Original replacement B retired".into());
            }
        }
        let mut a = ShellState::fixture(crate::media::MediaRegistry::default());
        a.core.active_account = Some(ACCOUNT.into());
        a.core.account_epoch = self.epoch.checked_add(2).ok_or("Epoch exhausted")?;
        a.canvas = None;
        a.overlay = None;
        crate::bootstrap::install_loaded(state, a);
        Ok(())
    }
    pub(in crate::canvas) fn reopen(&self, state: Signal<ShellState>) -> Result<(), String> {
        {
            let s = state.peek();
            if !guarded(&s)
                || s.core.active_account.as_deref() != Some(ACCOUNT)
                || s.core.account_epoch != self.epoch + 2
            {
                return Err("Returned A retired".into());
            }
        }
        crate::canvas::open_target(state, self.original.clone());
        Ok(())
    }
    pub(in crate::canvas) async fn b_snapshot(
        &self,
        state: Signal<ShellState>,
    ) -> Result<Value, String> {
        let runtime = {
            let s = state.peek();
            if !guarded(&s) {
                return Err("Replacement fixture retired".into());
            }
            let e = if let Some(b) = self.b.as_ref() {
                entry(b, "replacement-b-entry")
            } else {
                entry(&s, "replacement-b-entry")
            }
            .filter(|e| e.disk == self.b_disk && e.root == TS)
            .ok_or("Independent B entry missing")?;
            json!({"id":e.id,"account":e.disk.account,"workspace":e.disk.scope.workspace,
                "user":e.disk.scope.user,"file":e.disk.scope.file,"root":e.root,"text":e.text,
                "blocks":e.blocks,"capture":e.capture,"serial":e.serial,"loading":e.loading,
                "saving":e.saving,"sending":e.sending,"nativePresent":e.native.is_some(),"error":e.error})
        };
        Ok(json!({"runtime":runtime,"journal":journal(&self.b_disk,TS).await?}))
    }
}
pub(in crate::canvas) fn shell(state: Signal<ShellState>) -> Value {
    let s = state.peek();
    json!({"account":s.core.active_account,"epoch":s.core.account_epoch,
        "team":s.core.active_team,"user":s.self_account.user_id,"visit":s.shortcut_generation,
        "canvas":s.canvas.as_ref().map(|c|&c.id),"noSession":s.core.session.is_none(),
        "noTransport":s.core.transport.is_none(),"replyLedger":super::ledger()})
}
