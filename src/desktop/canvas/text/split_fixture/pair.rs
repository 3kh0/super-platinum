//! Fixture-only immutable completion dispatch; product authorities stay distinct.
use super::*;
use super_platinum_core::canvas::edit::{
    Error,
    structure::{AcceptedEnd, AcceptedSplit},
};
pub(super) enum PreparedPair {
    Middle(PreparedSplitEdit),
    End(PreparedEndEdit),
}
impl PreparedPair {
    pub(super) fn document_data(&self) -> &RawMessage {
        match self {
            Self::Middle(p) => p.document_data(),
            Self::End(p) => p.document_data(),
        }
    }
    pub(super) fn validate_ack(&self, ack: &EditAck) -> Result<AcceptedPair, Error> {
        match self {
            Self::Middle(p) => p.validate_ack(ack).map(AcceptedPair::Middle),
            Self::End(p) => p.validate_ack(ack).map(AcceptedPair::End),
        }
    }
}
pub(super) enum AcceptedPair {
    Middle(AcceptedSplit),
    End(AcceptedEnd),
}
impl AcceptedPair {
    pub(super) fn confirms_readback(&self, snapshot: &CanvasCollabSnapshot) -> Result<bool, Error> {
        match self {
            Self::Middle(a) => a.confirms_readback(snapshot),
            Self::End(a) => a.confirms_readback(snapshot),
        }
    }
    pub(super) fn finish(self, shell: &mut ShellState, attempt: &checklist::Attempt) -> bool {
        match self {
            Self::Middle(a) => checklist::split_ops::finish_split(shell, attempt, a),
            Self::End(a) => checklist::end_ops::finish_end(shell, attempt, a),
        }
    }
}
