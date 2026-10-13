/// Identity is scoped by the containing block as well as the owning message.
/// Strings are retained exactly; missing/empty IDs are not synthesized here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockElementMetadata {
    pub source: BlockElementSource,
    pub block_id: Option<String>,
    pub action_id: Option<String>,
    pub value: Option<String>,
    pub confirm: BlockConfirmationState,
    pub accessibility_label: Option<String>,
}

/// An inert select/picker must never become an actionable button merely because
/// the renderer currently draws both using the same disabled control shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockElementSource {
    Button,
    UnsupportedControl { kind: String },
}

/// Malformed confirmation data must not be treated as permission to skip it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockConfirmationState {
    Missing,
    Invalid,
    Parsed(BlockConfirmation),
}

/// Required fields remain optional in projection so incomplete incoming data
/// stays visibly incomplete to any future validation/interaction layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockConfirmation {
    pub title: Option<BlockText>,
    pub text: Option<BlockText>,
    pub confirm: Option<BlockText>,
    pub deny: Option<BlockText>,
    /// Preserve unknown styles instead of silently replacing them with default.
    pub style: Option<String>,
}

/// Original composition text, before emoji expansion, escaping, or rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockText {
    pub format: BlockTextFormat,
    pub text: String,
    pub emoji: Option<bool>,
    pub verbatim: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockTextFormat {
    PlainText,
    Mrkdwn,
    Unknown(String),
}
