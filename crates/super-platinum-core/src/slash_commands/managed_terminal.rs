//! Sealed transport outcome for the private managed storage path. No HTTP admission.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Origin {
    BeforeExecute,
    SlackReply,
    Uncertain,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManagedEvidence {
    version: u32,
    dispatch: Record,
    terminal: Record,
    origin: Origin,
}
impl ManagedEvidence {
    pub(crate) fn decode(raw: &str) -> Result<Self, Failure> {
        let value = codec::decode(raw.as_bytes(), 3 * 1024 * 1024).map_err(|_| Failure::Invalid)?;
        let evidence: Self = serde_json::from_value(value).map_err(|_| Failure::Invalid)?;
        evidence.validate()?;
        Ok(evidence)
    }
    fn validate(&self) -> Result<(), Failure> {
        Controller::restore(self.dispatch.clone())?;
        Controller::restore(self.terminal.clone())?;
        if self.version != 1
            || self.dispatch.phase != Phase::DispatchPossible
            || self.dispatch.retired
            || self.terminal.retired
            || self.dispatch.id != self.terminal.id
            || self.dispatch.frozen != self.terminal.frozen
            || self.dispatch.version.checked_add(1) != Some(self.terminal.version)
            || !match self.origin {
                Origin::BeforeExecute => self.terminal.phase == Phase::NotSent,
                Origin::SlackReply => matches!(self.terminal.phase, Phase::Completed(_)),
                Origin::Uncertain => self.terminal.phase == Phase::Unknown,
            }
        {
            return Err(Failure::Invalid);
        }
        Ok(())
    }
    pub(crate) fn matches(&self, dispatch: &Record, terminal: &Record) -> bool {
        self.dispatch == *dispatch && self.terminal == *terminal
    }
    pub(crate) fn terminal(&self) -> &Record {
        &self.terminal
    }
    pub(crate) fn before_execute(&self) -> bool {
        self.origin == Origin::BeforeExecute
    }
}
/// Neither Clone nor Deserialize. Only a sealed transport Report constructs this.
/// The managed queue retains that original Report while deriving flush-only evidence.
pub(crate) struct ManagedTerminal {
    evidence: ManagedEvidence,
}
impl ManagedTerminal {
    pub(crate) fn dispatch(&self) -> &Record {
        &self.evidence.dispatch
    }
    pub(crate) fn terminal(&self) -> &Record {
        &self.evidence.terminal
    }
    pub(crate) fn encoded(&self) -> Result<String, Failure> {
        serde_json::to_string(&self.evidence).map_err(|_| Failure::Persistence)
    }
}
pub(super) fn derive(dispatch: &Record, report: Report) -> Result<ManagedTerminal, Failure> {
    derive_retained(dispatch, &report)
}
pub(super) fn derive_retained(
    dispatch: &Record,
    report: &Report,
) -> Result<ManagedTerminal, Failure> {
    if dispatch.phase != Phase::DispatchPossible
        || dispatch.retired
        || report.id != dispatch.id
        || report.version != dispatch.version
    {
        return Err(Failure::Stale);
    }
    let mut terminal = dispatch.clone();
    terminal.version = terminal.version.checked_add(1).ok_or(Failure::Invalid)?;
    terminal.retry_after_secs = report.retry_after_secs;
    let origin = match &report.delivery {
        Delivery::NotSent => {
            terminal.phase = Phase::NotSent;
            Origin::BeforeExecute
        }
        Delivery::Reply(raw) => {
            terminal.phase = Phase::Completed(api::decode(raw.clone()));
            Origin::SlackReply
        }
        Delivery::HttpFailure(status) => {
            terminal.http_status = Some(*status);
            terminal.phase = Phase::Unknown;
            Origin::Uncertain
        }
        Delivery::Unknown => {
            terminal.phase = Phase::Unknown;
            Origin::Uncertain
        }
    };
    let evidence = ManagedEvidence {
        version: 1,
        dispatch: dispatch.clone(),
        terminal,
        origin,
    };
    evidence.validate()?;
    Ok(ManagedTerminal { evidence })
}
impl Controller {
    /// Does not commit or rearm. The managed host retains this for flush-only retry.
    pub(crate) fn finish_managed(&self, report: Report) -> Result<ManagedTerminal, Failure> {
        if self.recovered || self.pending.is_some() {
            return Err(Failure::NoReplay);
        }
        derive(&self.record, report)
    }
}
#[cfg(test)]
pub(crate) fn isolated_terminal(
    dispatch: &Record,
    delivery: Delivery,
    retry: Option<u64>,
) -> ManagedTerminal {
    derive(
        dispatch,
        Report {
            id: dispatch.id.clone(),
            version: dispatch.version,
            delivery,
            retry_after_secs: retry,
        },
    )
    .unwrap()
}
impl Record {
    pub(crate) fn protected_retirement(&self) -> Result<Self, Failure> {
        Controller::restore(self.clone())?;
        if !matches!(
            self.phase,
            Phase::NotSent
                | Phase::Completed(Reply::Acknowledged { .. })
                | Phase::Completed(Reply::Rejected { .. })
        ) {
            return Err(Failure::NoReplay);
        }
        if self.retired {
            return Ok(self.clone());
        }
        let mut next = self.clone();
        next.version = next.version.checked_add(1).ok_or(Failure::Invalid)?;
        next.retired = true;
        Ok(next)
    }
}
