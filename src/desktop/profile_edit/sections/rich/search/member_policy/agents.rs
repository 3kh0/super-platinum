//! Existing renderer-neutral Agentforce decoders, acquired under the Profile
//! query and original source witness. This does not borrow an Activity owner.
use super::*;
use super_platinum_core::block_actions::app_eligibility::agents as facts;

pub(super) async fn capture(
    state: Signal<ShellState>,
    search: &Search<impl Host>,
    witness: &Witness,
    users: &[&User],
    observed_profiles: &BTreeMap<String, Value>,
) -> Result<Option<facts::Access>, Error> {
    if users.is_empty() {
        return Ok(None);
    }
    let ctx = &search.context;
    let boot = request(
        state,
        search,
        witness,
        facts::boot_request(&ctx.client, &ctx.session),
    )
    .await?;
    let (me, enterprise) = facts::boot(&ctx.session, &boot)?;
    let prefs = request(
        state,
        search,
        witness,
        facts::preferences_request(&ctx.client, &ctx.session),
    )
    .await?;
    let org = if enterprise {
        Some(
            request(
                state,
                search,
                witness,
                facts::enterprise_request(&ctx.client, &ctx.session),
            )
            .await?,
        )
    } else {
        None
    };
    let settings = facts::settings(&prefs, org.as_ref())?;
    let mut profiles = BTreeMap::new();
    let mut accessible = BTreeSet::new();
    if facts::list_mode(witness.preferences.assignments(), &me)? == facts::ListMode::Fetch {
        let mut cursor = None;
        let mut seen = BTreeSet::new();
        let mut complete = false;
        for _ in 0..50 {
            let raw = request(
                state,
                search,
                witness,
                facts::list_request(&ctx.client, &ctx.session, cursor.as_deref())?,
            )
            .await?;
            let page = facts::Page::decode(&raw)?;
            accessible.extend(page.accessible);
            for profile in page.profiles {
                profiles.insert(profile.app().to_owned(), profile);
            }
            match page.cursor {
                None => {
                    complete = true;
                    break;
                }
                Some(next) => {
                    if !seen.insert(next.clone()) {
                        return Err(unavailable());
                    }
                    cursor = Some(next);
                }
            }
        }
        // Local bound/cycles are failures, never a partial accessibility grant.
        if !complete {
            return Err(unavailable());
        }
    }
    for (app, raw) in observed_profiles {
        if !profiles.contains_key(app) {
            profiles.insert(app.clone(), facts::Profile::decode(app, raw)?);
        }
    }
    if !witness.current(&state.peek(), search) {
        return Err(unavailable());
    }
    Ok(Some(facts::Access::new(settings, accessible, profiles)))
}
