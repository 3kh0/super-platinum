//! Rendering carries immutable authority; it never recreates it from fields.
use super::*;

#[derive(Clone, Debug)]
pub(crate) enum Inventory {
    Managed(durable::Review),
    Local(durable::LocalInventory),
}
impl Inventory {
    pub fn scope(&self) -> &Scope {
        match self {
            Self::Managed(review) => review.scope(),
            Self::Local(local) => local.scope(),
        }
    }
    pub fn local(&self) -> &LocalDraft {
        match self {
            Self::Managed(review) => review.local(),
            Self::Local(local) => local.local(),
        }
    }
    pub fn row(&self) -> Option<&durable::Row> {
        match self {
            Self::Managed(review) => Some(review.row()),
            Self::Local(local) => local.row(),
        }
    }
    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Managed(left), Self::Managed(right)) => left.same(right),
            (Self::Local(left), Self::Local(right)) => left.same(right),
            _ => false,
        }
    }
    pub fn managed(&self) -> Result<&durable::Review, durable::Error> {
        match self {
            Self::Managed(review) => Ok(review),
            Self::Local(_) => Err(durable::Error::ReviewRequired),
        }
    }
}

/// Uncommitted text is presentation/recovery information, never a replacement
/// for the transaction-owned pending input in Review's immutable inventory.
#[derive(Clone, Debug)]
pub(crate) struct View {
    inventory: Inventory,
    uncommitted: Option<LocalDraft>,
}
impl View {
    pub(super) fn new(inventory: Inventory, uncommitted: Option<LocalDraft>) -> Self {
        Self {
            inventory,
            uncommitted,
        }
    }
    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }
    pub fn scope(&self) -> &Scope {
        self.inventory.scope()
    }
    pub fn local(&self) -> &LocalDraft {
        self.inventory.local()
    }
    pub fn uncommitted(&self) -> Option<&LocalDraft> {
        self.uncommitted.as_ref()
    }
    pub fn same(&self, other: &Self) -> bool {
        self.inventory.same(&other.inventory)
            && match (&self.uncommitted, &other.uncommitted) {
                (None, None) => true,
                (Some(left), Some(right)) => same_local(left, right),
                _ => false,
            }
    }
}

/// Full account inventory is extracted without stripping local-only rows,
/// origins or alternate versions. Duplicate managed/local entries are checked,
/// never resolved by whichever map happens to be iterated last.
pub(super) fn views(
    account: &str,
    loaded: &durable::Loaded,
) -> Result<BTreeMap<DraftKey, View>, durable::Error> {
    let mut entries = BTreeMap::new();
    for (key, review) in loaded.managed() {
        if review.scope().account_id != account || &review.scope().key != key {
            return Err(durable::Error::Stale);
        }
        if !match (review.uncommitted(), loaded.uncommitted().get(key)) {
            (None, None) => true,
            (Some(left), Some(right)) => same_local(left, right),
            _ => false,
        } {
            return Err(durable::Error::Stale);
        }
        entries.insert(
            key.clone(),
            View::new(
                Inventory::Managed(review.clone()),
                loaded.uncommitted().get(key).cloned(),
            ),
        );
    }
    for (key, local) in loaded.locals() {
        if local.scope().account_id != account || &local.scope().key != key {
            return Err(durable::Error::Stale);
        }
        if local.row().is_some()
            || !match (local.uncommitted(), loaded.uncommitted().get(key)) {
                (None, None) => true,
                (Some(left), Some(right)) => same_local(left, right),
                _ => false,
            }
        {
            return Err(durable::Error::Stale);
        }
        if entries.contains_key(key) {
            // No cross-type full-inventory equality is exposed. Do not accept
            // a duplicate by comparing only row/local and discarding origins.
            return Err(durable::Error::Stale);
        } else {
            entries.insert(
                key.clone(),
                View::new(
                    Inventory::Local(local.clone()),
                    loaded.uncommitted().get(key).cloned(),
                ),
            );
        }
    }
    // A dangling pending write cannot become an invented local inventory.
    if loaded
        .uncommitted()
        .keys()
        .any(|key| !entries.contains_key(key))
    {
        return Err(durable::Error::ReviewRequired);
    }
    Ok(entries)
}
