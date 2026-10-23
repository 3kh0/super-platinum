//! One ordered rNCs plain preparation path. Staged observations do not grant
//! workspace freshness, native-buffer ownership, or permission to send.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparationSettings {
    /// Actual rNCs N: explicit override or strict emoji_mode == as_text.
    /// Ordinary own Edit supplies no override. Also passed to tEEs/fhSo.
    pub is_emoji_displayed_as_text: bool,
    /// Positively observed combined composer_smart_symbols/disable_smart_symbols gate.
    pub add_text_presentation_selectors: bool,
    /// Actual plain-mode source call argument; separate from the gate.
    pub skip_markdown_code: bool,
    pub use_channel_tagging: bool,
}
#[derive(Clone, Debug)]
pub struct PlainPreparation {
    original: Vec<Value>,
    settings: PreparationSettings,
    syntax_observations: SyntaxObservations,
    emoji_observations: EmojiObservations,
    syntax: Vec<Value>,
    symbols: Vec<Value>,
    emoji: Vec<Value>,
    mentions: MentionPlan,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PreparationStep {
    Need(MentionNeed),
    Complete(PreparedPlain),
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedPlain {
    original: Vec<Value>,
    processed: Vec<Value>,
    choices: Vec<MentionChoice>,
}
impl PlainPreparation {
    pub fn capture(
        ops: &[Value],
        settings: PreparationSettings,
        syntax: &SyntaxObservations,
        emoji: &EmojiObservations,
    ) -> Result<Self> {
        let syntax_observations = syntax.clone();
        let emoji_observations = emoji.clone();
        let syntax = PlainSyntax::capture(ops, syntax)?.prepared;
        let symbols = if settings.add_text_presentation_selectors {
            add_text_presentation_selectors(&syntax, settings.skip_markdown_code)?
        } else {
            syntax.clone()
        };
        let emoji = if settings.is_emoji_displayed_as_text {
            replace_colon_emoji(&symbols, true, emoji)?
        } else {
            symbols.clone()
        };
        let mentions = MentionPlan::capture(
            &emoji,
            settings.use_channel_tagging,
            settings.is_emoji_displayed_as_text,
        )?;
        Ok(Self {
            original: ops.to_vec(),
            settings,
            syntax_observations,
            emoji_observations,
            syntax,
            symbols,
            emoji,
            mentions,
        })
    }
    pub fn matches_input(&self, ops: &[Value], settings: &PreparationSettings) -> bool {
        self.original == ops && self.settings == *settings
    }
    pub fn original_ops(&self) -> &[Value] {
        &self.original
    }
    pub fn syntax_observations(&self) -> &SyntaxObservations {
        &self.syntax_observations
    }
    pub fn emoji_observations(&self) -> &EmojiObservations {
        &self.emoji_observations
    }
    pub fn syntax_ops(&self) -> &[Value] {
        &self.syntax
    }
    pub fn symbol_ops(&self) -> &[Value] {
        &self.symbols
    }
    pub fn emoji_ops(&self) -> &[Value] {
        &self.emoji
    }
    pub fn mention_plan(&self) -> &MentionPlan {
        &self.mentions
    }
    pub fn advance(&self, observed: &MentionObservations) -> Result<PreparationStep> {
        match self.mentions.advance(observed)? {
            MentionStep::Need(need) => Ok(PreparationStep::Need(need)),
            MentionStep::Complete { ops, choices } => {
                let mut document = Document::capture(&ops)?;
                links::linkify(&mut document, true)?;
                Ok(PreparationStep::Complete(PreparedPlain {
                    original: self.original.clone(),
                    processed: document.ops()?,
                    choices,
                }))
            }
        }
    }
}
impl PreparedPlain {
    pub fn original_ops(&self) -> &[Value] {
        &self.original
    }
    pub fn processed_ops(&self) -> &[Value] {
        &self.processed
    }
    pub fn choices(&self) -> &[MentionChoice] {
        &self.choices
    }
    pub fn encoding(&self, headers: bool) -> Result<EncodePlan> {
        EncodePlan::capture(
            &self.processed,
            crate::slack_lists::comments::edit_seed::mode::EditMode::PlainText,
            headers,
        )
    }
    pub fn comparison(&self, original_seed: &[Value], unfurled: &Value) -> Result<EditComparison> {
        EditComparison::capture(original_seed, &self.original, &self.processed, unfurled)
    }
}

#[cfg(test)]
mod tests;
