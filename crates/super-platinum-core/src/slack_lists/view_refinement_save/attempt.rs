use super::*;
pub struct ViewRefinementSaveAttempt {
    intent: ViewRefinementSave,
    phase: ViewRefinementSavePhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewRefinementSavePhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
/// Consumed once; never Clone or serializable. Desktop owns session/draft fences.
pub struct ViewRefinementSavePermit {
    intent: ViewRefinementSave,
}
impl ViewRefinementSavePermit {
    pub(crate) fn into_intent(self) -> ViewRefinementSave {
        self.intent
    }
}
impl ViewRefinementSaveAttempt {
    pub fn new(intent: ViewRefinementSave) -> Self {
        Self {
            intent,
            phase: ViewRefinementSavePhase::Reviewing,
        }
    }
    pub fn intent(&self) -> &ViewRefinementSave {
        &self.intent
    }
    pub fn phase(&self) -> ViewRefinementSavePhase {
        self.phase
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ViewRefinementSavePermit, ListError> {
        if self.phase != ViewRefinementSavePhase::Reviewing {
            return Err(ListError("This Save attempt was already admitted."));
        }
        self.intent.revalidate(fresh, allow, personal)?;
        self.phase = ViewRefinementSavePhase::InFlight;
        Ok(ViewRefinementSavePermit {
            intent: self.intent.clone(),
        })
    }
    /// Finish one dispatched response and hand off a positive ACK child once.
    /// This leaves metadata Unconfirmed, including a rejected ACK shape.
    pub fn acknowledge(&mut self, reply: &Value) -> Option<ViewRefinementAcknowledgement> {
        if self.phase != ViewRefinementSavePhase::InFlight {
            return None;
        }
        self.phase = ViewRefinementSavePhase::Unconfirmed;
        ViewRefinementAcknowledgement::capture(&self.intent, reply)
    }
    /// True requires typed pre-dispatch evidence. ACK and every dispatched failure
    /// are unconfirmed, and neither permits a retry from this attempt.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ViewRefinementSavePhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ViewRefinementSavePhase::NotRequested
        } else {
            ViewRefinementSavePhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> Option<String> {
        if self.phase != ViewRefinementSavePhase::Unconfirmed {
            return None;
        }
        let id = self.intent.observed_view_id(fresh)?.to_owned();
        self.phase = ViewRefinementSavePhase::Observed;
        Some(id)
    }
}
