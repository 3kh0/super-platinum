use super::*;

pub(super) fn select(
    candidate: &MentionCandidate,
    rows: &[Value],
    context: &mut Context<'_>,
) -> Result<Option<(Value, String, usize)>> {
    if rows.is_empty() {
        return Ok(None);
    }
    // Ordinary in-thread rNCs has no broadcast collection. Never manufacture it.
    if rows
        .iter()
        .any(|row| row.get("is_broadcast_keyword").is_some_and(|v| v == true))
    {
        return Err(SeedUnavailable::InvalidObservation);
    }
    let mut names = Vec::new();
    for row in rows {
        let mut observed = context.observed.names.iter().filter(|v| v.raw == *row);
        let Some(name) = observed.next() else {
            return context.missing(MentionNeed::Names(row.clone()));
        };
        if observed.next().is_some() {
            return Err(SeedUnavailable::InvalidObservation);
        }
        names.push((name.primary.clone(), name.secondary.clone()));
    }
    let mut normalized = Vec::new();
    for (primary, secondary) in &names {
        normalized.push((context.normalize(primary)?, context.normalize(secondary)?));
    }
    let mut order = Vec::new();
    // Retained source appends all primary names, then all secondary names.
    for value in normalized
        .iter()
        .map(|v| &v.0)
        .chain(normalized.iter().map(|v| &v.1))
    {
        if !value.is_empty() && !order.contains(value) {
            order.push(value.clone());
        }
    }
    order.sort_by_key(|name| std::cmp::Reverse(name.encode_utf16().count()));
    for name in order {
        let length = name.encode_utf16().count();
        if candidate.label.encode_utf16().count() > length {
            break;
        }
        let units: Vec<_> = candidate.text.encode_utf16().collect();
        let end = candidate
            .local_index_utf16
            .checked_add(length)
            .ok_or(SeedUnavailable::Limit)?
            .min(units.len());
        let segment = String::from_utf16(
            units
                .get(candidate.local_index_utf16..end)
                .ok_or(SeedUnavailable::InvalidObservation)?,
        )
        .map_err(|_| SeedUnavailable::InvalidUnicode)?;
        let full = context.normalize(&name)? == context.normalize(&segment)?;
        if full || rows.len() <= 1 {
            let matching: Vec<_> = normalized
                .iter()
                .enumerate()
                .filter(|(_, v)| v.0 == name || v.1 == name)
                .map(|(i, _)| i)
                .collect();
            let good = good_enough(&name, &candidate.label, context)?;
            let replace_length = if rows.len() != 1 || full {
                length
            } else {
                candidate.label.encode_utf16().count()
            };
            if good && matching.len() == 1 && (full || !flag(&rows[matching[0]], "is_usergroup")?) {
                let i = matching[0];
                return Ok(Some((rows[i].clone(), names[i].0.clone(), replace_length)));
            }
            if good && matching.len() > 1 {
                let Some(history) = context.observed.history.as_ref() else {
                    return context.missing(MentionNeed::History);
                };
                let Some(query) = context.observed.history_queries.get(&candidate.label) else {
                    return context.missing(MentionNeed::HistoryQuery(candidate.label.clone()));
                };
                let Some(now) = context.observed.now_ms else {
                    return context.missing(MentionNeed::Clock);
                };
                if !now.is_finite() {
                    return Err(SeedUnavailable::InvalidObservation);
                }
                let ids: Vec<_> = matching
                    .iter()
                    .map(|i| id(&rows[*i]))
                    .collect::<Result<_>>()?;
                let scores = history.score_normalized(query, &ids, now);
                let mut ranked: Vec<_> = matching.iter().copied().zip(scores).collect();
                if ranked.iter().any(|(_, v)| !v.raw.is_finite()) {
                    return Err(SeedUnavailable::InvalidObservation);
                }
                ranked.sort_by(|a, b| b.1.raw.total_cmp(&a.1.raw));
                if ranked[0].1.raw - ranked[1].1.raw >= 30.0 {
                    let i = ranked[0].0;
                    return Ok(Some((rows[i].clone(), names[i].0.clone(), replace_length)));
                }
            }
            // RZMQ returns UNVERIFIED immediately on a full unresolved match;
            // tEEs discards that candidate rather than trying a shorter name.
            if full {
                return Ok(None);
            }
        }
    }
    Ok(None)
}

fn good_enough(name: &str, label: &str, context: &mut Context<'_>) -> Result<bool> {
    let strip = |s: &str| {
        s.strip_prefix('@')
            .or_else(|| s.strip_prefix('\u{ff20}'))
            .unwrap_or(s)
            .to_owned()
    };
    let name = context.normalize(&strip(name))?;
    let label = context.normalize(&strip(label))?;
    let first = label.split([' ', '-', '.', '_']).next().unwrap_or("");
    Ok(name
        .split([' ', '.', '_'])
        .any(|word| word.split('-').next() == Some(first)))
}
