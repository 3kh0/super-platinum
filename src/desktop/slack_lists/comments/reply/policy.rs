use super::*;
use super_platinum_core::state::posting_metadata::authority::{Authority, RawChannel};
pub(super) enum Witness {
    Live(Authority, RawChannel),
    Fixture(Value),
}
impl Witness {
    pub fn current(&self, shell: &ShellState, binding: &draft::Binding) -> bool {
        if !binding.current(shell) {
            return false;
        }
        match self {
            Self::Live(authority, channel) => {
                channel.current(&shell.core) && authority.metadata(&shell.core).is_ok()
            }
            Self::Fixture(raw) => {
                fixture::compose::enabled(shell)
                    && raw["id"].as_str()
                        == Some(binding_target(shell, binding).map_or("", |t| t.channel_id()))
            }
        }
    }
}
pub(super) async fn acquire(
    mut state: Signal<ShellState>,
    binding: &draft::Binding,
    serial: u64,
    target: &Target,
) -> Result<(ReplyPolicy, Witness), Error> {
    let fixture_mode = crate::fixture::is_fixture();
    let observed = if fixture_mode {
        None
    } else {
        let mut shell = state.write();
        if !binding.owner.reading(&shell, serial) {
            return Err(unavailable());
        }
        let authority = Authority::capture(&shell.core, &binding.owner.scope.team)
            .map_err(|_| unavailable())?;
        let read = authority
            .begin_channel_read(&mut shell.core, target.channel_id())
            .map_err(|_| unavailable())?;
        Some((authority, read))
    };
    let (client, session) = inputs(&state.peek(), binding)?;
    let request = api::conversations_generic_info(
        &client,
        &session,
        &BTreeMap::from([(target.channel_id().to_owned(), 0)]),
    );
    let raw = network::request(state, &binding.owner, serial, request, false).await?;
    if raw["ok"] != true {
        return Err(unavailable());
    }
    let rows = raw["channels"].as_array().ok_or_else(unavailable)?;
    let mut matching = rows
        .iter()
        .filter(|v| v["id"].as_str() == Some(target.channel_id()));
    let channel = matching.next().ok_or_else(unavailable)?.clone();
    if matching.next().is_some() {
        return Err(unavailable());
    }
    if !binding.owner.reading(&state.peek(), serial) {
        return Err(unavailable());
    }
    if fixture_mode {
        let policy = fixture::compose::policy(state, channel.clone())?;
        return Ok((policy, Witness::Fixture(channel)));
    }
    let (authority, read) = observed.unwrap();
    let mut shell = state.write();
    if !binding.owner.reading(&shell, serial) {
        return Err(unavailable());
    }
    let channel = read
        .apply(&mut shell.core, channel)
        .map_err(|_| unavailable())?;
    let readonly = authority
        .metadata(&shell.core)
        .map_err(|_| unavailable())?
        .thread_read_only(authority.configuration(), channel.value())
        .map_err(|_| unavailable())?;
    let policy = ReplyPolicy::observed(
        channel.value().clone(),
        target.record_raw()["archived"] == true,
        !readonly,
    );
    Ok((policy, Witness::Live(authority, channel)))
}
