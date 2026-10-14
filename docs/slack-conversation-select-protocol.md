# Conversation selector protocol bounds

## Multi-conversation source provenance

The bounded `multi_conversations_select` implementation uses the public Slack
[shared boot bundle](https://a.slack-edge.com/bv1-13/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js).
Its SHA-256 is
`1681ecc18bbfead52efb391b1ccfb3e15f1b4a1093a8475d847b221ce196c537`.
The local audit copy is under `tmp/sb/js/` with the same filename. Offsets below
are decoded JavaScript string character offsets, not compressed byte offsets.

| Module / offset | Observed contract |
| --- | --- |
| `QjSI` 13262315 | Multi-conversation component, ordinary staging/Apply/confirmation, and input-inline local selection. Default requiredness is false. |
| `QjSI` 13263779 | Constructor uses saved selection, then initial selection, then `[]`. Empty arrays are truthy and retain an explicit clear. |
| `QjSI` 13263942 | Mounted display resets only for a changed, truthy initial array. A changed `[]` resets it; removing or nulling the initial array does not. |
| `QjSI` 13265363 | Action contains ordered selected IDs and a supplied initial array, including `[]`. Maximum violations block ordinary Apply but are forwarded locally for input validation. |
| `bgmB` 1727423 | Wrapper passes saved conversations and chooses inline selection for input blocks. |
| `iGIl` 1742940 | Saved prop/state resolution coalesces null or missing `selected_conversations` to absence. |
| `cxJV` 1750345 | Initial block state includes a supplied `initialConversations` array, including `[]`. |
| `AM6q` 1754817 | Full-state projection uses `selected_conversations`; it adds no response URL flag for this control. |
| `G9Gm` 1605531 | Props undergo generic camel-case conversion; this is not an origin-default resolver. |
| `n9UX` 13254326 | Shared multi wrapper supplies required/maximum validation presentation. |
| `fRHe` near 4200149 | Generic search defaults distinguish an explicit channel search object from the component's no-filter defaults. |

The [official multi-select reference](https://docs.slack.dev/reference/block-kit/block-elements/multi-select-menu-element/#conversations-list)
documents initial arrays, positive maximum, confirmation, placeholder and filter.
The [official filter reference](https://docs.slack.dev/reference/block-kit/composition-objects/conversation-filter-object/)
documents public/private/IM/group-DM categories and external/bot exclusions.
Captured empty filter/include behavior is broader than the public authoring
schema; the implementation retains that observed renderer behavior.

## Implemented core contract

- `MultiConversation` and `Conversations(Vec<String>)` remain distinct from
  single conversation, channel-array, and user-array controls. Arrays retain
  order and use the exact `selected_conversations` wire property.
- Initial arrays may be absent, null, or empty. Typed saved state with a missing
  or null selected array falls back to the initial array. Only `[]` is an
  explicit saved clear. Malformed array shapes fail parsing; malformed saved
  identities remain visible but block submission until corrected.
- IDs retain their original identity, including member IDs. Parsing and
  selection never open a DM, join a channel, or send a request. Directory
  eligibility and fresh canonical metadata proofs belong to the desktop layer.
- Arrays are bounded to 1,000 unique syntactically valid IDs. A supplied maximum
  must be a positive integer. Over-limit existing arrays remain editable;
  ordinary commits and modal submission enforce the current maximum.
- Ordinary messages default to optional. Explicit message requiredness is
  respected on commit. Modal input-block `optional` owns requiredness instead
  of the element's message-specific required flag.
- Ordinary changes stage locally. Committing initializes full sibling state;
  untouched absent initial arrays stay absent, supplied empty initial arrays
  stay present, and clears serialize `[]`. Later initial-display resets do not
  silently replace already committed sibling wire state.
- Ordinary captures bind account/workspace/message identity, canonical element
  revision and metadata. Confirmation uses the existing action lifecycle.
  Request preparation requires the committed state array to match the captured
  action and uses the existing non-replayable block-action request path.
- Modal support is limited to ordinary submit-only input blocks. Dirty values
  survive compatible reconciliation, pristine values follow canonical state,
  and incompatible control types cannot consume another control's saved value.
- Neither ordinary actions nor modal full state emit `response_url_enabled`
  for this multi selector, even if the raw element contains that field.

## Filter and default boundaries

`QjSI`'s search-options branch near 13266100 independently establishes the same
filter rules used by the existing generic conversation directory. No filter or
an empty filter starts with nonarchived public/private channels, group DMs and
members including self. A nonempty filter with explicit nonempty `include`
selects categories and excludes archived channels. With a nonempty filter but
no nonempty `include`, the generic channel defaults remain in effect: public
and private channels, archived channels allowed, group DMs excluded. This
distinction must not be collapsed by discarding the raw filter.

Bot exclusion applies to member choices; external/shared exclusions and the
captured internal `only_from_team_id` constraint remain directory policy.
Malformed/unknown filter properties fail closed. This multi selector does not
inherit the single `channels_select` Enterprise restriction. Unsupported
team-restriction/Enterprise policy still needs an explicit desktop refusal.

`default_to_current_conversation: true` remains explicitly unsupported in both
ordinary and modal contexts. Absent or false flags are supported; malformed
flags are rejected. The official multi-select rule gives the current
conversation default precedence over the initial array, unlike the single
selector's rule. The [Slack SDK type source](https://raw.githubusercontent.com/slackapi/node-slack-sdk/main/packages/types/src/block-kit/block-elements.ts)
also records this distinction.

The recovered `QjSI` component receives an already resolved initial array.
Neither spelling of the default flag was found in the four recovered public
shared/async-shared/vendors/async-vendors bundles. This is bounded negative
evidence, not proof about unrecovered Slack code. Origin availability, filtered
origin handling and resulting initial action metadata must be established
before enabling the default; mutable active-channel state is not an origin.

Modal confirmation, dispatch-on-change, visibility restrictions, non-input
controls, specialized containers and unproven service/enterprise overrides
remain unsupported. This slice does not claim full selector or modal parity.

## Focused regression coverage

Core tests cover initial/saved null versus empty arrays, ordered channel/member
IDs, malformed state, requiredness, reduced maximum correction, raw filters,
deferred origin default, full sibling state, dirty/pristine reconciliation,
cross-kind ownership, mounted initial-array removal, confirmation capture,
stale owner/element rejection and exact non-replayable action preparation.
Tests prepare synthetic requests without a Slack session or live transport.
Build and native acceptance results are recorded separately in the worklog.
