use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SplitSequenceTarget {
    Original,
    FirstChild,
}
#[derive(Clone)]
pub(in crate::canvas::edit::structure::split) struct Second {
    pub target: SplitSequenceTarget,
    pub enter: TextDraft,
    pub offset: usize,
    pub parent: TextDraft,
    pub child: TextDraft,
    pub updates: [Vec<TextDraft>; 3],
}
impl SplitWitness {
    pub fn with_second_split(
        mut self,
        target: SplitSequenceTarget,
        enter: &TextDraft,
        offset_utf16: usize,
        parent: &TextDraft,
        child: &TextDraft,
    ) -> Result<Self, Error> {
        if self.second.is_some() || !self.pre_updates.is_empty() {
            return Err(Error::Unsupported);
        }
        let live = match target {
            SplitSequenceTarget::Original => self.parent_draft(),
            SplitSequenceTarget::FirstChild => self.child_draft(),
        };
        if live != enter {
            return Err(Error::Changed);
        }
        SplitWitness::new(enter, offset_utf16, parent, child)?;
        self.second = Some(Second {
            target,
            enter: enter.clone(),
            offset: offset_utf16,
            parent: parent.clone(),
            child: child.clone(),
            updates: std::array::from_fn(|_| Vec::new()),
        });
        Ok(self)
    }
    pub fn with_final_updates(
        mut self,
        original: &[TextDraft],
        first_child: &[TextDraft],
        second_child: &[TextDraft],
    ) -> Result<Self, Error> {
        let groups = [original, first_child, second_child];
        let starts = self.sequence_starts().ok_or(Error::Unsupported)?;
        for (start, reports) in starts.into_iter().zip(groups) {
            super::super::super::end::typing::validate_updates_from(start, reports)?;
        }
        let all = self
            .parent_updates
            .iter()
            .chain(&self.updates)
            .chain(groups.into_iter().flatten())
            .cloned()
            .collect::<Vec<_>>();
        pre_enter::budget(&[], &all, &[])?;
        self.second.as_mut().ok_or(Error::Unsupported)?.updates = groups.map(<[TextDraft]>::to_vec);
        Ok(self)
    }
    fn sequence_starts(&self) -> Option<[&TextDraft; 3]> {
        let second = self.second.as_ref()?;
        let parent = self.parent_updates.last().unwrap_or(&self.parent);
        let child = self.updates.last().unwrap_or(&self.child);
        Some(match second.target {
            SplitSequenceTarget::Original => [&second.parent, child, &second.child],
            SplitSequenceTarget::FirstChild => [parent, &second.parent, &second.child],
        })
    }
    pub(in crate::canvas::edit::structure::split) fn sequence_final(
        &self,
        index: usize,
    ) -> Option<&TextDraft> {
        let second = self.second.as_ref()?;
        second.updates[index]
            .last()
            .or_else(|| self.sequence_starts().map(|s| s[index]))
    }
    pub fn offset_utf16(&self) -> usize {
        self.offset
    }
    pub fn member_drafts(&self) -> Vec<&TextDraft> {
        if self.second.is_some() {
            (0..3)
                .map(|i| self.sequence_final(i).expect("bounded member"))
                .collect()
        } else {
            vec![self.parent_draft(), self.child_draft()]
        }
    }
    pub fn visual_order(&self) -> Vec<usize> {
        match self.second_split_target() {
            Some(SplitSequenceTarget::Original) => vec![0, 2, 1],
            Some(SplitSequenceTarget::FirstChild) => vec![0, 1, 2],
            None => vec![0, 1],
        }
    }
    pub fn second_split_target(&self) -> Option<SplitSequenceTarget> {
        self.second.as_ref().map(|s| s.target)
    }
    pub fn second_enter_draft(&self) -> Option<&TextDraft> {
        self.second.as_ref().map(|s| &s.enter)
    }
    pub fn second_offset_utf16(&self) -> Option<usize> {
        self.second.as_ref().map(|s| s.offset)
    }
    pub fn second_initial_parent_draft(&self) -> Option<&TextDraft> {
        self.second.as_ref().map(|s| &s.parent)
    }
    pub fn second_initial_child_draft(&self) -> Option<&TextDraft> {
        self.second.as_ref().map(|s| &s.child)
    }
    pub fn final_updates(&self) -> Option<&[Vec<TextDraft>; 3]> {
        self.second.as_ref().map(|s| &s.updates)
    }
    /// Private proof view: the second truncation is an actual report, not an ACK.
    pub(in crate::canvas::edit::structure::split) fn flattened(&self) -> Result<Self, Error> {
        let second = self.second.as_ref().ok_or(Error::Unsupported)?;
        let mut first = self.clone();
        first.second = None;
        match second.target {
            SplitSequenceTarget::Original => first.parent_updates.push(second.parent.clone()),
            SplitSequenceTarget::FirstChild => first.updates.push(second.parent.clone()),
        }
        first
            .parent_updates
            .extend(second.updates[0].iter().cloned());
        first.updates.extend(second.updates[1].iter().cloned());
        first.checked()
    }
}
