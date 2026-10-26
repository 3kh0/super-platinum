//! Explicit application presentation; this does not observe Slack's locale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanvasReferenceText {
    _english: (),
}
impl CanvasReferenceText {
    pub const fn english() -> Self {
        Self { _english: () }
    }
    pub const fn private_canvas(self) -> &'static str {
        "Private canvas"
    }
    pub const fn deleted_canvas(self) -> &'static str {
        "Deleted canvas"
    }
}
