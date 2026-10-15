//! Complete immutable local inventory, including the lineage of retained input.
use super::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InventoryRecord {
    version: u32,
    scope: Scope,
    local: LocalDraft,
    #[serde(deserialize_with = "required_option")]
    blocked: Option<(String, LocalDraft)>,
    alternates: Vec<(String, u64, LocalDraft)>,
    #[serde(deserialize_with = "required_option")]
    row: Option<(u64, String)>,
    origins: Vec<(String, WriterOrigin)>,
    clock: u64,
    #[serde(deserialize_with = "required_option")]
    generation: Option<String>,
}
impl InventoryRecord {
    pub(super) fn capture(view: &LocalInventory) -> Result<Self, AppError> {
        if view.uncommitted.is_some() {
            return Err(invalid(
                "cannot persist queued input as committed inventory",
            ));
        }
        Ok(Self {
            version: 1,
            scope: view.scope().clone(),
            local: view.local().clone(),
            blocked: view.blocked().map(|b| (b.token.clone(), b.draft.clone())),
            alternates: view
                .alternates()
                .iter()
                .map(|a| (a.writer_id.clone(), a.revision, a.draft.clone()))
                .collect(),
            row: view
                .row
                .as_ref()
                .map(|r| Ok::<_, AppError>((r.revision(), wire::encode(&r.inner)?)))
                .transpose()?,
            origins: view
                .origins
                .iter()
                .map(|o| (o.writer.clone(), o.origin.clone()))
                .collect(),
            clock: view.clock,
            generation: view.generation.clone(),
        })
    }
    pub(super) fn restore(&self) -> Result<LocalInventory, AppError> {
        if self.version != 1
            || !valid_uuid(&self.scope.account_id)
            || self.scope.key.team.is_empty()
            || self.scope.key.channel.is_empty()
            || self.scope.key.thread.as_ref().is_some_and(String::is_empty)
            || self.generation.as_deref().is_some_and(|g| !valid_uuid(g))
        {
            return Err(invalid("invalid retained inventory"));
        }
        sql_number(self.clock)?;
        let row = self
            .row
            .as_ref()
            .map(|(revision, raw)| {
                wire::decode(&self.scope, sql_number(*revision)?, 7, raw).map(Row::new)
            })
            .transpose()?;
        let mut seen = std::collections::BTreeSet::new();
        let alternates = self
            .alternates
            .iter()
            .map(|(writer_id, revision, draft)| {
                if !valid_uuid(writer_id) || *revision == 0 || !seen.insert(writer_id.clone()) {
                    return Err(invalid("invalid retained alternate"));
                }
                sql_number(*revision)?;
                Ok(LocalAlternate {
                    writer_id: writer_id.clone(),
                    revision: *revision,
                    draft: draft.clone(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        if self.origins.len() != alternates.len()
            || self
                .origins
                .iter()
                .zip(&alternates)
                .any(|((id, origin), a)| id != &a.writer_id || !writers::valid_origin(origin))
        {
            return Err(invalid("retained origins do not match inventory"));
        }
        if self
            .blocked
            .as_ref()
            .is_some_and(|(token, _)| !valid_uuid(token))
        {
            return Err(invalid("invalid retained blocked token"));
        }
        Ok(LocalInventory {
            review: LocalReview {
                scope: self.scope.clone(),
                local: self.local.clone(),
                stored: row.as_ref().map(|r| r.inner.stored.clone()),
                blocked: self.blocked.as_ref().map(|(token, draft)| BlockedLocal {
                    token: token.clone(),
                    draft: draft.clone(),
                }),
                alternates,
            },
            row,
            origins: self
                .origins
                .iter()
                .map(|(writer, origin)| VersionOrigin {
                    writer: writer.clone(),
                    origin: origin.clone(),
                })
                .collect(),
            clock: self.clock,
            generation: self.generation.clone(),
            uncommitted: None,
        })
    }
}

pub(super) fn local(conn: &Connection, scope: &Scope) -> Result<LocalInventory, AppError> {
    adapter_schema::require(conn)?;
    owner(conn, &scope.account_id)?;
    let key = serde_json::to_string(&scope.key)?;
    let active: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM composer_draft_sync WHERE destination=?1)",
        [&key],
        |r| r.get(0),
    )?;
    let row = active
        .then(|| storage::read_row(conn, scope).map(Row::new))
        .transpose()?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT content FROM composer_drafts WHERE destination=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()?;
    let blocked: Option<(String, String)> = conn
        .query_row(
            "SELECT token,content FROM composer_draft_blocked WHERE destination=?1",
            [&key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let alternates = read_alternates(conn)?
        .remove(&scope.key)
        .unwrap_or_default();
    let origins = writers::origins(conn, scope, &alternates)?;
    let (clock, generation, _, _) = clock(conn, scope)?;
    Ok(LocalInventory {
        review: LocalReview {
            scope: scope.clone(),
            local: raw
                .map(|s| wire::strict(&s))
                .transpose()?
                .unwrap_or_default(),
            stored: row.as_ref().map(|r| r.inner.stored.clone()),
            blocked: blocked
                .map(|(token, raw)| {
                    Ok::<_, AppError>(BlockedLocal {
                        token,
                        draft: wire::strict(&raw)?,
                    })
                })
                .transpose()?,
            alternates,
        },
        row,
        origins,
        clock,
        generation,
        uncommitted: None,
    })
}
pub(super) fn managed(view: LocalInventory) -> Result<Review, Error> {
    let row = view.row.ok_or(Error::Stale)?;
    Ok(Review {
        inner: ProtectedReview {
            row: row.inner.clone(),
            review: view.review,
        },
        row,
        clock: view.clock,
        generation: view.generation,
        origins: view.origins,
        uncommitted: view.uncommitted,
    })
}
pub(super) fn review(conn: &Connection, scope: &Scope) -> Result<Review, Error> {
    managed(local(conn, scope).map_err(|_| Error::Storage)?)
}
pub(super) fn exact(conn: &Connection, expected: &Review) -> Result<Review, Error> {
    if expected.uncommitted.is_some() {
        return Err(Error::Pending);
    }
    let current = review(conn, expected.scope())?;
    if !current.same(expected) {
        return Err(Error::Stale);
    }
    Ok(current)
}
pub(super) fn clock(
    conn: &Connection,
    scope: &Scope,
) -> Result<(u64, Option<String>, Option<String>, u64), AppError> {
    let raw: Option<(i64, Option<String>, Option<String>, i64)> = conn.query_row(
        "SELECT revision,generation,writer_id,serial FROM composer_draft_local_clock WHERE destination=?1",
        [serde_json::to_string(&scope.key)?], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
    ).optional()?;
    let Some((clock, generation, writer, serial)) = raw else {
        return Ok((0, None, None, 0));
    };
    if clock <= 0
        || serial < 0
        || generation.is_some() != writer.is_some()
        || generation.as_deref().is_some_and(|s| !valid_uuid(s))
        || writer.as_deref().is_some_and(|s| !valid_uuid(s))
        || generation.is_none() && serial != 0
    {
        return Err(invalid("invalid local continuation clock"));
    }
    Ok((clock as u64, generation, writer, serial as u64))
}

pub(super) fn local_from_review(review: &Review) -> LocalInventory {
    LocalInventory {
        review: review.inner.review.clone(),
        row: Some(review.row.clone()),
        origins: review.origins.clone(),
        clock: review.clock,
        generation: review.generation.clone(),
        uncommitted: review.uncommitted.clone(),
    }
}
