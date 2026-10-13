# Native startup responsiveness

The intermittent blank-window launch remains unresolved. An earlier signed live
launch showed a blank window and agent timeout until its exact app bundle was
activated. Later launches have been healthy. Neither the new probe nor a healthy
trace is a fix for that observation.

This document describes bounded diagnostics for distinguishing progress in the
native event loop, the WebView, and Dioxus application tasks. It does not change
window activation, background throttling, edit delivery, or retry policy.

## Enable the probe

Use an already-built signed binary and set these variables before launching:

```sh
SUPER_PLATINUM_RUNTIME_PROBE=1
SUPER_PLATINUM_STARTUP_TRACE=1
SUPER_PLATINUM_AGENT=1
SUPER_PLATINUM_AGENT_SOCK=/tmp/sp-runtime-owned.sock
```

The first variable must equal `1`; its value is cached once. The startup trace and
agent are separate optional instruments. Give each owned process a unique socket
and log. Do not remove another running process's socket or terminate unrelated
application instances.

For an offline direct-executable observation, for example:

```sh
SUPER_PLATINUM_FIXTURE=slack-list \
SUPER_PLATINUM_RUNTIME_PROBE=1 \
SUPER_PLATINUM_STARTUP_TRACE=1 \
SUPER_PLATINUM_AGENT=1 \
SUPER_PLATINUM_AGENT_SOCK=/tmp/sp-runtime-owned.sock \
"./target/debug/Super Platinum.app/Contents/MacOS/super-platinum" \
  >tmp/runtime-owned.log 2>&1 &
runtime_probe_pid=$!
```

An equivalent bundle launch uses `open -n` with each variable passed through
`--env` and logs captured through `--stdout` and `--stderr`. A bundle launch is
not proof that its window became focused: record the observed focus separately.
The normal fixture capture harness may activate windows, so it is not by itself
a controlled background-launch experiment. Omitting `SUPER_PLATINUM_FIXTURE`
uses the real session; do that only for a deliberately scoped live investigation.

## What is recorded

Lines begin with `super-platinum runtime-probe:` followed by a whitelisted JSON
object. Implementation: `src/desktop/performance/runtime_probe.rs`, its adjacent
JavaScript, and narrow configuration/tick wiring in `media.rs` and `runtime.rs`.

| Field or phase | Meaning |
| --- | --- |
| `configured`, outer `elapsed_ms` | Probe setup and milliseconds since that setup. |
| `window_hidden` / `window_visible` | Native window state immediately after creation, before the WebView is created. Hidden here is expected during normal startup. |
| `native_events`, `user_events`, `window_events` | Aggregate events observed by the native callback. User events include framework work; these are not edit or ACK counters. |
| `native_focused` | Last observed native focus, initially sampled at window creation. |
| `heartbeats`, `heartbeat_age_ms` | Progress of the existing 200 ms Dioxus runtime tick; null age means no heartbeat has run. |
| `js_samples` | Number of accepted structural JavaScript reports, capped at 16. |
| `js.elapsed_ms` | Milliseconds since the initial document's probe script started; a different origin from outer elapsed time and startup metrics. |
| `js.ready`, `visibility`, `focused` | Document readiness, document visibility, and `document.hasFocus()`. Visible does not mean foreground or unobscured. |
| `js.interpreter`, `headless` | Whether the pinned interpreter exists and its actual headless flag, or null before available. |
| `js.socket_state` | Edit WebSocket ready state: null if unavailable, 0 connecting, 1 open, 2 closing, 3 closed. An open socket does not prove edit or ACK delivery. |
| `js.queued_batches` | Interpreter queue length when exposed as an array. Zero is expected for the headless immediate-edit path and does not prove there are no in-flight edits. |
| `js.root_children` | Direct element children under `#main`; counts structure, not content or paint completion. |
| `js.raf_callbacks` | How many of two one-shot animation-frame callbacks ran. This is not a frame-rate measurement. |
| `js.script_errors`, `resource_errors`, `unhandled_rejections` | Numeric error-event counters since document startup, each capped at one million. ErrorEvent instances count as script errors; other captured error events count as resource errors. Rejection reasons and error payloads are never read. |

The native watchdog runs on a separate thread, with scheduled observations at
1, 2, 5, 10, and 20 seconds. It invokes no window APIs and does not depend on
Dioxus task polling. The native callback counts events without intercepting or
replacing framework dispatch. JavaScript starts in the initial document head,
before root component tasks, and reports through a dedicated diagnostic custom
protocol. Its readiness/visibility events and 1/2/5/10-second timers are capped;
listeners, timers, and outstanding one-shot frame callbacks are removed when its
20-second deadline is observed. A late final JS timer may only clean up without
emitting a final report. Native `phase:finished` supplies the terminal watchdog
observation while the process remains alive.

Collection is bounded, not a guarantee that a suspended OS thread will wake at an
exact deadline. Delayed callbacks cannot add accepted samples after the native
deadline. With the probe disabled there is no added thread, custom protocol,
document script, or native callback. The ordinary runtime timer is reused, and
the heartbeat handle is dropped after expiry.

Only fixed enums, booleans, bounded counts, and elapsed times are accepted. Unknown
fields, arbitrary strings, oversized bodies, and out-of-range values are rejected.
No DOM text, HTML, window titles, URLs, session data, edit bytes, or raw IPC payloads
are logged. The probe does not wrap the interpreter or consume its responses.
The error listeners do not cancel default behavior or propagation. They expire
with the same deadline and sample budget. `node scripts/check-runtime-probe.mjs`
checks the actual script against inaccessible private event fields, report caps,
and overdue cleanup, including an event arriving before a delayed cleanup timer.

## Interpretation and the pinned renderer

The repository pins Dioxus to
`57d6794ad60b949e5bd8aa282f6f8c3dc97a365e`. Its desktop
`webview.rs::poll_vdom` waits for outstanding edits to flush before polling VDOM
work. A stalled acknowledgement can therefore delay application tasks, including
the agent dispatcher; an agent timeout alone cannot distinguish that from other
startup delays. This is a source-derived possibility, not the demonstrated cause
of the earlier blank window.

On the observed macOS path, desktop startup initially creates a hidden window.
The pinned interpreter receives `headless=true`, which applies edit bytes and
marks them finished directly rather than waiting for `requestAnimationFrame`.
The healthy trace below confirms that actual flag. Do not assume a background
animation-frame pause is withholding edits on this path. Relevant pinned sources
are `packages/desktop/src/{app,webview,edits,launch}.rs` and
`packages/interpreter/src/ts/native.ts`.

| Observation | Narrow conclusion / next check |
| --- | --- |
| Native watchdog advances but no native events or JS reports advance | The process's watchdog thread is alive. Check a native stack sample and display/power state; this alone does not locate the event-loop stall. |
| Native events advance but initial JS never reports | Check document/WebView initialization and diagnostic protocol delivery. First establish that this same build/platform can deliver a healthy probe. |
| JS reports advance while Dioxus heartbeats stop | JS and the native reporting path are alive. Inspect app-task blocking and edit/ACK state; the probe cannot distinguish those causes by itself. |
| Socket open and nonempty root, but missing paint callbacks | Structure arrived; actual compositing/paint still needs a window capture. The headless edit path need not wait for those callbacks. |
| Heartbeats and fresh agent pings advance | Those app tasks are responsive at the sampled times. This does not establish every interaction or prove continuous responsiveness between samples. |

The Dioxus [0.7 async documentation](https://dioxuslabs.com/learn/0.7/essentials/basics/async/)
states that its futures run on the main thread. Native observations are deliberately
outside that task queue. The pinned source and compiler remain authoritative for
the concrete APIs and edit gate.

## Completed observations: 2026-10-13

The integrated desktop suite passed **759 tests, 5 ignored**, and the signed build
passed before this experiment. The `slack-list` offline fixture was launched by
the signed bundle's executable with the probe enabled. Its native focus remained
false throughout the 20-second trace.

- First-paint instrumentation reported **1632.4 ms** from app startup.
- By the first 1-second JS timer, the document was complete and visible,
  `headless=true`, the edit socket was open, root child count was 3, and both
  one-shot animation-frame callbacks had run.
- At the native 20-second observation there had been **94 app heartbeats**, with
  the last **190 ms** earlier. Native focus was still false.
- Agent socket checks at approximately 0.25 and 1 second found no socket. All
  four checks from approximately 2 through 20.5 seconds succeeded, with round-trip
  times **0.170–1.036 ms**. The early absence is startup readiness, not a measured
  hung dispatcher.
- The operator inspected the captured window and confirmed the fixture was
  painted while backgrounded.

Evidence: `tmp/runtime-probe-direct-background.{log,json}` and
`tmp/agent-ui/runtime-probe-direct-background.png`.

A second launch used `open -n` on the same signed bundle and offline fixture:

- First-paint instrumentation reported **589.0 ms**. The first 1-second JS timer
  again reported a complete, visible document, `headless=true`, an open edit
  socket, three root children, and two completed frame callbacks.
- At 20 seconds, **98 app heartbeats** had run and the latest was **158 ms** old.
- The approximately 0.25- and 1-second socket checks found no socket; all four
  checks from approximately 2 through 20.5 seconds succeeded in **0.495–1.118 ms**.
- Both native and document focus remained false throughout. The artifact name
  `bundle-foreground` describes the requested experiment, not observed focus.
  This is a second unfocused run, not a foreground/background comparison.
- The agent screenshot path failed with `screencapture` exit 1 and
  `could not create image from window`. The operator then inspected a CUA
  `getScreenshot` capture showing the fully painted List fixture with inactive
  gray window controls. That is visual evidence from a different capture method;
  the failed agent capture is not counted as a successful PNG assertion.

Bundle evidence: `tmp/runtime-probe-bundle-foreground.{log,json}`. The owned
fixture process for that observation was PID 62707. The temporary experiment
driver is `tmp/run-runtime-probe.py`; these local artifacts and process IDs are
historical observations, not permanent fixtures or current process ownership.

These are two healthy instrumented offline launches. They do not reproduce the
earlier live failure, establish a performance trend, validate real-session boot,
or prove that direct and bundle launches differ. Their individual timings must
not be presented as a comparative speedup. The probe adds native wakeups,
custom-protocol requests, logging, and two frame callbacks, and may affect the
behavior being investigated. Repeat comparable runs with it off as well as on.

## Next controlled reproduction

Two subsequent instrumented live launches used the same signed binary and real
session, with unique sockets and bounded observations. Both were healthy:

| Requested launch | First paint / conversation paint | Agent observations | Native 20-second observation |
| --- | --- | --- | --- |
| `open -n` | 1117.4 / 1143.4 ms | Socket absent at 0.25/1 s; four pings from 2–20.5 s passed in 0.317–0.709 ms | 97 heartbeats; last 61 ms earlier |
| `open -n -g` | 1050.8 / 1136.9 ms | Socket absent at 0.25 s; five pings from 1–20.5 s passed in 0.228–28.454 ms | 97 heartbeats; last 56 ms earlier |

The first launch actually had native/document focus early, then lost it; the
background launch remained unfocused. Both reported a complete, visible document,
headless interpreter, open edit socket, three root children, and two frame
callbacks. Evidence: `tmp/runtime-probe-live-bundle-{foreground,background}.{log,json}`.
The background agent screenshot failed with `screencapture` exit 1. A subsequent
CUA screenshot was inspected and showed a painted private conversation, muted
system-message text, and inactive window controls; a fresh agent ping also passed.
These healthy counterexamples do not reproduce or fix the intermittent blank
launch, and their timings are not a comparative performance claim. They predate
the additional numeric error counters and contain no error-counter samples.

A final signed `open -n -g` launch with the counters enabled also stayed healthy.
Eight JavaScript samples were accepted. The last sample reported zero script
errors, two resource errors, zero unhandled rejections, an open edit socket,
three root children, and both frame callbacks. This proves the new schema reaches
the native receiver; the resource count does not identify which resources failed.
At 20 seconds there were 99 heartbeats, the last 154 ms earlier, and native focus
remained false. Five pings from about 1 through 20.5 seconds passed in
0.250–0.387 ms; the 0.25-second socket check preceded readiness. Startup trace
reported first paint 309.5 ms and conversation paint 668.3 ms for this individual
warm run, without establishing a trend. Evidence:
`tmp/runtime-probe-live-counters-bundle-background.{log,json}`. Agent screenshot
again failed with exit 1; an inspected CUA capture showed the painted private
conversation. The rebuilt client was left running. No blank launch was reproduced.

The preceding live verification launch through `open -n` had remained visually blank
for more than two minutes, and agent requests timed out. The native sample
(`tmp/link-boot-sample.txt`) showed the main thread waiting in the AppKit event
loop; it did not establish the cause. Restarting the same signed executable
directly with startup tracing restored a usable signed-in window: first paint
2742.7 ms, conversation paint 3265.6 ms, history paint 4287.2 ms
(`tmp/link-live-startup.log`). Native Files navigation and List editing then
worked. These unmatched live runs demonstrate recovery, not a fixed startup bug
or a measured advantage of either launch method.

A closer read of that failed-launch sample provides a narrower discriminator:
all 1,710 main-thread samples were waiting in AppKit and the Tokio workers were
parked. The edit-server listener was waiting in `EditWebsocket::accept_loop`
(sample lines 247–256). Demangling lines 292–293 with the system `c++filt`
identifies a separate `EditWebsocket::handle_connection::{closure#1}` worker,
blocked in receive for all 1,710 samples. An earlier reading mistook the Rust
symbol suffix for a separate UDP handler and incorrectly inferred an absent edit
connection; that inference is withdrawn. The pinned `desktop/src/edits.rs`
connection-worker closure sends an edit batch and then waits in `websocket.read()`
for its acknowledgement. The sampled stack is consistent with that wait. It does
not show whether the WebView received or applied the batch, why an acknowledgement
was missing, or whether the JavaScript connection remained healthy. The optimized
leaf symbol says `UdpSocket::recv`; it does not establish that this worker uses
UDP, which is absent from the pinned edit transport source.

The smallest next diagnostic requires no source change: repeat the scoped live
bundle launch with the existing runtime probe, startup trace, and a unique agent
socket/log enabled together. Preserve the bounded 20-second trace before any
activation rescue. Compare interpreter presence, edit socket state, root child
count, and app heartbeats with the native stack sample. An open socket and active
JavaScript with stalled app heartbeats would support investigating edit application
or acknowledgement; missing interpreter/socket progress would instead point to an
earlier initialization stage. The probe does not count received batches or sent
acknowledgements, so neither outcome alone establishes a cause or fix.

Use the same signed build, fixture or account/cache state, window geometry, and
power/display conditions. Record the launch mechanism and which app actually
owns foreground focus before and after launch. Change launch mechanism and focus
origin separately; do not treat a filename or requested activation as evidence.

Take fresh, bounded agent pings and structural state at approximately 1, 2, 5, 10,
and 20 seconds, keeping absence, timeout, and successful latency separate. Capture
the native window independently if the agent stalls. Before any activation rescue,
collect a bounded native stack sample and retain the probe log. If activation is
then used, record its exact time and capture before/after state. Stop only the
experiment's owned process. Keep a reproduced failure distinct from healthy
counterexamples and from any subsequently verified fix.


## Controlled follow-up, 2026-10-13

The signed canvas late-event build was used for two sequential real-session
`open -n -g` launches. Only the prior task-owned process was stopped between
launches; no activation rescue was used during either twenty-second observation.

| Runtime probe | First paint / conversation / history (ms) | Agent observations |
| --- | --- | --- |
| Off (`a1`) | 1672.3 / 2259.9 / 4771.7 | Socket absent at .25/1s; four pings from 2–20.5s passed, 0.246–4.288ms |
| On (`a2`) | 1039.1 / 1556.7 / 3561.8 | Socket absent at .25s; five pings from 1–20.5s passed, 0.202–0.724ms |

The instrumented run had 96 heartbeats at twenty seconds, newest 112ms old,
with native focus false. Its last accepted JavaScript sample reported complete
visible document, headless interpreter, open socket, three root children, zero
queued batches, zero script errors/rejections and two resource errors. Both
windows were visibly painted in CUA screenshots inspected after observation.
Initial accessibility snapshots exposed only native window chrome despite those
painted contents; sparse accessibility alone is not a blank-window diagnosis.
The probe-off process used 286784KiB RSS at the later point sample; this is neither
a memory baseline nor a regression comparison.

Evidence: `tmp/runtime-controlled-{a1,a2}-bundle-background.{log,json}`. These
healthy sequential runs do not reproduce the old failure and do not establish
that instrumentation improves speed. Warm-cache state, OS scheduling and other
work were not controlled enough for a comparative performance claim.


## Passive edit-socket coverage

The opt-in probe now attaches passive listeners to the pinned interpreter's
current edit socket at the existing report cadence. Numeric fields record the
attachment generation/time, message/open/close/error counts since attachment,
outgoing buffered bytes, and interpreter nodes/stack lengths. It detaches on
replacement, report cap or deadline and adds no new timers. It never reads
event payloads, URLs, close reasons or edit bytes, and does not wrap transport
or interpreter methods. The Node harness passes payload-isolation, rebind,
limit and deadline cleanup checks (`tmp/runtime-socket-observer-js.log`).

Coverage begins only at `socket_observer_since_ms`: earlier first edit batches
can be missed. `socket_messages` is a delivery count, not applied edit count;
`socket_buffered_bytes` is queue size, not an acknowledgement counter. The
pinned implementation has no public acknowledgement event/counter. These
observations can narrow a reproduced stall but cannot prove ACK delivery or
resolve the earlier blank launch by themselves. The final locked desktop suite
(807 passed, five ignored) and signed integration run below validate the schema
and live delivery of these fields.


The final signed integration build was launched once through `open -n -g` with
probe and startup tracing (`a3`). No activation rescue occurred during the
twenty-second observation. The socket was absent at .25/1/2 seconds after the
launch request; pings at 5/10/20.5 seconds passed in 57.314/.390/.460 ms. Process
startup trace reported first paint 443.2 ms, conversation paint 743.2 ms and
history paint 3105.2 ms; that clock starts inside the process and excludes
LaunchServices delay, so it is not end-to-end launch latency.

At the last accepted JavaScript sample (10 seconds), the passive observer had
attached at 51 ms, generation one, and counted 44 messages with zero socket
errors/closes or outgoing buffered bytes. Interpreter nodes/stack were 7432/1.
The native twenty-second report had 99 heartbeats, newest 52 ms old. Root
inspected the painted live window afterwards and opened the private test canvas
read-only successfully. This is successful diagnostic integration, not a speed
comparison or reproduction/fix of the intermittent blank launch. Evidence:
`tmp/runtime-controlled-a3-bundle-background.{log,json}`.
