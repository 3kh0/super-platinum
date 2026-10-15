//! Read-only recovery labels: a schedule receipt is never a delivery receipt.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckKind {
    Send,
    Update,
    Schedule,
}
impl CheckKind {
    pub fn for_operation(op: &Operation) -> Option<Self> {
        match op.kind {
            OperationKind::Send => Some(Self::Send),
            OperationKind::Update => Some(Self::Update),
            OperationKind::Schedule { .. } if op.known_schedule_check() => Some(Self::Schedule),
            _ => None,
        }
    }
    pub fn row(self) -> &'static str {
        match self {
            Self::Send => "delivery",
            Self::Update => "update",
            Self::Schedule => "schedule",
        }
    }
    pub fn attribute(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Update => "update",
            Self::Schedule => "schedule",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Send => "Check delivery",
            Self::Update => "Check update",
            Self::Schedule => "Check schedule",
        }
    }
    pub fn error(self, error: CheckError) -> &'static str {
        if error == CheckError::Pending {
            return "A previous status check is unresolved. It will not be repeated; the saved draft and current text are retained.";
        }
        match self {
            Self::Send => match error {
                CheckError::Cooldown(_) => {
                    "Please wait before checking delivery again. The saved cooldown is still active."
                }
                CheckError::Fault(Fault::Unsupported) => {
                    "Connect to Slack before checking delivery."
                }
                CheckError::Fault(Fault::Stale) => {
                    "Saved delivery state changed. Refresh Drafts to review it."
                }
                _ => {
                    "Delivery status could not be saved. Check delivery is paused until storage is available; nothing was sent again."
                }
            },
            Self::Update => match error {
                CheckError::Cooldown(_) => {
                    "Please wait before checking the update again. The saved cooldown is still active."
                }
                _ => {
                    "Update status could not be saved. Check update is paused until storage is available; nothing was written again."
                }
            },
            Self::Schedule => match error {
                CheckError::Cooldown(_) => {
                    "Please wait before checking the schedule again. The saved cooldown is still active."
                }
                CheckError::Fault(Fault::Unsupported) => {
                    "Connect to Slack before checking the schedule."
                }
                CheckError::Fault(Fault::Stale) => {
                    "Saved schedule state changed. Refresh Drafts to review it."
                }
                _ => {
                    "Schedule status could not be saved. Check schedule is paused until storage is available; nothing was scheduled again."
                }
            },
        }
    }
}
