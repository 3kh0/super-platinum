//! Local submit-only scalar editing. A keyed control owns one presentation;
//! remote same-ID updates keep that presentation, close/reopen does not.
use super::*;
use crate::realtime::activity_counts::CursorOwner;
use std::{
    rc::{Rc, Weak},
    sync::Arc,
};
use super_platinum_core::block_actions::form::InputKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Email,
    Url,
    Integer,
}
impl Kind {
    fn matches(self, kind: &InputKind) -> bool {
        matches!(
            (self, kind),
            (Self::Email, InputKind::Email)
                | (Self::Url, InputKind::Url)
                | (
                    Self::Integer,
                    InputKind::Number {
                        is_decimal_allowed: false,
                        min_value: None,
                        max_value: None
                    }
                )
        )
    }
    fn value(self, literal: String) -> InputValue {
        match self {
            Self::Email => InputValue::Email(Some(literal)),
            Self::Url => InputValue::Url(Some(literal)),
            Self::Integer => InputValue::Number((!literal.is_empty()).then_some(literal)),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Owner {
    context: Option<Arc<CursorOwner>>,
    scope: ModalScope,
    reset: u64,
    view: String,
    key: InputKey,
    field_revision: u64,
    binding: Weak<()>,
    kind: Kind,
}
impl Owner {
    pub(crate) fn capture(shell: &ShellState, key: &InputKey, binding: &Rc<()>) -> Option<Self> {
        Self::capture_kind(shell, key, binding, Kind::Email)
    }
    pub(crate) fn capture_url(
        shell: &ShellState,
        key: &InputKey,
        binding: &Rc<()>,
    ) -> Option<Self> {
        Self::capture_kind(shell, key, binding, Kind::Url)
    }
    pub(crate) fn capture_integer(
        shell: &ShellState,
        key: &InputKey,
        binding: &Rc<()>,
    ) -> Option<Self> {
        Self::capture_kind(shell, key, binding, Kind::Integer)
    }
    fn capture_kind(
        shell: &ShellState,
        key: &InputKey,
        binding: &Rc<()>,
        kind: Kind,
    ) -> Option<Self> {
        let s = shell.app_modal.as_ref()?;
        let f = s.current_form()?;
        let field = f.fields.get(key)?;
        if shell.overlay != Some(Overlay::AppModal)
            || !current_scope(shell, s)
            || !kind.matches(&field.kind)
        {
            return None;
        }
        let context = Context::capture(shell)
            .and_then(|c| CursorOwner::capture(&shell.core, &c.transport, &c.session))
            .map(Arc::new);
        if context.is_none() && !(s.fixture && shell.core.transport.is_none()) {
            return None;
        }
        Some(Self {
            context,
            scope: s.modal.scope().clone(),
            reset: s.reset_revision,
            view: f.view_id.clone(),
            key: key.clone(),
            field_revision: field.revision,
            binding: Rc::downgrade(binding),
            kind,
        })
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        let Some(s) = shell.app_modal.as_ref() else {
            return false;
        };
        self.binding.upgrade().is_some()
            && shell.overlay == Some(Overlay::AppModal)
            && current_scope(shell, s)
            && !s.locked()
            && s.modal.scope() == &self.scope
            && s.reset_revision == self.reset
            && self
                .context
                .as_ref()
                .map_or(s.fixture && shell.core.transport.is_none(), |c| {
                    c.current(&shell.core)
                })
            && s.current_form().is_some_and(|f| {
                f.view_id == self.view
                    && f.unsupported.is_empty()
                    && f.fields.get(&self.key).is_some_and(|field| {
                        field.revision == self.field_revision && self.kind.matches(&field.kind)
                    })
            })
    }
    pub(crate) fn set(&self, mut state: Signal<ShellState>, literal: String) {
        self.apply(&mut state.write(), literal);
    }
    pub(super) fn apply(&self, shell: &mut ShellState, literal: String) {
        if !self.current(shell) {
            return;
        }
        if let Some(form) = shell.app_modal.as_mut().and_then(Session::current_form_mut) {
            // Native strings remain literal, including an explicit empty clear.
            let _ = form.set_value(&self.key, self.kind.value(literal));
        }
    }
}
