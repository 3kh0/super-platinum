# Canvas comment reader protocol

This records a bounded read-only implementation, not complete Canvas comments
parity. No real workspace request or comment mutation was performed to prepare
this candidate. Public shipped client code supplies the private document-stream
contract; synthetic fixtures test our implementation of that contract.

## Sources

- [Slack: Use a canvas](https://slack.com/help/articles/203950418-Use-a-canvas-in-Slack)
  documents Show all threads and Most recent / Unread / Resolved filters.
- [Slack: Canvas and List access](https://slack.com/help/articles/15678967614611-Manage-access-permissions-for-canvases-and-lists)
  distinguishes comment access from permission to edit the Canvas body.
- [Slack: conversations.replies](https://docs.slack.dev/reference/methods/conversations.replies/)
  documents canonical parent timestamps, cursor pagination and default latest=now.
- Public comment-stream asset:
  `https://a.slack-edge.com/bv1-13-br/gantry-v2-async-client-canvas-comment-stream-view.d473053ee50515a3.min.js`
  SHA256 `e3176d2784cbf43aced9cb194ae912583cbff6aa92c5089b8164da878f454643`.
- Public thread-router asset:
  `https://a.slack-edge.com/bv1-13-br/gantry-v2-async-client-canvas-thread-view-router.639f2c20d9a85f3d.min.js`
  SHA256 `2ff52dd8c75f8f905a4c7bca4ce217e14d2943fb4a3805d1a47aa1597ce08cc3`.
- Retained shared bundle `gantry-v2-shared.0b81595876785d7a.min.js`, SHA256
  `f085df09fb4f938caf425ea66fb0244c8f92a8012eea8eb298e4f629b377dbf7`.
- Retained boot bundle `gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js`,
  SHA256 `1681ecc18bbfead52efb391b1ccfb3e15f1b4a1093a8475d847b221ce196c537`.

The review package's `references/protocol-index.json` pins all retained files,
character offsets and short excerpts. Public chunk names/hashes were read from
the existing Slack renderer's inline script mapping with read-only CDP. Public
JavaScript assets were then downloaded without session credentials. No private
stores, network response bodies, message content or authentication data were read.

## Source-backed read contract

`Oo8B` at comment-stream character offset 2821 calls `rGEL.Ay` with the selected
file mapped through `IIdT.p`, `limit:20`, `documentStream:filter`, and optional
cursor (offset3121). `IIdT` at shared3766858 changes only the initial `F` to `C`.
It does not choose the currently viewed channel or a channel sharing the Canvas.

`rGEL` at shared1424347 builds `conversations.history` with `channel`, `limit`,
`ignore_replies=true`, `include_pin_count=true`, `inclusive=true`,
`no_user_profile=true`, `include_stories=true`,
`include_free_team_extra_messages=true`, `include_date_joined=false`,
`document_stream` and optional `cursor`. Its cold cache sends
`cached_latest_updates={}`. The candidate does the same. It does not emulate
workspace-dependent EKM/tombstone flags or warm message-version optimization.

Raw response `messages`, `has_more` and `response_metadata` become the client's
`msgs`, `hasMore` and `responseMetadata`. `Oo8B` offsets3393-3589 separates
`subtype=document_comment_root` roots and message replies with a distinct
`thread_ts`, then stores `response_metadata.next_cursor` and `has_more`.
The list remains in server order. The candidate attaches only replies matching
a returned root and uses the newest exact timestamp for the last-reply preview.
Unmatched previews are not presented as independent threads.

`Pn2I` at boot7627951 defines the exact filter values `latest_reply`, `archived`,
`unread`; the public stream asset at10923-11165 supplies the labels. Switching
filter begins a new read scope; an earlier result cannot append to it.

The shipped thread route carries both Quip `commentThreadId` and Slack `threadTs`.
The `MVSX` request-Quip-comments path (ending at boot8166818) can bootstrap a
Quip ID through `Qpc:` plus a controller-derived content snippet. This candidate
does **not** bootstrap a thread from an export section, guess a Quip ID, or call
that branch. It opens only a canonical Slack timestamp returned by the document
stream. The existing `conversations.replies` builder supplies `channel`, `ts`,
`inclusive=true`, `limit=28`, optional cursor, plus a cold
`cached_latest_updates={}` matching `rGEL`'s replies path. No explicit latest is
needed for this canonical path; the documented default is now.

## Local admission and bounded navigation

The parser requires a successful envelope, typed messages, bounded cursors and
payloads, exact optional channel identity, and a document root carrying
`document_comment.thread_id`. The first reply page must include that exact root;
later pages may omit it. A foreign parent, malformed root, duplicate timestamp,
or `has_more=true` without a usable next cursor fails the page atomically.
Errors are shown as errors, including channel-not-found; they are not recast as
an empty stream even though the web client's fetcher sometimes does so.

These are local safety bounds, not claimed Slack maxima: two MiB per decoded
response,1000 raw messages per response,200 presented rows per page,4096 bytes per
cursor. The requested limits remain20 roots/28 replies. Only the current page is
rendered. Next can traverse arbitrarily many distinct valid pages; Previous
rereads up to eight recent request cursors. Refresh comments returns to the first
page when an earlier cursor has been evicted. A constant-space cursor-cycle
detector and ten consecutive non-progress pages stop malformed continuation;
there is no ten-page or200-root lifetime cutoff. Retained recent message IDs are
bounded at200 and exist only for progress detection.

Reads capture account/epoch/team/viewer, credentials, transport identity, Canvas
file and visit, plus request serial/query. Fresh files.info verifies the same
Canvas before live comment dispatch. Closing, reopening, switching account,
changing credentials, deleting the Canvas or selecting another query retires
result adoption. Comment messages never enter the active channel/thread bags.

Retry-After fences survive reader close/reopen in a small process-local stable
owner registry. At capacity, an overflow fence conservatively retains the latest
deadline. Unrepresentable delays remain blocked. One owner-bound timer enables
an explicit retry; no timer issues a request. This is not a durable cross-process
rate-limit database. Transport retains its existing read retry semantics.

Focus handoff is bound to the actual Canvas dialog node, component lease, visit,
and initiating comment control. Disabling/removing an opener transfers focus to
the stable Back control; Back restores Show all threads. New focus/pointer/key
input cancels a queued handoff. There is no background or delayed unconditional
refocus.

## Explicit remaining work

Comment creation/reply/edit/delete, section anchors and Quip-only thread bootstrap,
resolve/reopen/all-resolve, unread marking, subscriptions, reaction UI, collaboration
and realtime reconciliation are not implemented here. The message bodies reuse
typed rich text and file nodes; this is not the full transcript action/reaction/
unfurl renderer. Refresh provides new read evidence; this slice does not claim
live comment updates or warm cached comment history. Raw tombstone variants that
do not satisfy the supported message shape fail closed. Real Canvas access and
native pixels remain root-owned verification gates, not proven by synthetic data.
