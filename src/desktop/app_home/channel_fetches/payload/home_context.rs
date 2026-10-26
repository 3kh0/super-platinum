//! Source-owned Home context inputs, independent of the bounded diagnostic log.
use super::*;

#[derive(Default)]
pub(super) struct State {
    init: Option<Arc<Proof>>,
    internal: Option<Accepted<Vec<String>>>,
    selected: Option<Accepted<Option<Value>>>,
    default: Option<Accepted<Option<Value>>>,
}

struct Proof {
    owner: Owner,
    kind: source::PayloadKind,
    // Retain the actual accepted carrier, not just a reconstructed store value.
    _bytes: Arc<str>,
    effect: Option<source::Effect>,
    complete: bool,
}
#[derive(Clone)]
struct Accepted<T> {
    proof: Arc<Proof>,
    value: T,
}
impl Proof {
    fn summary(&self) -> Value {
        let effect = self.effect.as_ref().map(|effect| {
            if effect.kind == source::Kind::UserPrefs {
                let mut payload = serde_json::Map::new();
                if let Some(value) = effect.payload.get("workspace_filter_selected") {
                    payload.insert("workspace_filter_selected".into(), value.clone());
                }
                serde_json::json!({"kind":effect.kind,"payload":payload})
            } else {
                serde_json::json!(effect)
            }
        });
        serde_json::json!({
            "payloadKind":match self.kind {source::PayloadKind::Init=>"init",source::PayloadKind::Extras=>"extras"},
            "effect":effect,"complete":self.complete,
            "observation":if self.effect.is_some(){"applied-effect"}else{"accepted-payload"}
        })
    }
}

/// The normal payload dispatcher calls this after its synchronous effects.
/// A failed dispatch can have applied effects; those keep their real provenance,
/// but a failed Init never grants a complete initialization observation.
pub(super) fn settled(shell: &mut ShellState, owner: &Owner, receipt: &source::Settlement) {
    if !owner.current(shell) {
        return;
    }
    let ctx = &owner.context;
    let stores = &shell.core.workspaces[&ctx.session.team_id].cfm_payload;
    let selected = stores.user_prefs.get("workspace_filter_selected").cloned();
    let default = stores.default_workspace.clone();
    let internal = stores.internal_workspace_ids.clone();
    let payload = &mut shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&ctx.key())
        .unwrap()
        .payload;
    let state = &mut payload.home_context;
    let bytes: Arc<str> = receipt.original_bytes().into();
    let proof = |effect| {
        Arc::new(Proof {
            owner: owner.clone(),
            kind: receipt.kind(),
            _bytes: bytes.clone(),
            effect,
            complete: receipt.result().is_ok(),
        })
    };
    if receipt.kind() == source::PayloadKind::Init {
        state.init = receipt.result().is_ok().then(|| proof(None));
    }
    for effect in receipt.applied() {
        match effect.kind {
            source::Kind::InternalWorkspaces => {
                if let Some(ids) = &payload.workspace_ids {
                    if internal.as_ref() == Some(&serde_json::json!(ids))
                        && effect.payload.get("internalIds") == internal.as_ref()
                    {
                        state.internal = Some(Accepted {
                            proof: proof(Some(effect.clone())),
                            value: ids.clone(),
                        });
                    }
                }
            }
            source::Kind::UserPrefs => {
                // A partial prefs merge cannot certify an inherited, unobserved
                // nonempty value. An explicit key or actual missing key can.
                if effect.payload.get("workspace_filter_selected") == selected.as_ref() {
                    state.selected = Some(Accepted {
                        proof: proof(Some(effect.clone())),
                        value: selected.clone(),
                    });
                }
            }
            source::Kind::DefaultWorkspace => {
                if effect.payload.get("id") == default.as_ref() {
                    state.default = Some(Accepted {
                        proof: proof(Some(effect.clone())),
                        value: default.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    // Observed absence is available only after the responsible accepted
    // acquisition. A default State or warm Store alone is never sufficient.
    if receipt.result().is_ok() {
        let fresh_owner = |p: &Arc<Proof>| {
            Arc::ptr_eq(&p.owner.token, &owner.token)
                && Arc::ptr_eq(&p.owner.context, &owner.context)
        };
        if receipt.kind() == source::PayloadKind::Init
            && selected.is_none()
            && !state
                .selected
                .as_ref()
                .is_some_and(|v| fresh_owner(&v.proof) && v.value.is_none())
        {
            state.selected = Some(Accepted {
                proof: proof(None),
                value: None,
            });
        }
        if receipt.kind() == source::PayloadKind::Extras
            && default.is_none()
            && !state
                .default
                .as_ref()
                .is_some_and(|v| fresh_owner(&v.proof) && v.value.is_none())
        {
            state.default = Some(Accepted {
                proof: proof(None),
                value: None,
            });
        }
    }
}

/// No acquisition or mutation authority. Missing differs from observed null.
#[derive(Clone)]
pub(crate) struct Observation {
    owner: Owner,
    init: Arc<Proof>,
    internal: Accepted<Vec<String>>,
    selected: Accepted<Option<Value>>,
    default: Accepted<Option<Value>>,
    rows: BTreeMap<String, Value>,
}
impl Observation {
    pub(crate) fn capture(shell: &ShellState, team: &str) -> Result<Self, Error> {
        let context = Context::capture(shell, team)?;
        let slot = shell
            .app_home
            .channel_fetches
            .slots
            .get(&context.key())
            .ok_or_else(unavailable)?;
        let state = &slot.payload.home_context;
        let observed = Self {
            owner: Owner {
                context: slot.context.clone(),
                token: slot.token.clone(),
            },
            init: state.init.clone().ok_or_else(unavailable)?,
            internal: state.internal.clone().ok_or_else(unavailable)?,
            selected: state.selected.clone().ok_or_else(unavailable)?,
            default: state.default.clone().ok_or_else(unavailable)?,
            rows: slot.payload.teams.clone(),
        };
        observed
            .current(shell)
            .then_some(observed)
            .ok_or_else(unavailable)
    }
    pub(crate) fn internal_workspace_ids(&self) -> &[String] {
        &self.internal.value
    }
    pub(crate) fn workspace_filter_selected(&self) -> Option<&Value> {
        self.selected.value.as_ref()
    }
    pub(crate) fn default_workspace(&self) -> Option<&Value> {
        self.default.value.as_ref()
    }
    pub(crate) fn rows(&self) -> &BTreeMap<String, Value> {
        &self.rows
    }
    /// Relevant effects only: never expose the full Init carrier or credentials.
    pub(crate) fn provenance(&self) -> Value {
        serde_json::json!({"source":"accepted-cfm-payload","init":self.init.summary(),
            "internalWorkspaces":self.internal.proof.summary(),
            "workspaceFilterSelected":self.selected.proof.summary(),"defaultWorkspace":self.default.proof.summary()})
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        if !self.owner.current(shell)
            || [
                &self.init,
                &self.internal.proof,
                &self.selected.proof,
                &self.default.proof,
            ]
            .iter()
            .any(|p| !p.owner.current(shell))
        {
            return false;
        }
        let ctx = &self.owner.context;
        let slot = &shell.app_home.channel_fetches.slots[&ctx.key()];
        let state = &slot.payload.home_context;
        let stores = &shell.core.workspaces[&ctx.session.team_id].cfm_payload;
        state
            .init
            .as_ref()
            .is_some_and(|p| Arc::ptr_eq(p, &self.init))
            && state.internal.as_ref().is_some_and(|v| {
                Arc::ptr_eq(&v.proof, &self.internal.proof) && v.value == self.internal.value
            })
            && state.selected.as_ref().is_some_and(|v| {
                Arc::ptr_eq(&v.proof, &self.selected.proof) && v.value == self.selected.value
            })
            && state.default.as_ref().is_some_and(|v| {
                Arc::ptr_eq(&v.proof, &self.default.proof) && v.value == self.default.value
            })
            && slot.payload.workspace_ids.as_deref() == Some(self.internal.value.as_slice())
            && stores.internal_workspace_ids.as_ref()
                == Some(&serde_json::json!(self.internal.value))
            && stores.user_prefs.get("workspace_filter_selected") == self.selected.value.as_ref()
            && stores.default_workspace == self.default.value
            && slot.payload.teams == self.rows
    }
}

#[cfg(test)]
mod tests;
