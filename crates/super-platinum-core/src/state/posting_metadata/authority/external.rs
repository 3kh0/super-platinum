//! Owned access to j08h routing history; never a posting permission.
use super::*;
use crate::slack::PreparedRequest;
use std::time::Instant;

impl Authority {
    /// Match the actual request's transport and credentials at dispatch time.
    /// A later callback cannot authenticate an earlier request by recapturing.
    pub fn capture_for_request(
        core: &CoreAppState,
        transport: &Transport,
        session: &WorkspaceSession,
    ) -> Result<Self> {
        let authority = Self::capture(core, &session.team_id)?;
        if !std::ptr::eq(authority.scope.transport.as_ref(), transport)
            || !same_credentials(&authority.scope.session, session)
        {
            return Err(Unknown::Identity);
        }
        Ok(authority)
    }
    /// Adopt only explicitly present connection arrays from actual accepted
    /// channel metadata responses. Capture this Authority before the request;
    /// caller also fences its query and expected response IDs. The Channel
    /// model retains these arrays in `extra`; no default boolean or serialized
    /// model is supplied to posting policy. This does not refresh RawChannel.
    pub fn observe_external_channel_connections(
        &self,
        core: &mut CoreAppState,
        channels: &[crate::slack::models::Channel],
    ) -> Result<()> {
        let store = &mut self.metadata_mut(core)?.external;
        for channel in channels {
            if !id(&channel.id) {
                store.retire();
                return Err(Unknown::Identity);
            }
            let mut raw = serde_json::Map::new();
            for key in ["connected_team_ids", "connected_limited_team_ids"] {
                if let Some(value) = channel.extra.get(key) {
                    raw.insert(key.into(), value.clone());
                }
            }
            store.channel(&Value::Object(raw))?;
        }
        Ok(())
    }
    /// False means absence from this initialized local positive store. It does
    /// not classify the ID as an internal workspace. Keep the witness through
    /// final dispatch: any later positive or validation expiry retires it.
    pub fn external_team(&self, core: &CoreAppState, team: &str) -> Result<ExternalTeam> {
        let store = &self.metadata(core)?.external;
        Ok(ExternalTeam {
            authority: self.clone(),
            team: team.into(),
            value: store.contains(team)?,
            revision: store.revision(),
        })
    }
    pub fn external_validation(&self, core: &CoreAppState) -> Result<ExternalValidation> {
        self.metadata(core)?.external.status(Instant::now())
    }
    /// Reserve once before await. The picker scheduler owns query cancellation,
    /// network execution and Retry-After; dropping this descriptor requires
    /// `failed` to release the pending lease while retaining the throttle.
    pub fn begin_external_validation(
        &self,
        core: &mut CoreAppState,
    ) -> Result<ExternalValidationRead> {
        let nonce = self
            .metadata_mut(core)?
            .external
            .begin_validation(Instant::now())?;
        Ok(ExternalValidationRead {
            authority: self.clone(),
            nonce,
        })
    }
    /// Only actual accepted Xgp+.Is/1Nz1.OU external-team search objects belong
    /// here. Ordinary team info, directory IDs and internal absence do not.
    /// Caller validates query owner, success envelope and full page before
    /// passing its raw objects. The scope captured before that read must remain
    /// current; never capture an Authority only at callback time.
    pub fn observe_external_search_objects(
        &self,
        core: &mut CoreAppState,
        objects: &[Value],
    ) -> Result<()> {
        let ids: Vec<_> = objects
            .iter()
            .map(|raw| {
                raw.get("id")
                    .and_then(Value::as_str)
                    .filter(|v| id(v))
                    .ok_or(Unknown::Shape)
            })
            .collect::<Result<_>>()?;
        let store = &mut self.metadata_mut(core)?.external;
        for id in ids {
            store.positive(id)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct ExternalTeam {
    authority: Authority,
    team: String,
    value: bool,
    revision: Arc<()>,
}
impl ExternalTeam {
    pub fn value(&self) -> bool {
        self.value
    }
    pub fn current(&self, core: &CoreAppState) -> bool {
        self.authority.metadata(core).is_ok_and(|metadata| {
            metadata.external.current(&self.revision)
                && metadata.external.contains(&self.team) == Ok(self.value)
        })
    }
}
impl fmt::Debug for ExternalTeam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ExternalTeam { redacted }")
    }
}
pub struct ExternalValidationRead {
    authority: Authority,
    nonce: Arc<()>,
}
impl ExternalValidationRead {
    pub fn current(&self, core: &CoreAppState) -> bool {
        self.authority
            .metadata(core)
            .is_ok_and(|m| m.external.validation_current(&self.nonce))
    }
    pub fn request(&self, core: &CoreAppState) -> Result<PreparedRequest> {
        if !self.current(core) {
            return Err(Unknown::Stale);
        }
        Ok(core.client.rest_form(
            &self.authority.scope.session,
            "users.slackConnect.orgList",
            vec![
                ("connection_type", "all".into()),
                ("limit", "1".into()),
                ("sort", "count".into()),
            ],
        ))
    }
    /// Consume one actual success envelope. No pagination: CZZ0 intentionally
    /// takes only orgs[0].team_summary.id and never removes earlier positives.
    pub fn apply(self, core: &mut CoreAppState, response: &Value) -> Result<()> {
        self.authority
            .metadata_mut(core)?
            .external
            .finish_validation(&self.nonce, Some(response), Instant::now())
    }
    pub fn failed(self, core: &mut CoreAppState) -> bool {
        if !self.current(core) {
            return false;
        }
        let _ = self
            .authority
            .metadata_mut(core)
            .unwrap()
            .external
            .finish_validation(&self.nonce, None, Instant::now());
        true
    }
}
