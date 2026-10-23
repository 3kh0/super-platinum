use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodingNeed {
    Emoji(String),
    File(String),
    UnicodeName(String),
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EmojiEncoding {
    pub name: Option<String>,
    pub unicode: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct EncodingObservations {
    /// None is positively custom; Some is ce1x standard plus actual UHpe.i8.
    pub emoji: BTreeMap<String, Option<EmojiEncoding>>,
    /// None is positively absent file; Some is exact XNVs.fileIsList.
    pub files: BTreeMap<String, Option<bool>>,
    /// Exact UHpe.qJ result for a reached valid Unicode string; None is known absent.
    pub unicode_names: BTreeMap<String, Option<String>>,
}
pub(super) struct Context<'a> {
    pub observed: &'a EncodingObservations,
    pub need: Option<EncodingNeed>,
    pub budget: budget::Budget,
}
impl Context<'_> {
    pub fn emoji(&mut self, text: &str) -> Result<Option<EmojiEncoding>> {
        let Some(value) = self.observed.emoji.get(text) else {
            self.budget.reserve(text, 1)?;
            self.need = Some(EncodingNeed::Emoji(text.into()));
            return Err(SeedUnavailable::InvalidObservation);
        };
        if let Some(value) = value {
            self.budget.reserve(&(&value.name, &value.unicode), 1)?;
        }
        Ok(value.clone())
    }
    pub fn file(&mut self, id: &str) -> Result<bool> {
        if let Some(value) = self.observed.files.get(id) {
            return Ok(value.unwrap_or(false));
        }
        self.budget.reserve(id, 1)?;
        self.need = Some(EncodingNeed::File(id.into()));
        Err(SeedUnavailable::InvalidObservation)
    }
    pub fn unicode_name(&mut self, code: &str) -> Result<Option<String>> {
        let Some(value) = self.observed.unicode_names.get(code) else {
            self.budget.reserve(code, 1)?;
            self.need = Some(EncodingNeed::UnicodeName(code.into()));
            return Err(SeedUnavailable::InvalidObservation);
        };
        self.budget.reserve(value, 1)?;
        Ok(value.clone())
    }
    pub fn reserve<T: serde::Serialize + ?Sized>(
        &mut self,
        value: &T,
        copies: usize,
    ) -> Result<()> {
        self.budget.reserve(value, copies)
    }
    pub fn append(&mut self, target: &mut String, value: &str) -> Result<()> {
        target
            .len()
            .checked_add(value.len())
            .filter(|n| *n <= budget::LIMIT)
            .ok_or(SeedUnavailable::Limit)?;
        self.reserve(value, 1)?;
        target.push_str(value);
        Ok(())
    }
}
