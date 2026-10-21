//! Named synthetic session with a disconnected transport: no HTTP capability.
use super::*;
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{
        Transport,
        client::{PreparedRequest, RequestBody},
    },
    state::posting_metadata::authority::BootRead,
};
mod policy;
pub(crate) mod recovery;
mod seed;
pub(in crate::canvas) use policy::{held_closed, hold, release, request};

pub(in crate::canvas) const TS: &str = "1719800000.000100";
pub(in crate::canvas) const SELF: &str = "UATTRSELF";
pub(in crate::canvas) const EDITOR: &str = "UATTR0001";
pub(in crate::canvas) const OTHER: &str = "UATTR0002";
pub(in crate::canvas) const EMPTY: &str = "UATTR0003";
pub(in crate::canvas) const UNKNOWN: &str = "UATTR0009";
pub(in crate::canvas) const AUTHOR: &str = "UATTR0004";
pub(in crate::canvas) const REPLY_AUTHOR: &str = "UATTR0005";
const COOKIE: &str = "synthetic-canvas-attribution-cookie";
const TOKEN: &str = "synthetic-canvas-attribution-token";
#[derive(Clone)]
struct Carrier {
    transport: Arc<Transport>,
    session: WorkspaceSession,
    account: Option<String>,
    epoch: u64,
}
#[derive(Default)]
struct Ledger {
    carrier: Option<Carrier>,
    boot: Value,
    root: Option<Value>,
    real: bool,
    hidden: bool,
    policy_requests: Vec<Value>,
    policy_responses: Vec<Value>,
    held: Option<(tokio::sync::oneshot::Sender<Value>, Value)>,
    hold_next: bool,
    releases: Vec<Value>,
    retained: Value,
    original_profile: Option<super_platinum_core::slack::models::User>,
    hydration: Vec<Value>,
}
static DATA: OnceLock<Mutex<Ledger>> = OnceLock::new();
fn data() -> &'static Mutex<Ledger> {
    DATA.get_or_init(Default::default)
}
pub(in crate::canvas) fn requested() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-root-attribution"
                | "canvas-root-attribution-narrow"
                | "canvas-editor-recovery"
                | "canvas-editor-recovery-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) -> Result<(), Error> {
    if !requested() {
        return Ok(());
    }
    if shell.core.session.is_some() || shell.core.transport.is_some() || shell.core.cache.is_some()
    {
        return Err(refused());
    }
    #[cfg(any(test, feature = "protected-native-fixture"))]
    {
        let team = shell.core.active_team.clone().ok_or_else(refused)?;
        let session = WorkspaceSession {
            team_id: team.clone(),
            user_id: SELF.into(),
            enterprise_id: None,
            name: "Canvas attribution".into(),
            url: "https://canvas-attribution-fixture.slack.com".into(),
            token: TOKEN.into(),
        };
        let transport = Arc::new(Transport::disconnected_fixture(COOKIE));
        shell.core.active_account = Some(super::super::reply::fixture::ACCOUNT.into());
        shell.self_account.user_id = SELF.into();
        let workspace = shell.core.workspaces.get_mut(&team).ok_or_else(refused)?;
        workspace.users.clear();
        workspace.self_user_id = SELF.into();
        shell.core.session = Some(Session {
            d_cookie: COOKIE.into(),
            unified_user_client_enabled: BTreeMap::from([(team.clone(), false)]),
            workspaces: BTreeMap::from([(team.clone(), session.clone())]),
        });
        shell.core.transport = Some(transport.clone());
        let members = vec![
            member(SELF, "Canvas Self", "Canvas Self"),
            member(EDITOR, "Editor Display", "Editor Real"),
            member(OTHER, "Other Display", "Other Real"),
            member(EMPTY, "", ""),
            member(AUTHOR, "Original Author", "Original Author"),
            member(REPLY_AUTHOR, "Reply Author", "Reply Author"),
        ];
        let boot = json!({"ok":true,"team":{"id":team,"name":"Canvas attribution","plan":"plus"},"self":members[0],
            "users":members,"workspaces":[{"id":team}],"read_only_channels":[],"thread_only_channels":[],
            "account_types":{"is_admin":[],"is_owner":[],"is_primary_owner":[]}});
        let read =
            BootRead::begin_full(&mut shell.core, &transport, &session).map_err(|_| refused())?;
        let request = wire(&read.request(&shell.core).map_err(|_| refused())?)?;
        if !read.apply(&mut shell.core, boot.clone(), true)? {
            return Err(refused());
        }
        let adopted = seed::members(shell, &session)?;
        for id in [EDITOR, OTHER] {
            let url = format!("https://example.test/canvas-attribution/{id}.png");
            let asset = shell.media.register_avatar(id, &url);
            shell.media.insert(
                asset,
                "image/png",
                include_bytes!("../../../../../assets/icons/icon-512.png").as_slice(),
            );
        }
        let carrier = Carrier {
            transport,
            session,
            account: shell.core.active_account.clone(),
            epoch: shell.core.account_epoch,
        };
        *data().lock().unwrap() = Ledger {
            carrier: Some(carrier),
            boot: json!({"request":request,"response":boot,"accepted":true,"canonicalIntake":adopted}),
            ..Default::default()
        };
        Ok(())
    }
    #[cfg(not(any(test, feature = "protected-native-fixture")))]
    Err(refused())
}
fn member(id: &str, display: &str, real: &str) -> Value {
    json!({"id":id,"team_id":"T1","name":format!("fixture_{id}"),"real_name":real,"deleted":false,
        "is_bot":false,"is_app_user":false,"updated":101,"profile":{"display_name":display,"real_name":real,"image_48":format!("https://example.test/canvas-attribution/{id}.png")}})
}
fn carrier(shell: &ShellState) -> Result<Carrier, Error> {
    let value = data().lock().unwrap().carrier.clone().ok_or_else(refused)?;
    if !requested()
        || shell.core.cache.is_some()
        || shell.core.active_account != value.account
        || shell.core.account_epoch != value.epoch
        || shell.core.active_team.as_deref() != Some(&value.session.team_id)
        || shell.self_account.user_id != SELF
        || !shell
            .core
            .transport
            .as_ref()
            .is_some_and(|t| Arc::ptr_eq(t, &value.transport))
        || shell.core.session.as_ref().is_none_or(|s| {
            s.d_cookie != COOKIE
                || s.workspaces.get(&value.session.team_id).is_none_or(|v| {
                    v.token != TOKEN || v.user_id != SELF || v.url != value.session.url
                })
        })
        || value.transport.http().is_ok()
    {
        return Err(refused());
    }
    Ok(value)
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    carrier(shell).is_ok() && shell.canvas.as_ref().is_some_and(|v| v.id == "Ffixture")
}
pub(in crate::canvas) fn isolation(shell: &ShellState) -> Value {
    json!({"syntheticSession":enabled(shell),"noHttp":enabled(shell),"fixture":if recovery::requested(){"canvas-editor-recovery"}else{"canvas-root-attribution"},"account":shell.core.active_account,"team":shell.core.active_team,"self":shell.self_account.user_id})
}
fn refused() -> Error {
    Error::Offline("Closed Canvas attribution fixture unavailable".into())
}
fn wire(request: &PreparedRequest) -> Result<Value, Error> {
    let RequestBody::Form(fields) = &request.body else {
        return Err(refused());
    };
    Ok(
        json!({"method":request.method,"url":request.url,"fields":fields.iter().filter(|(k,_)|k!="token").collect::<Vec<_>>()}),
    )
}
pub(in crate::canvas) fn root() -> Value {
    data()
        .lock()
        .unwrap()
        .root
        .clone()
        .unwrap_or_else(|| base_root(Some(EDITOR), true))
}
fn base_root(editor: Option<&str>, visible: bool) -> Value {
    let mut row = super::root(TS, "Original Canvas root & retained body.");
    row["user"] = json!(AUTHOR);
    row["document_comment"]["is_visible"] = json!(visible);
    row["document_comment"]["thread_id"] = json!("fixture-attribution-root");
    if let Some(editor) = editor {
        row["document_comment"]["most_recent_editor"] = json!(editor);
    }
    row
}
pub(in crate::canvas) fn configure(
    shell: &ShellState,
    editor: Option<&str>,
    visible: bool,
    real: bool,
    hidden: bool,
) -> Result<(), Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    let mut d = data().lock().unwrap();
    d.root = Some(base_root(editor, visible));
    d.real = real;
    d.hidden = hidden;
    Ok(())
}
pub(super) fn response(ticket: &Ticket) -> Result<Value, Error> {
    match &ticket.query {
        Query::Stream(Filter::Recent) => {
            Ok(json!({"ok":true,"has_more":false,"messages":[root()]}))
        }
        Query::Replies(ts) if ts == TS => {
            let mut reply = super::reply(
                "1719800001.000100",
                TS,
                "Ordinary reply sender remains unchanged.",
            );
            reply["user"] = json!(REPLY_AUTHOR);
            Ok(json!({"ok":true,"has_more":false,"messages":[root(),reply]}))
        }
        _ => Err(refused()),
    }
}
pub(in crate::canvas) fn observe(shell: &ShellState, model: &State) {
    if !enabled(shell) {
        return;
    }
    let rows = model
        .rows
        .iter()
        .map(|r| serde_json::to_value(&r.message).unwrap())
        .collect::<Vec<_>>();
    data().lock().unwrap().retained = json!({"serial":model.serial,"ownerCurrent":model.owner.as_ref().is_some_and(|o|o.current(shell)),"loading":model.loading,"rows":rows,"root":model.root,"attribution":super::super::attribution::snapshot(shell,model)});
}
pub(in crate::canvas) fn proof(shell: &ShellState) -> Value {
    let isolation = isolation(shell);
    let mut proof = {
        let d = data().lock().unwrap();
        json!({"isolation":isolation,"boot":d.boot,"policyRequests":d.policy_requests,"policyResponses":d.policy_responses,"held":d.held.is_some(),"releases":d.releases,"retained":d.retained,"hydration":d.hydration})
    };
    proof["requests"] = json!(super::ledger());
    proof["responses"] = json!(super::responses());
    proof
}
/// Synthetic equivalent of the typed profile/extras publication in discovery.
/// This is deliberately not a canonical metadata-source adoption.
pub(in crate::canvas) fn hydrate(shell: &mut ShellState, restore: bool) -> Result<(), Error> {
    if !enabled(shell) || shell.profile_user.as_deref() != Some(EDITOR) {
        return Err(refused());
    }
    let user = shell
        .core
        .workspaces
        .get_mut("T1")
        .and_then(|w| w.users.get_mut(EDITOR))
        .ok_or_else(refused)?;
    let before = serde_json::to_value(&*user).map_err(|_| refused())?;
    let mut ledger = data().lock().unwrap();
    if restore {
        *user = ledger.original_profile.take().ok_or_else(refused)?;
    } else {
        if ledger.original_profile.is_some() {
            return Err(refused());
        }
        ledger.original_profile = Some(user.clone());
        let mut raw = before.clone();
        raw["profile"]["title"] = json!("Synthetic loaded profile");
        raw["im_mpim_ids"] = json!(["DATTR0001"]);
        raw["has_more_mpims"] = json!(false);
        *user = serde_json::from_value(raw).map_err(|_| refused())?;
    }
    ledger
        .hydration
        .push(json!({"restore":restore,"before":before,"after":user}));
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_author_and_root_identity_do_not_follow_attribution_editor() {
        let a = base_root(Some(EDITOR), true);
        let b = base_root(Some(OTHER), true);
        assert_eq!(a["user"], AUTHOR);
        assert_eq!(a["ts"], b["ts"]);
        assert_eq!(a["user"], b["user"]);
        assert_eq!(a["text"], b["text"]);
        assert_ne!(
            a["document_comment"]["most_recent_editor"],
            b["document_comment"]["most_recent_editor"]
        );
    }
    #[test]
    fn missing_editor_is_absent_while_visibility_is_explicit() {
        let raw = base_root(None, false);
        assert!(raw["document_comment"].get("most_recent_editor").is_none());
        assert_eq!(raw["document_comment"]["is_visible"], false);
        let parsed: Message = serde_json::from_value(raw).unwrap();
        assert_eq!(parsed.user.as_deref(), Some(AUTHOR));
    }
}
