# Draft observation integration: implementation and remaining boundaries

The Editor observation handoff and native projection APIs are implemented and
have disconnected Windows verification; see [draft-native-projection.md](draft-native-projection.md)
and the parity worklog. The original editor plan below is retained as its design
contract, not an unimplemented backlog. **Production realtime observation and
ordinary remote composer autosave remain disabled.** No remote write timer is
authorized merely by completion of the observation/projection APIs.

## Current prerequisite implementation

**Current source and gates: isolated V19**

**Current isolated source: V19; full desktop tests running.** Root installed
one cfg(test) signature change at 17:54:24-28 UTC through installer
`55d9b65f984c00b83648631df38856156c7175695448a63052412d414c02888e`.
Independent source review 70dbffde is CLEAR; root read the helper, four callers
and cfg(test) module declaration. Receipt:
`.validation-evidence/protected-owned-route-test-install/receipt.json`.
Current inventory has 2,417 files, SHA
`e956210c2f04966189dacaa79d18112084ee84b3bad15bdbd3c6df733af95bbc`;
main's 2,351 files are unchanged. V19 formatting passes in 3.346554899995681 s,
with empty output and unchanged inventories.

Full desktop tests are running in root session 1901, started 17:54 UTC:

```text
gate-v19.py v19-desktop-full1 --source-v19 cargo test --locked --no-default-features --features protected-native-fixture,original-download-native-fixture --bin super-platinum --target-dir C:/Users/mortar/Documents/GitHub/super-platinum/target
```

After a pass, build and seal V19's actual current inventory, then compare EXE/PDB
bytes with V18 before deciding whether frame re-extraction is needed. No native
launch or quiet-window request is pending. Read-only `verify_sealed_inputs.py`
is independently CLEAR, SHA
`4210592fa8131a63118758e9e3f59c4cdf9dd9f75d32679d10f9912818ea4ba5`.
It fails explicitly; root will inspect its success before any separate native
launch. It is a verifier, not a runner.

**Prior V18 evidence: test compile failure and cleared frame review.** Root applied V17's six
payload targets at 17:40:44-48 UTC through installer 44a70. V17 inventory SHA
`8b330eacd760742cd9d1428376dd46162c1d87969c82ed497bdbb464f13cc4be`;
installation receipt SHA
`b482c5029f8a2ff260f786f2cfdce5b697a79cdcace81fb238bd958f4aa20b45`.
Root then applied V18's two Python targets at 17:43:35-39 UTC through installer
`465a655546f38e394d05482a51cce288fe6420a8c0389bcf90a94bcaee3b5767`.
Current inventory has 2,417 files, SHA
`dede68e4f2affd8b1c1bf1f7e43917abe15bb7c78182d1fa15d4640d77ae5237`;
main's 2,351 files are unchanged. Root read both complete installer helpers.
V18 formatting passes in 3.4866460999910487 s with empty output; installed Search
Python tests pass **22 tests in 0.066 s**.

V18 default runtime check **passes**, root session 32013 exit 0,
11.985241200003657 s, log SHA
`3fc5d481167e3161e0974e1ab897ec353334d00d07dedaa991432dddbcf897c7`;
whole inventories are unchanged. V18 installation receipt SHA is
`fbcea4d8e394896e0ec58b14ae5c76e15813c7c27f011a1303b6618f1f3025d7`.
V18 native build **passes**, root session 81887 exit 0, 275.71541330000036 s,
log SHA `e7634aa3b568b9c4a00c0b5d87a784099d107da8a4ad94e384d1315ce7e3bcb3`;
whole inventories are unchanged. Seal `tmp/parity-native-v18-build1/build.json`,
SHA `ea0b2085d1bc77e1e5ae3bc25c7661bb6244243bed643dabda947b2435228045`;
EXE SHA `dad1d499679e8f7d07d91eb87136cfc285e83b7363b8891cfd6f483a5a94c34a`;
PDB SHA `648800eeb3498b95338ca5a2ed1f8513b55955a008b54b88a620f3bae91b5c69`.

Actual f40/41bc extraction completed under
`tmp/protected-owned-send-v18-frame-audit1` and
`tmp/protected-owned-send-v18-private-frame-audit1`. Root read seven prologues,
private calls and matching PDB. Known poll chain: `start` 266,520 + 64 pushes,
`Delivery` 281,304 + 64, dispatch 161,176 + 64, wrapper 24,992 + 56, totaling
**734,240 bytes**, leaving 314,336 within the 1 MiB reserve. The constructor
chain totals 491,400 bytes. The comparable V16 poll chain was 1,562,304 bytes.
This measured reduction is meaningful, but extraction success is measurement
completion only: unknown ancestors/callees remain, and total stack safety is
not proved. The independent bounded-retry review is now clear as recorded below.

V18 full desktop test gate **failed compilation before any tests executed**:
E0308 at `owned_route_tests.rs:137`, expected `Applied`, found `Box<Applied>`;
the test helper's return type at line 90 is stale. Root session 12181 exited with
failure after 14.713822499994421 s, log SHA
`cc13417fc733630ba4bc79ac618edadb634346d4adb352385969cccbc0c01e39`.
Whole inventories are unchanged. Root read the helper and four callers; a tiny
cfg(test)-only V19 return-type correction preserves the same output and is now
installed as recorded above. Full tests are running; the original failed V18
package and evidence remain preserved.

The actual V18 frame review is independently CLEAR **for a bounded retry**, at
`tmp/protected-owned-send-v18-independent-frame-review.json`, SHA
`268625b1d2904d7ac6a1f9e81d9a4a8f4f17e74ba4678baeb5cf7bee4ed01967`.
Constructor chain is 491,400 bytes (557,176 margin); known poll chain is 734,240
bytes (314,336 margin), both before unknown frames. This is not a whole-stack
safety guarantee. Next: V19 tests, then a fresh matching build and seal. No quiet
request is pending and no native launch has occurred.

**No V17/V18 full-suite pass exists yet.** No native run occurred after the
Search failure; the V19 correction/test/build sequence remains outstanding. Earlier V16 suites/build/frame findings retain their
original source attribution.

**Latest native result: Search V16 cohort FAILED**, runner exit 1 in
8.5439179 s at `tmp/search-has-native-v16-1`. Normal completed seven stages plus
final (eight PNGs) with a successful receipt; narrow failed the initial midpoint
assertion before any accepted stage and produced one failure PNG. Both processes
closed with empty cleanup arrays. Root read all nine PNGs and found no visual
blocker; `root-visual-review.json` retains that review. The narrow mismatch was
one ULP (1.42e-14 px); the two-file Python correction is now installed as V18.
The original failed cohort is preserved.
Independent failed-artifact audit is complete, SHA
`fdd3d7fc2aaa0ceab27dc31b2b58762eea5a448b4377a463d1018e0c1448feb4`;
the overall cohort remains failed and is not accepted.

The extra immediate preflight failed because the Python `-c` shell quoting was
wrong; root then continued with a separate runner call. **No successful immediate
prelaunch check is claimed.** The prior seal verified source, and post-run
`verify_v16_inputs.py` verified whole V16/main/EXE inputs. The user had replied
Ready now for this separate capture; the mouse is now released with no pending
readiness reply. Canvas/download have not launched. V16 Send remains on HOLD;
independent artifact SHA
`231d39ddde5b2533063eeddacb10f8cf1ca9045cb568c6a69a3e65be8224551c`.

V17 payload candidate
`05aa9d9157d3b0b899857155fabb307c34ae458630edb3a7c6fd74a87a00a29d`
is independently source-CLEAR: six targets using `Box<Applied>`, `Arc<Finished>`
and a boxed Baseline, with size/identity tests. It is now installed as V17 within current V18, as recorded above; runtime
compilation is underway, with no full-suite or native proof yet.

**Prior V16 evidence: build passes, Send retry held on measured frames.** V15 Search and the
V16 poll-stack fix are root-applied; main's 2,351 files are unchanged. V16 has
2,417 files, inventory SHA
`1ca49a03d397a4539b4c7341b3dfaa249512d9490afbcc6cda6a6871d411df7f`.
V15 install receipt SHA
`174486992c76359cba62c131d2171f34832196a06d65e195da71b8cd85602f6e`,
inventory SHA `0c3a7f3663451bc9c5cbf16515e8b8a1637acfae0e63f81076896a4040989194`.
V16 receipt `.validation-evidence/protected-owned-send-poll-stack-install/receipt.json`
has SHA `b92a13539dcff07ba5bdd08cba7f42d785d2cffb42f0cbcac1a217f785bc712f`.

Poll fix 0f34aa15 is independently source-CLEAR: `run_owned` is now a non-inlined
boxed-future constructor and `run_inner` is byte-identical. Static actual-code
evidence identifies a private call edge of 608,552 + 767,336 + 128 push bytes =
1,376,016 bytes, exceeding the 1,048,576-byte reserve by 327,440 bytes. Audit SHA
`8c69141d53a76326d9f068ebb35631d6c0442aaadc322fbcbe3c3f38b3197c7a`.
This is static evidence, **not dynamic attribution of the native2 crash**.

V16 formatting passes in 3.5117094 s with empty output; installed 19 Python tests
pass in 0.071 s and five Node tests in 81.3257 ms. Both fixture features and
`--bin super-platinum` were used for the passing desktop gate below.

V16 desktop full1 **passes 1,903 / 7 ignored**, 250.53945580001164 s total /
9.54 s tests; root session 51726 exited 0. Log SHA
`b6d455e96d99c8f25461e1d4893bd44acdf44e5b71a7126a7d0dedf110ca556e`.
V16 default runtime check1 **passes**, exit 0 in 14.295658500006539 s, log SHA
`4de9f419e6c7c792c8248c58845b79c9b125a917f987fe624ccec24f9427d694`.
Root session 64396 is terminal; before/after whole inventories are unchanged.

V16 native build **passes**, root session 33513 exit 0, 317.9029964000074 s,
log SHA `6065697ba1ffd379429db33af4da12a0e1ee52b0fdba8f3973a379671e6b87d8`;
whole inventories are unchanged. Seal `tmp/parity-native-v16-build1/build.json`,
SHA `b738caf33fc73bca2b8e8e298ad9dc4298cd86cd47e9de452b23e855ad354e20`;
EXE SHA `8be02b0ba98aa1d549f4f2b547320c15d07c4be92c4d0f1d63f79cbe0d653bd2`;
PDB SHA `f6f5caae4a3a833ce7e69d9d2ca6f4ab96b0e63dabb7abc14f9fa07249295242`.

**Native retry is on HOLD after actual frame inspection.** Public and private
auditors ran under `tmp/protected-owned-send-v16-frame-audit1` and
`tmp/protected-owned-send-v16-private-frame-audit1`. The private binary still has
`start` 608,552 bytes directly calling `run_inner` 767,336 bytes at `0x1411896ce`:
with 128 push bytes the edge totals 1,376,016 bytes, 327,440 above the unchanged
1 MiB reserve. The new constructor is 415,640 bytes; its pair totals 1,024,288
before surrounding frames.
The longer constructor chain also exceeds the reserve: boxed `start` poll
24,992 + 56 push bytes, then `start` 608,552 + 64, then constructor 415,640 + 32,
totals **1,049,336 bytes**, already 760 above 1 MiB before return addresses or
other ancestors. Root decision record:
`tmp/protected-owned-send-v16-private-frame-audit1/root-decision.json`.
Helper success is not a whole-stack safety pass.
Root read the actual disassembly and withheld the V16 Send native launch.
A scoped actual stack-size reduction is being prepared, with independent audit
review underway. No new source fix or native result is claimed.

The user was told the reason and the mouse is released. The earlier Ready now
reply preceded this Send hold; no V16 Send native launch occurred. The subsequent separate Search availability reply and failed capture are
recorded above; the mouse is released with no pending readiness reply. No overhead repeat or
main-source promotion has occurred.

Private-frame helper successor
`41bc8f84e1f74d6df6fc103718808b6498c4490177c99ad51963981358816b81`
is independently CLEAR with five tests. Missing dispatch now fails explicitly;
the retained six-frame / three-edge reference is unchanged. Root read the
original helper and exact delta. Native validation remains on hold after the actual V16 frame findings above;
any eventual retry will use existing named repository fixture scripts and checks;
no new combined runner is claimed.

New `gate-v16.py` SHA
`049d12d3f625f070252af549b2915c0d33595190893897ee3d48a3ece0a8df84`;
prior gate helpers are unchanged.

Root released the isolated-source hold to compile the confirmed candidate. The
V14 diagnostic c558 is preserved unrun and now rejects current source. LLDB works
with an official Python 3.11 portable temporary runtime and process-local
environment only; there was no global installation or fixture/debugger attach.
The Ready now reply preceded the V16 frame-audit hold; the user has now been
told the reason and the mouse is released. The earlier
e726 missing-dispatch-role issue is resolved by the cleared successor above.
No Send retry, performance adoption or main promotion is claimed; the separate
Search attempt failed as recorded above.

**Prior V14 evidence (pre-V15/V16).**

V14 source/build gates passed, but native2 failed. V13 wrote all six
initialization targets, then its postcheck failed because `account.rs` was both
a target and a dependency and the helper expected its old base. Failed receipt
`1977b01c57635405102ad171d0a676abf510ea4cada174799d8f5569389b4e3a`
is preserved. Root's read-only whole-tree audit matched all 2,411 expected files
with main unchanged. Metadata recovery through reviewed helper 29f24436 is now
applied: `.validation-evidence/initialization-custody-recovery/receipt.json`, SHA
`df24260253f30ce53e0f8f1f17e24c4240b6e4612f5425e904d4131c8af4e0c9`.
The canonical LF V13 inventory SHA is
`006fa790c46779977df4c75419da663c3981d1649d386215bea608f164715324`.
The earlier 11bef value was only a CRLF prediction, not the published inventory.

The V14 boxing fix is applied through installer
`954963f75e384ebfc5534765a517c03a04b76bddd79354dd75cda9f5091d0f98`.
Receipt `.validation-evidence/protected-owned-send-stack-install/receipt.json`, SHA
`14a70e43f661e5c93a89f7de039c5b8b418bd730ea16ce73830cbbd1b8f19ab3`;
actual V14 inventory has 2,411 files, SHA
`cb5439e51b9ceb86fd300be9a6b38748e708d6083b1c67ebeb0a53c939eb5c7b`.
Main source is unchanged. `v14-root-fmt1` passes in 3.4483424 s with empty output.
V14 account focused1 **passes 18 / 0 failed / 0 ignored, 1,892 filtered**,
348.33239210001193 s total / 0.38 s tests, log SHA
`f8759dd45c9fbce93e3afa88962188c0aba41fea888e7a54a7c105cbea2a2556`.
This includes ten new initialization tests, the adapted Editor test and seven
inventory tests. Combined desktop full1 **passes 1,903 / 7 ignored**,
12.094863499994972 s total / 10.77 s tests, log SHA
`84a65416a99ebfddb94295dece68b746ef6300e66259e68838962b38b7a46614`.
Both gates enable `protected-native-fixture,original-download-native-fixture`;
V14 default runtime check1 **passes in 37.80523730001005 s**, log SHA
`778aa255ca265530576b6274d98477cf4fa69bc63fd7fa7194f1b01b9a7e0eaa`.
Main and isolated inventories are unchanged.

V14 native build1 **passes in 367.7511188000062 s**, log SHA
`253501001810be6b11aa3d46a24c00bb6d568570612c6502b7c74000b3ef0a26`.
Seal `tmp/protected-owned-send-v14-build1/build.json`, SHA
`984d32af6b1dd33e495ddf6b7b2ffcca39f6765f2890bebe5976fef6c2704f83`;
EXE SHA `e2edf97aa9197188d4f65aeaf92efc1b670428e8b76e6a2eccc05dcb5b94689d`;
PDB SHA `31c0fc9b033cb33a33d76c26ae0f8a2f97b91f9235cb537e23e4100606b19603`.
The actual-binary frame audit is independently CLEAR:
`tmp/protected-owned-send-v14-frame-audit1/report.json` measures `start` 490,696
bytes, `install` 136 bytes and pushes 96 bytes, leaving 557,648 bytes within the
unchanged 1,048,576-byte reserve. The construction-frame reduction is 930,416
bytes versus V5. Root inspected the disassembly. This is not a whole-stack or
poll-stack bound, and the native failure below disproves the fix's sufficiency,
not the measured reduction.

**V14 native2 FAILED** at `tmp/protected-owned-send-native2`: the first
normal/channel process again overflowed its main-thread stack after initial/input.
Root session 69182 exited 1; runner elapsed time was 3.8591086999949766 s.
PID 17796 creation/exit values are 134365573442587576 / 134365573471268337.
The closed log SHA is
`ab58e9af4166d4dc0d3c54ee67c77984db349a8a9fdd29ef630d7782da922017`:
one readiness record, four stage edges, zero terminal records and main-stack
overflow. The input after-edge exists in the raw log but was not observed by the
runner's pointer checkpoint. No completed-edge guard or Send/HTTP/durable outcome
is proved. Root read both PNGs: the two-line Unicode input and selection remained
intact, with no visual blocker. The EXE is unchanged, process closed, pointer
restored to [1096, 1388] without movement, and both `cleanupErrors` and wrapper
`finalizationErrors` are empty. Independent failed-artifact audit is complete:
`tmp/protected-owned-send-native2-independent-failed-audit.json`, SHA
`f4af1a8b7b7eb9a3934f4c103ab0c7f89522a41717a2840dbeb14f9c5733ec73`.
Root and the auditor each read both PNGs; root's visual record is
`tmp/protected-owned-send-native2/root-visual-review.json`. This audits the
failure artifacts and does not establish workflow acceptance.

The user mouse is released; there is no automatic retry. Remaining actual frames
are under investigation. V12 Canvas/download captures remain unlaunched; no
capture result is inferred from the earlier availability reply. No overhead
repeat or main-source promotion has occurred. Structured-search native fixture
V2, package `85b9078bde575555385c6fc397b795b752976c4ffcc3d422c495e463357b2055`,
is independently source-CLEAR (12 targets, 25 dependencies, 19 Python and five
Node tests) and installed as V15; the V16 desktop suite passes. Its first native
cohort subsequently failed as recorded at the top; no complete acceptance.

The earlier V14 debugger/source-hold plan is superseded by the current V16
checkpoint above. Diagnostic c558 remains unrun and rejects the new source;
V15 Search is installed. Separate `gate-v15.py` SHA
`a4d960c05b65ca54cc4bd039e53634a35d90f190bcb6cb91e324df64a953432a`
and the original a07 gate helper remain preserved.

Root reviewed the full V12 wrapper v2 composition and preflight passes, SHA
`7cae67ed448df149eb809e24e1654e44e53dc8b667971c10588add6d1f0818df`.
It binds the same sealed e5bd4955 V12 binary only, with exact compatibility to
current V14 and the same 13 unchanged runtime helpers. V12 captures have not
run; native2's failed V14 attempt does not establish their acceptance.

Frame auditor v2 f40ca5fe is independently CLEAR with ten pure tests, limited to
frame analysis. Gate-helper changes only add V13/V14 flags (SHA a07b275c; old
ad15 retained). These tool checks are not V14 runtime validation. V12's source
gates and sealed e5bd4955 Canvas/download binary retain their prior-cohort
attribution; that cohort remains unlaunched.
Wrapper v2 has passed the exact receipt-bound preflight described above; its
native captures remain pending. No overhead repeat has occurred.

**Prior V12 source and sealed-build evidence.** The reviewed Canvas
width fix is installed through independently cleared installer e880905e.
The 2,407-file inventory SHA is
`9224137711542f3c44c15a0cf26788d0568344860914b8376121fd403b8ff65a`;
Canvas installation receipt SHA
`296062bf8330b75d8812bc59430b71cb182fff7e7a5defa41b1a2f42131c5b3e`.
All 19 installed JavaScript geometry cases pass. Root formatting passes in
3.301655 s and core formatting in 2.1983861 s.

V12 core gates pass. Download focused1 passes **eight tests**, including three
new core fixture tests, 75.2326212 s total / 0.14 s tests, log SHA
`4964bfea791f99f98b0912608608313c0d21e9938e29432eeddb7b2db6a7f299`.
Full core passes **2,161 / 5 ignored plus one doctest**, 73.47301490000973 s total /
38.65 s tests / 0.22 s doctest, receipt `v12-core-full1.json`, log SHA
`76196bffc93a77b45b224223d5d17cbdd1f1058bca72b43621d52e9c816abf03`.
V12 desktop download focused1 **passes ten tests / 0 failed / 0 ignored,
1,890 filtered**, 425.5983033999946 s total / 0.02 s tests, log SHA
`bbc788460dec779d2a76754f7df83567b41bc1063c97b3493a8842c4e23c5d09`.
Combined feature desktop full1 **passes 1,893 / 7 ignored**, 11.764522199999192 s
total / 10.80 s tests, log SHA
`0c19ab9597fdd036341bdb67b2962bf9d45dc33d22099c4118d758f91fc8c707`.
The exact command enables `protected-native-fixture,original-download-native-fixture`;
this was the V12 full feature result; V14's 1,903 / 7 pass above now supersedes it.
V12 `v12-default-runtime-check1` **passes in 50.03424599999562 s**, log SHA
`63c39620b1e1c2dfcbc9c367f8d4bd141c5af2d88e78a4e895371b154c6dcfb4`;
main and isolated inventories are unchanged.

V12 native build **passes in 378.53641349999816 s**, log SHA
`909ec130bb4ddd104e0c3444a41ea3feb8108cb3590831eaf029897e7a705a55`.
Root session 24139 is terminal PASS; no build process remains live. Source
inventories are unchanged. Sealed binary
`tmp/download-canvas-native-build1/native.exe`, SHA
`e5bd49556616d737d10828acb37185c85bab0cce784bab691de0fac94043c405`;
`build.json` seal SHA
`1411889d3bff7edd50ea2d358df746615da4101136cd9118428d86978d350cc4`.
Root's exact wrapper preflight and five mocked tests pass. These are not native
capture results. Wrapper f7546aa1 at `tmp/download-canvas-native-run-prep` is ready
for the planned 54-PNG Canvas/download cohort, but it has not launched.
The earlier 12:34 EDT availability checkpoint is superseded by the native2
attempt above; V12 captures still have not run and the mouse is released.

The earlier e99bb43a installer preparation and dd1880acf auditor hold are
historical checkpoints; the applied V14 installer and cleared auditor v2 are
recorded above. The hold concerned incorrect known-prologue classification and
did not retract the manually verified V5 measurements. Earlier plain-Python
invocations and artifacts remain preserved; new invocations use `-X utf8 -B`.
The V14 retry failed as recorded above; no overhead repeat has occurred.

V13/V14 installation and pending validation are recorded above; these V12
results do not validate the successor source.

The preceding V11 installed download native fixture V3 5333e0fe in 16 targets
through installer 95bf3baf. The 2,406-file
inventory SHA is
`a7c83bd6ac2a8ad0151a0ec9fdb02a77505b076828a519ad1965b2812c23e51f`;
receipt `.validation-evidence/original-download-native-install/receipt.json`, SHA
`3a1e6b7e751282a0a7f7ed1a1651a48bc2c07cacd5f8f007056701fe03915494`.
Root's 13 installed Python tests pass in 0.067 s. New Rust and native checks have
not completed; the V10 gates below retain their prior-cohort attribution.
Canvas width fix ac5ab001 is now included in V12 as recorded above.
Initialization subsequently entered V13 and its metadata recovery is recorded
above. Main source is unchanged; no overhead repeat has occurred.

C3/C4, Account inventory, source jobs, structured search and original downloads
remain installed at `tmp/protected-c3-validation1`. **None of these isolated
changes has been promoted to main.** The preceding V10 had 2,400 files; inventory SHA
`002e85127d0a01ab08fbdda7aacbdb76d794bbf8452e43d300dd6bf367826bc2`.
The download installation receipt is
`.validation-evidence/original-download-install/receipt.json`, SHA
`1e664e753f5cf168245e15a1fa89bfefa67db775c86dac2f9dc5ee6adca7e914`.

The preceding V10 source gates are closed:

- Full core: **2,158 passed / 5 ignored plus one doctest**, 68.7555099 s total /
  37.56 s tests / 0.24 s doctest, log SHA
  `433ee72e5e31798700f174fa869a4acdbaf542d03deae84f04aa3e9d3ddf336e`.
  The four download policy and eight HTTP focused tests also pass.
- Full default desktop: **1,885 passed / 7 ignored**, 404.2758797 s total /
  11.13 s tests, log SHA
  `d32bbd55faf73d45bad0552f75d82eb6205ba9f1393a12398b447f89272d11ad`.
  All five `download::tests` are verified in the log.
- Non-test protected check: **PASS**, 52.4116729 s, log SHA
  `d0888640e943e914fb22b958fa9fbebd29f943a518b04119f2e5e9d6e6e3680d`.
- Root and core formatting: **PASS**, 3.3275846 s and 2.2389225 s, empty logs.

No V10 full feature suite was repeated; its contemporary full-feature evidence
was V8b's 1,884 / 7 pass. V12 subsequently passed combined full feature
**1,893 / 7 ignored**; the latest V14 result is **1,903 / 7 ignored**. V9 and V10 each had a passing runtime
feature check. Main source remains at the Rich cohort: full core **2,130 / 5
ignored plus one doctest** and root/protected checks pass. Main's latest full
desktop **1,824 / 7** and full feature **1,830 / 7** remain pre-Rich evidence;
no later main desktop run is implied. Those V10 checks preserve their sealed
source attribution; they are not Rust validation of the V12 fixture and width-fix cohort.

**Native evidence and remaining work**

Canvas cp1 is independently CLEAR for its **older ae21a080 cohort only**:
eight distinct closed processes / 40 original PNGs / 32 ordered stages,
43.504966 s, runner exit 0. Root and the independent auditor read all 40 PNGs.
The binary's 2,287-source seal and pre/post 11-helper runtime audits pass; logs,
counts, geometry and SQLite checks pass (integrity OK, active empty, one archive).
All eight pointer restorations pass with empty error arrays. Owned-process
termination exit 1 is controlled termination, not graceful shutdown evidence.
Independent audit `tmp/canvas-cp1-independent-audit.json`, SHA
`67f7d199ed8e2c11786e164352980289d9e2e89c55bb6a71aafa353e9b387bc7`;
root run receipt `native-cp1.json`, SHA
`0ff3fb0d73e2244db295fc4eac2328e66875cbcd07319d9c7f313c7222037cee`;
root visual receipt `root-cp1-visual-review.json`, SHA
`6b6de64288c2ead4090359e70336ad111abe90134d26b4ad8c4352b8593c663c`.

The primary Delete paragraph / Saving / Checking / Unconfirmed labels still
change button width and shift Cancel. Stable loading geometry remains an open
visual follow-up: V12 contains the source fix and 19 geometry checks pass, but
fresh native validation is pending. cp1 proves bounded
synthetic-ACK and controlled-restart behavior, not live Slack, crash recovery or
trusted IME behavior. It does not validate isolated V12.

The first V5 native attempt **FAILED** at
`tmp/protected-owned-send-native1`: the first normal/channel process overflowed
its main-thread stack after the initial/input stages. No terminal receipt was
emitted. Root personally read the two captured PNGs; the pointer was restored,
`processExited` is true and `cleanupErrors` is empty. The independent audit is
finalized below. This is preserved failure evidence, not an accepted workflow.
The sealed binary is unchanged:
`tmp/protected-c3-native-build1/protected-c3-native.exe`, SHA
`1d20595240a83acef69d0bad91af0345e80876d289780b1f5a162f636bf16752`.
Root's preflight V3 a6459a65 passed immediately before launch; it supersedes the
older c2237e4a checkpoint for this attempt. No rerun or overhead repeat occurred,
and the mouse is released. Older protected, emoji and Canvas evidence retains
its own source/helper attribution; this failed V5 attempt does not validate V12.

The failed V5 audit is finalized at
`tmp/protected-owned-send-native1-independent-audit.json`, SHA
`6cd6d69f9f84a9850b4ec88adbf3fba89001a2fc0f6f13c8bb21c271ddcbc503`;
root visual record: `tmp/protected-owned-send-native1/root-visual-review.json`.
The native input after-edge exists, but the runner's pointer checkpoint did not
observe it; no completed-edge guard is claimed. With no terminal or SQLite export,
there is **no Send, HTTP or durable-outcome proof**.

Independent measurement of matching EXE/PDB bytes found the direct call chain:
`start` 490,696 bytes + `Root.install` 930,552 bytes = 1,421,248 bytes, exceeding
the 1,048,576-byte main-thread reserve. The minimal `Box::pin` source candidate,
manifest `8de8bfa9aa907d339cce6378c3c41cd10753dba8487665ca850b7134f862f330`,
is independently source-CLEAR and now installed as V14. Its Rust suites and construction-frame audit pass, but the V14 native retry
still overflowed as recorded above. No overhead repeat has occurred.

The download native fixture is installed as V11 with 13 Python checks passing;
its V12 Rust gates and sealed native build pass; captures are unrun. The OS save dialog remains separately
unverified. Source preparation and passing suites do not establish
native or live acceptance. cp1 ran after the fresh Ready now reply; the mouse is
released again. The subsequent V5 attempt and V14 retry both failed as recorded
above; no automatic further retry is planned.
No Cargo work, source changes or overhead repeat accompanied cp1. Slack CDP was
rechecked reachable (Chrome 152.0.7977.130, protocol 1.3), so no restart was needed.

**Boundaries for continuation**

Keep the main observer pins (`agent.rs`, `main.rs`, `state/boot.rs`, `fixture.rs`,
`media.rs`) unchanged through the pending diagnostic. The C3/C4 changes are
installed and validated in the isolated tree only; no root promotion is implied.
Preserve immutable binaries, inventories, runtime-helper closures and explicit
override receipts. Validation uses the copied independent lock and explicit
shared core target directory; desktop tests use `--bin`, core uses `--lib` /
`--doc`, and the unrelated example that references temporary source is excluded.

Production protected-store activation, ordinary remote autosave, realtime
observation ingress and native Slash activation remain off. Broader route,
migration, drain and parity work remains incomplete. Earlier accepted native
slices retain their recorded cohorts and disconnected limits. Historical failures,
zero-test filters, superseded package holds and intermediate states are retained
below and in [the dated worklog](parity-worklog.md); they are not current instructions.

## Historical execution snapshots

### Retained Rich-to-V10 execution sequence (historical)

The following text preserves the successive source reviews, failures, fixes,
receipts and gate results recorded during this sequence. Its intermediate
"pending", "uninstalled", "unrun" and freeze instructions are superseded by the
current status above. They describe their original checkpoint only.

**Current source and gates**

Isolated Account **V7 passes all 16 new focused tests, full default desktop
(1,864 passed / 7 ignored), and full feature desktop (1,870 passed / 7 ignored)**.
The final non-test feature check and formatting also pass; all component gates
are closed. Source-jobs V8b gates are closed, including feature desktop (1,884 passed / 7
ignored) and formatting. Isolated Search V9 passes 15 focused core tests and full
core (2,146 passed / 5 ignored plus one doctest). Its default desktop, runtime
feature check and formatting now pass, closing V9 source gates. V5's receipt-bound
preflight successor is independently clear. Download V10 is now installed
isolated-only; its four policy and eight HTTP focused tests pass, and full core
passes 2,158 / 5 ignored plus one doctest. V10 desktop (1,885 passed / 7 ignored),
runtime feature check and both formatting checks now pass, closing its source
gates. Full feature 1,884 / 7 remains V8b evidence; no V10 feature full was repeated.
Main source is unchanged. The earlier compilation failure is preserved, and V5
build/preflight evidence retains its original attribution. No native launch or
overhead repeat has occurred. Details and exact receipts follow.

**Newest source cohort: Rich v2**, independently reviewed 543fa..., installed
exactly five targets. Receipt
`tmp/slash-rich-storage-root-integration-prep/installation/root-integration.json`,
SHA `c405db020a2070ddc5c13a2548355ce29033d00203ebfb720f9b151536fa926f`.
Reviewed Rich test correction 78b71ea... is installed exactly in one cfg(test)
target, source 1cc0b8d..., receipt
`tmp/slash-rich-step-oracle-fix/root-integration.json`, SHA
`dd88d3976944c22f31ef7d5e63b2ea1978298e73b7583ac72da7a74308f30cce`.
Focused 2 **passes 19 / 0 failed, 2,116 filtered**, 29.8914107 s total / 5.76 s tests,
log SHA `ad3e0cf880b71b6419255ac4191ec6a92bc63b39eee0e81f6bc62495a01c237d`.
Executed SELECT now requires the exact no-such-table error; pinned WAL negative
proof is unchanged. The original 17 / 2 preparation-oracle failure is preserved.

Before this test-only fix, root-check passed 52.4712793 s, log SHA
`af49338a9a5143c3f8d53e0253ffb44fbe5566eada71a54e46a2357d95d36a60`,
and protected check passed 54.4338234 s, log SHA
`ee150cead6ec6e618309bbfff26399b4da6472d59cceca1fbe62b15a35741755`.
Rich full core 2 **passes 2,130 / 5 ignored + one doctest**, 39.5419317 s total /
36.99 s tests / 0.19 s doctest, log SHA
`aba9ac6e11d3efaecf9c39e8face41e4bbe9e303aa99cd6f9a047e7a8d213191`.
Final formats pass: core 2.29449 s /root 3.2531314 s, empty logs. Earlier root/protected
checks remain passing. No repeat default full desktop is required for the private
core plus cfg(test)-only correction; this does not claim a new desktop test run.
No native or overhead repeat has occurred.

Emoji preflight v4 independently passes after three exact receipts, artifact
preflight-complete-v4.json, SHA
`b67409d3b727037ef9618569a38c3db8f929e7d7555c439a49a976f39f794f72`.
No current runtime-pin drift is found.

The source gates below and both immutable binaries are **pre-Rich evidence**.

All pre-Rich source gates pass: independently locked core **2,111 passed / 5
ignored + one doctest**, default desktop **1,824 passed / 7 ignored, docs 0**,
and protected-native-feature desktop **1,830 passed / 7 ignored, docs 0**.
Both Bookmarks picker VDom tests pass. Final format checks pass (core 2.3326955 s,
desktop 3.2571312 s). These completed gates cover the installed C0/Slash/typed-bridge
and Bookmarks cohort; those gates are closed for the pre-Rich cohort. Rich focused19 and full core2,130
plus formatting also pass, as recorded above; native verification is separate.

Protected feature evidence: 329.8603196 s total / 10.27 s tests,
`tmp/bookmarks-picker-root-gates/desktop-native-feature1`, log SHA
`df2f3d505000c1a1f26fbfec07b2484a1d1a25645812bcbef9e746a3c4b178df`.
Earlier compile, fixture and wire-comparison failures are preserved in the dated
ledger. They are historical failures, not the current gate status.

**Builds and native verification**

The immutable protected build **passes in 386.135375 s**, with 2,343 unchanged
source entries. Binary SHA
`71487d17c014ef487aa76935e6cc088eda8d119efed83043dda61f39d88ca54e`,
log SHA `076b7455254598c9008c6f36ab3389c458d7612f429ccaaa29150854b581fa6e`,
seal SHA `6444fdeeb5a109611b630a5b7e598b2ebfee8780cd7fb0cb02985a828ebff578`.
Root's full runtime verifier passes all 17 helpers.

The immutable emoji build **passes in 415.0895318 s**, with 2,343 unchanged source
entries and unchanged pins. Binary SHA
`a5b54faeb3be13e6a144f02657486b51d1143a0d12c1d315594537e8652a4d32`,
log SHA `b633f0bf1462e330158d9275c36d1fc6f9246e73c0228012a48ad41d099b793e`,
seal SHA `3e637c8966f88ed876c3252b8fe8a0aa2e0b7060d7ff149a6e5f78dca5c858ed`.
Original full preflight passes 32 targets/ 81 pins, 14 helpers and retained overhead,
recorded in preflight-complete.json. No native launch or overhead repeat.

Reviewed cleanup 55642... is installed exactly in three targets, explicit override
receipt `tmp/protected-bookmarks-cleanup-root-gates/install-1/override-receipt.json`,
SHA `a4845c050a2ffde2dabcd338d301d7bb727c7d556e9e18cdd5f9b85642ff3f70`.
Installed verifier 17 passes; root's 10 mock tests pass in 0.139 s. This post-build
override is separately attributed, not represented as original sealed source.

The older Canvas ae21 binary and its exact 11-helper closure remain separately
preserved and audited. Captures are already authorized by the goal, but there
has been no fresh mouse-availability reply; the old List availability reply does
not establish current availability. No captures have started. Slack CDP remains
reachable, so no Slack restart is needed.

**Accepted evidence retains its original cohort**

Subitem si2 is independently CLEAR: four processes / 36 PNGs, root all 36 read,
46 calls / 10 opens / 72 edges and cleanup verified, on earlier f895/C1 plus
visual-fix source. Si1 remains a historical visual HOLD. Earlier Lists/Canvas
74 PNGs belong to af267fa9; D pa4's 150 PNGs belong to 89687b72. None validates
the newest source or establishes live/trusted-IME/current-server-history behavior.

Packed native evidence is audited, but its delivery/joint gate failed despite
capture improvement: **no packing adoption or repeat cohort**. Ordinary thin is
unchanged. The one-shot emoji diagnostic overhead test passed its budget once;
its TLS-versus-production-OnceLock limitation remains, with no production-latency
or retirement-adoption claim. Exact receipts and binary attribution remain in
the [dated worklog](parity-worklog.md).

**Next actions and limits**

C3 source f706... is independently CLEAR for compile feedback. Root created
`tmp/protected-c3-validation1` with helper 6c23544c...: 2,351 source/context inputs,
2,370 final isolated files and exactly 53 overlays. The root source is unchanged;
all five observer pins remain untouched. Isolated inventory SHA
`702c98fd55a5ac054a7d7550691688c9d33f30417f2bf696bb6aff9c8e390e97`;
evidence is in that copy/.validation-evidence. C3 isolated core validation preserves two zero-test attempts: the filename
filter compiled but ran zero tests (72.4359488 s, log SHA
`20cf66176f33056119cba3c768c9d34d8fbbbc3e26d0ed22738c9c661ad3ae75`),
and a guessed alias also ran zero (0.4026503 s, log SHA
`1f30d394713ce2650ed9e7062f64e36c9f101bad87a0c49ad509955e9cf69f6d`).
Neither provides test coverage. Correct filter
`owned_queue::operations::tests::inventory` passes **three tests**, 0.4902737 s
total / 0.15 s tests, log SHA
`b13b713ad32cf2857d97db4e863612745775af8a97386a3c53459e602f23295d`.
The `local_fifo` filter passes **three tests**, 0.5894872 s total / 0.25 s tests,
log SHA `8153e0c0407d21907aed96782467f1203d96546a1dfe766bbd9d495a6a35a3b7`.
Evidence remains under `tmp/protected-c3-validation-root-gates`; root/isolated
inventory checks are unchanged. Isolated desktop route-focused1 **failed compilation; no desktop tests ran**.
At owned_route_tests.rs:358, tokio::spawn requires Send, but actual protected
execution Evidence retains Cell<bool>/Cell<Option<u64>> across an await. The two
diagnostics are retained, 145.8719005 s total, log SHA
`3f8d8cd169e240718c80d17a7ab46abfa9ceb6ddeefac5dc1cb6ab4a60561461`.
Root/isolated source checks are unchanged. The test-only local polling correction is now isolated-only, with **no production
Sync change**. Original f706 stays frozen.

Isolated C3 full core --lib **passes 2,136 /5 ignored**,36.3675687 s total /
35.95 s tests, log SHA
`70e9ce3afe0413df0e0d11ef019a4be825881af4e2740815c7373cbccd128ee3`.
Separate core --doc **passes one test**,35.6189233 s total /0.20 s test, log SHA
`25df29755a8352f91c184b27d872525f32a9b7811d213fc0f2a2c75d10470ea8`.
Test-only amendmentaba80df6... is applied **only to the isolated owned_route_tests.rs**;
receipt copy/.validation-evidence/local-http-test-fix/receipt.json, SHA
`3a14cd45ed39895f05519c811c22b25a51727556f4d84f83a448459703683892`.
Isolated source v2 SHA
`9aee77939843a0e88e8bb425c2a1f42fab5a534c29b99ade85990940cb3a377a`;
the original source record is preserved and root source remains unchanged.
Isolated desktop focused2 compiled and ran: **10 passed /2 failed /0 ignored /
1,831 filtered**,5.62 s tests /297.9030552 s total, log SHA
`8623c7883459058eaa41a7a5df322dbd464dc9b3aabbc95ac0e0690eeca09277`.
StartLocal rollback retry returns Pending at test line508; retry-inventory
retirement times out at line622 before reaching the held read. Initial handoff,
post-await retirement and local FIFO paths passed. The author is investigating
the actual retained retry flow; **no correction or full desktop pass yet**.
Original isolated source v2 and root remain unchanged. Isolated protected-check1 **passes in63.7800338 s**, log SHA
`c5f16e2037c09173c953c27399688f6113eae2e6c3b1f734171c71c67fcc0007`;
default-check1 **passes in59.1735017 s**, log SHA
`4c4f86e2f2b1cf03bbc3bcced957822f24577dcbda49fbbd7abaec8ce96729c5`.
Main and isolated sources were unchanged for these checks. Desktop focused2
remains **10 passed /2 failed**; compile checks do not clear those failures.
Reviewed retry amendmentae716993... is installed **only in the isolated copy**,
receipt SHA `50545126a9a261c00888f581b3dbb708f196cf8d82ec621f2736d79842ad5a20`.
Isolated v3 source inventory SHA
`4981ae1fc0f15ad89885ea410d74ee997e9319b30f3d36841a62f82effb16509`.
Isolated C3 focused3 **passes12 /0 failed**,180.4392498 s total /0.76 s tests,
log SHA `da3ebc320e668fd90971b3834ea716192052f15d7eb4209e1a4c646ca60f7c1d`.
Root and isolated sources remain unchanged. Isolated C3 full default desktop **passes 1,836 /7 ignored**,10.7147302 s total /
9.84 s tests, log SHA
`1becd75450945680715fdc6e5a818ac8d79d95e45a036ff046b633cf4a682932`.
Core fmt1 passes2.4949219 s and root fmt1 passes3.2986606 s, both with empty logs.
All source inventories remain unchanged; receipts are under
`tmp/protected-c3-validation-root-gates`. Core is unchanged since the isolated
2,136 /5 plus one doctest pass. **Feature full has not run**; C4 is now installed only in the isolated copy
and its combined default passes1,848/7; feature full passes1,854/7; native build passes; independent helper audit/default check pass, no launch. No native acceptance or root promotion is claimed.

C4 is installed **only in the isolated copy**, receipt
.validation-evidence/c4-install/receipt.json, SHA
`a4f1ef95766f764c1f6a96c005343b817058e128463cadbd000945056a9b2768`.
Isolated v4 inventory SHA
`f921647638cfc0a2ac1caed41f89aea7d1d49d4f38078711f32fbe94d9e4fb13`.
Source focused tests **pass five**,306.1804027 s total /0.13 s tests, log SHA
`2e859811141bfe511c14e1a820f717a5359dbbdcb42f201f444df3f8faa81d50`.
Observation focused tests **pass seven**,1.0235999 s total /0.15 s tests, log SHA
`9eec433e3fbfb21f9ec930b47fb91ef9645b5d8d4a57577d314eca2a4fa5878c`.
Main and isolated inventories are unchanged during validation. Isolated C4 full default **passes 1,848 /7 ignored**,10.8686241 s total /
10.16 s tests, log SHA
`5d3a73f0f15ad82adcb27577c7f498eec9457aa303e29f40e0d52b8c0625ce18`.
Isolated C4 feature full **passes 1,854 /7 ignored**,330.1329403 s total /
10.54 s tests, log SHA
`de282b478f68b7296c04c5b4c69457b2a5611acf3951f018531707dcbf7144ed`.
Core remains the isolated2,136 /5 plus one doctest pass because no core files changed.
Final c4-root-fmt1 passes3.4059807 s with empty output; main and isolated checks
remain unchanged.

Runner3629 is installed **only in the isolated copy**, seven new scripts, receipt
.validation-evidence/owned-send-runner-install/receipt.json, SHA
`a547644a68be21adbe9a1700ade918b9fae3fb0f664559d9d4e1df7933a49acf`.
Source v5 has2,381 files, SHA
`6b7d6c1f2cb8c559d1358926a23b9c2f4bfa2e9a59906afd65cfa1b1ca1df4f4`.
Installed41 Python tests pass in1.079 s. Exact build-helper855c check passes;
Isolated V5 native build1 **passes in384.9611627 s**, with main and all2,381
isolated source entries unchanged. Binary
`tmp/protected-c3-native-build1/protected-c3-native.exe`, SHA
`1d20595240a83acef69d0bad91af0345e80876d289780b1f5a162f636bf16752`;
log SHA `403a8be6da2357ba5a3a0b36f6340183becbb005650ab382fc4119a3596eea34`,
seal SHA `d5e0132426c7a60208b4e573e21090c115e2dbfa489f1ecfa772ab64154bdf33`,
result SHA `4f559f1785aee704e02b1a0960562467fe046e5e98f611e0f6ac2cd3723284be`.
Root read-only preflight passes422 runtime files for four planned processes /
20 stages, receipt `tmp/protected-c3-native-preflight-prep/preflight-complete.json`,
SHA `3ddc558eea516a825cf8dc90ff0ccf0c71910c4e40b57df276b4437918bfe64a`.
V5 final c4-default-check1 **passes in41.1569954 s**, log SHA
`26d084721138e315e43ecb761960e9e761efe39c00fc8a7437b234891d6d631c`;
both inventories remain unchanged. The V5 build/preflight is independently CLEAR:
the reviewer rechecked all422 runtime files, exact seal/result/binary and the
seven-target/17-pin helper closure. **No native launch has occurred.**

Account component bc7c2a99 is now installed **only in the isolated V6 copy**:
15 targets (10 existing, five new), using independently cleared installer
`a6de37b517dcd92e827009f46cf0354dc786bd6c3e97edeca745461534b0a44d`.
Root's seven scratch tests pass. Receipt
`.validation-evidence/account-inventory-c4-install/receipt.json`, SHA
`d43c868fb1a168350d84cd798d6be5e89334da82ffcf4b5f6babef03762f48b8`;
V6 inventory has 2,386 files, SHA
`b23921623e5aaf7aad097c715713bc5798fabb80a9dff672fce46e9f14faf133`.
Main source is unchanged. This supersedes the unexecuted f904 installer, whose
missing `-X utf8` child invocation was corrected before installation.

V6 runtime `c4-account-default-check1` **passes in 35.5964691 s**, log SHA
`681eff532503b7881f30b896ab276ea1b8dda64699ee16a752d43d9ae5bc4d5a`.
Ownership-focused1 then **failed compilation with nine private Guard diagnostics**
in existing view/composer tests, 53.82764 s, log SHA
`fd23bae6a2d6f0a9a29d5e5f8bff90b7a4c2019870a1e84878c3627d2e604048`.
No tests executed; this is a test compilation failure, not a runtime failure.
The independently cleared visibility amendment
`a406ac466394e9f2c00e058106aa20a337a39f7f5e8145a68c7d0b2599f1f39e`
is installed **only in the isolated copy**, changing four visibility declarations
in one `ownership.rs` target, with no test or body changes. Receipt
`.validation-evidence/account-inventory-visibility-fix/receipt.json`, SHA
`3ce97fc18199036c73c26ac2e6b5e0ea4579ead00bbe1ce11bcfc1b3e75f944b`;
V7 inventory SHA
`dbd2ebce8300b009906847ad562ab95047bccd72d643252c273ab85b602c0a42`.
All 16 new isolated V7 focused tests **pass**:

- Ownership: five tests, 307.9265855 s total / 0.00 s tests, log SHA
  `94a40b6493965b34ff31d57eb99e89e2029a8f3a44e62204b13a0acd002116f1`.
- Account inventory: seven tests, 1.2589608 s total / 0.29 s tests, log SHA
  `0ce6a6b7fbc1f96210589bcedb1f5a0bf36a2f5f6e5bac12039ccc1a32280904`.
- Owned inventory: four tests, 0.9074997 s total / 0.24 s tests, log SHA
  `373f6a25488f3ce9fb53a9eacb05fb348e37aa9b78a11ce3fcabf53c2c288c89`.

Isolated V7 full default desktop **passes 1,864 / 7 ignored**, 11.3970552 s
total / 10.67 s tests, log SHA
`3240d563875860d2e316175becd66cb4962cc00a4ef69b3178f226e61a149d0e`.
Isolated V7 `c4-account-feature-full1` **passes 1,870 / 7 ignored**, 342.0381494 s
total / 10.75 s tests, log SHA
`9d5ccac7a79a3307e42faa0f8a1e9c0fc910fcea30426a58a558eb9cce0097d8`.
Isolated V7 final gates are closed: `c4-account-protected-check1` **passes in
42.0360923 s**, log SHA
`cdb06cca81254a11cc961c48a887e726beb0b25d3970e63515b9f9bd845e2ebf`;
`c4-account-root-fmt1` **passes in 3.4349952 s**, with empty output.
All 16 component focused tests, default desktop 1,864 / 7 ignored and feature
desktop 1,870 / 7 ignored now pass. Main and isolated inventories are unchanged.
The earlier 35.5964691 s default runtime check belongs to V6, before the four
visibility-only declarations; the feature runtime check above covers V7.
Core source is unchanged: main core 2,130 and isolated core 2,136 / 5 ignored
plus one doctest retain their separate attribution. V5 remains the sealed,
unlaunched native cohort. No native launch or overhead run has occurred.
Source-jobs work remains separately staged and uncompiled; it is not covered by
these component gates. Cargo is quiet.

The broader parity audit's two follow-ups are assigned for temporary preparation
only: original downloads beyond the media cache limit to attachment_integration,
and structured `has:file` / `has:link` search filters to read_navigation_fixture.
Neither assignment establishes implementation, integration or acceptance.
Source-jobs V8 is installed **only in the isolated copy**, exactly nine targets;
main source is unchanged. Installer 613319f9 is independently CLEAR, with seven
root scratch tests passing in 0.292 s and a passing read-only check. Installation
receipt `.validation-evidence/source-jobs-c4-install/receipt.json`, SHA
`89b642f680f9e84d387503187829fb08a5e59bcedc4c133423891651534abef5`;
`isolated-sources-v8.json` has 2,393 files, SHA
`d3097290c43d19621233f69dde10005796e4586c5863bd652aaf586ffbb592d5`.
V8 `c4-source-jobs-focused1` **failed compilation before any tests ran**:
three diagnostics at two cfg(test) sites, 54.4178797 s, log SHA
`964550a6fa54bfcd04fff6bd22cf796d832491e7278c955d0b270f88e9bcd5e0`.
The non-test protected check **passes in 39.1902102 s**, log SHA
`0c6b0293213efe99e4fb9deb6445cd082bebe2fb6d7dc35ea4a6b804c5209867`.
Reviewed test-only fix 7b9124ad changes two expressions in one isolated test file;
receipt `.validation-evidence/source-jobs-renderer-compile-fix/receipt.json`, SHA
`bbc9a649ec262e5e7b91b865c1ae196afb177694d2d0349cfa0ebae0876e2a8a`.
V8b inventory SHA
`6d0c8c706f3a3abe8507f02227693374d03091a840c64c75ad0ee74df94f3d0f`.
Isolated V8b focused2 **passes 14 / 0 ignored, 1,871 filtered**, 322.3288644 s
total / 0.38 s tests, log SHA
`6907eabb60f34b0339df09c19bcd11266cabc937aad4db05f463138686d4e32a`.
Desktop full1 **passes 1,878 / 7 ignored**, 25.0391183 s total / 11.28 s tests,
log SHA `1c5acb86ca0cde6b7efa5ec24c322ccf55db2dfad24ee61e7a9365b5db254118`.
V8b feature full1 **passes 1,884 / 7 ignored**, 352.3917026 s total / 11.20 s
tests, log SHA
`8cc584d8b30ea2bb2c1a59dd2723fad82c00da89cc2fce920e548d591f7745d3`.
Root formatting passes in 3.4393513 s; V8b gates are closed.

Search V9 is installed **only in the isolated copy**, four targets through the
independently cleared installer 54c497. Receipt
`.validation-evidence/search-has-install/receipt.json`, SHA
`aeb2364df31c026a8534089bed0889e2850a4000b98d6445b5d003219d8d10e5`;
V9 inventory SHA
`c59b1da44264cc03b344f969e1962b7fed462745547a0eb03d49d985996d0851`.
Core focused tests **pass 15**, 43.6218167 s total, log SHA
`c4befc3296595f84a2bda1f93f601af1c65d1ecfaf5ca60ec0daf9eff069f7fe`.
Isolated V9 core full1 **passes 2,146 / 5 ignored plus one doctest**,
56.8224617 s total / 39.31 s tests / 0.29 s doctest, log SHA
`411475d06b1b1efa3878cc38d7f28fd9ab5ce427c1a245a4344bc44d73446d9c`.
V9 desktop and remaining source gates now pass, as detailed below.

Download successor
`fbcfb7cf04f6d468bd50972b200cfb5ef16cf8e179c854276547b44938bb9711`
is independently source-CLEAR. It corrects actual original-only URL entry points
and specifies a stable 96 px button. Its core test results and isolated V10 installation are recorded below;
desktop validation now passes, with no native acceptance. Original package 783's review hold remains historical evidence.

Search V9 source gates are closed. Desktop full1 **passes 1,880 / 7 ignored**,
332.4238544 s total / 11.16 s tests, log SHA
`cc14e287d81ffda7dcebd2c21953456c306fb05c985e01f352dabaf451b781a5`.
Protected non-test check1 **passes in 29.1948429 s**, log SHA
`6d1ae300961db516a168a7ae776820c8b4a4830aa7e0a704fe0445d82a1ceaba`.
Root formatting passes in 3.3271708 s and core formatting in 2.2201482 s, both
with empty logs. Source inventories are unchanged. Core full 2,146 / 5 ignored
plus one doctest and 15 focused tests already pass. No V9 feature full was
repeated: common search is covered by default full and the runtime feature check;
the latest full feature suite, 1,884 / 7 ignored, belongs to V8b.

V5 preflight successor c2237e4a is now independently CLEAR, binding one embedded-CSS
override while keeping 421 runtime files strict. Download V10 installer 364ece4e
is independently CLEAR and passes root verification: 13 targets, main inventory
2,351 files, V9 inventory 2,393 files, expected V10 inventory 2,400 files.
Original-download V10 is installed **only in the isolated copy**, exactly 13
targets, through installer 364ece4e and source package fbcfb7. Receipt
`.validation-evidence/original-download-install/receipt.json`, SHA
`1e664e753f5cf168245e15a1fa89bfefa67db775c86dac2f9dc5ee6adca7e914`.
`isolated-sources-v10.json` contains 2,400 files, inventory SHA
`002e85127d0a01ab08fbdda7aacbdb76d794bbf8452e43d300dd6bf367826bc2`.
The alias is preserved and main source is unchanged.
V10 core gates pass:

- Original-download focused1: four tests, 65.105096 s total / 0.03 s tests, log SHA
  `7c1cfbe0a9eb85e4b0bddde999a07b4600bbee4d4f2df854af43e2c7c8eefdff`.
- HTTP focused1: eight tests, 1.1914069 s total / 0.78 s tests, log SHA
  `0f2d12460d3184337f21598d38ae10732ed0f6e96fe51ff3b3c8a4e7fa96e4ac`.
- Full core1: **2,158 passed / 5 ignored plus one doctest**, 68.7555099 s total /
  37.56 s tests / 0.24 s doctest, log SHA
  `433ee72e5e31798700f174fa869a4acdbaf542d03deae84f04aa3e9d3ddf336e`.

V10 source gates are closed. Desktop full1 **passes 1,885 / 7 ignored**,
404.2758797 s total / 11.13 s tests, log SHA
`d32bbd55faf73d45bad0552f75d82eb6205ba9f1393a12398b447f89272d11ad`;
all five `download::tests` are verified in the log. Non-test protected check1
**passes in 52.4116729 s**, log SHA
`d0888640e943e914fb22b958fa9fbebd29f943a518b04119f2e5e9d6e6e3680d`.
Root formatting passes in 3.3275846 s and core formatting in 2.2389225 s, both
with empty logs. Full core 2,158 / 5 ignored plus one doctest and all 12 focused
core tests already pass. No V10 feature full was repeated: the latest full feature
suite, 1,884 / 7 ignored, remains V8b evidence; V10's runtime feature check passes.
All inventories and main source are unchanged, and Cargo is quiet.

The download native fixture at `tmp/original-download-native-prep` is still WIP:
ten hooks plus a checker/local server, with no freeze, compilation or native run.
The OS save dialog remains separately unverified. V5 preflight c2237e4a is
independently CLEAR. Fresh Canvas mouse availability is still pending; no native
launch or overhead run occurred during this continuation.

Slack CDP was rechecked
reachable (Chrome 152.0.7977.130, protocol 1.3); no restart was needed. No native
launch or overhead run has occurred.
The earlier nine-diagnostic compilation failure is preserved. V5 remains the
sealed, unlaunched native cohort and does not provide V7 native proof.

Prior C3-only full passes retain
that earlier isolated source attribution.

Runner successor
`3629ecf84d96980c1fdf483d4fd810379ab1c609796bf14b1fb048d7435573b6`
is independently CLEAR (seven targets/17 pins); root's41 Python tests pass in
1.438 s. It is installed only in the isolated copy; native build passes with preflight, no launch yet. Prior eddd review HOLD remains
historical evidence; successor source clearance is not native acceptance.

Runneredddbf02... passes root verification
of seven targets/17 pins and36 Python tests in1.676 s. Native is still unrun;
these checks do not establish native acceptance or root source promotion.
C4 is installed only in the isolated copy, focused five/seven tests pass, and
combined default passes1,848/7; feature full passes1,854/7; native build passes; independent helper audit/default check pass, no launch. Pin updates are coordinated with acceptance_audit. No native acceptance or root promotion is claimed.

Validation uses the copied independent lock and explicit shared core target
directory. Desktop tests use --bin; core uses --lib/--doc. Examples are intentionally
excluded because an existing example references arbitrary temporary source. This
is isolated compile feedback, **not root promotion**. Native runner and 20 PNGs
remain pending. C4 source/observation inventory 5139f845... is staged: six targets,
17 pins and 12 authored tests; review is pending and it is not installed.

C3 route source package
`f706f82f44aefdf6d3bbed22527a1481ea53cc39958f79bb382cd983cfd09d6c`
is frozen at `tmp/protected-store-c3-route-binding-prep/route-source-review-v2`:
53 targets / 360 pins / 18 authored Rust tests unrun. nativeLaunchReady=false;
the strict runner remains outstanding. It overlaps five observer pins (agent.rs,
main.rs, state/boot.rs, fixture.rs, media.rs), so root created the isolated
compile-validation copy described above. **No root promotion before the pending diagnostic**;
this is not installed or native-accepted work.

Rich focused/full core and formatting pass. Continue only the separately
scoped preparation and diagnostic work below; no native result yet. Both
immutable builds and the reviewed helper override are ready for their separately
attributed native checks once mouse availability permits. Keep the current source cohort frozen through these builds.
Rich v2 and its test-only oracle correction are installed; focused 19, full
core 2,130 and formatting pass. C3 route source package
`f706f82f44aefdf6d3bbed22527a1481ea53cc39958f79bb382cd983cfd09d6c`
is frozen at `tmp/protected-store-c3-route-binding-prep/route-source-review-v2`:
53 targets / 360 pins / 18 authored Rust tests unrun. nativeLaunchReady=false;
the strict runner remains outstanding. It overlaps five observer pins (agent.rs,
main.rs, state/boot.rs, fixture.rs, media.rs), so root created the isolated
compile-validation copy described above. **No root promotion before the pending diagnostic**;
this is not installed or native-accepted work. Other route, migration
and drain work remains incomplete.

Production protected-store activation, ordinary remote autosave, realtime
observation ingress and native Slash activation remain off. Broad parity remains
incomplete. Temporary implementation work does not authorize a production switch,
and passing source tests do not replace native or live verification.


### Earlier historical snapshots

These retained snapshots are superseded by the current status above. Their
running/pending/failure statements are historical, not current gate results.

All six matched emoji benchmark processes complete exit0 (264 samples /3,168 phase
values /24 warmups). Independent report audit and final SHA are pending; desktop
projection remains unpromoted and no native/frame gain is claimed. Lists view31
with21 tests remains staged under review, while D checker work continues.

Combined full desktop passes **1,690 / 7 ignored** after **5m45s compile and
8.48s tests**, docs0, in `slash-completion-thin-desktop-full.log`. Independent
core remains **1,755 / 5 ignored plus one doctest PASS**.

The thin lifecycle example's explicit locked dev build passes exit0 after the
full tests, recorded in `emoji-thin-lifecycle-dev-build`. Root retains verifier,
toolchain and no-override records. The matched six-process run is active at
`tmp/emoji-thin-lifecycle-measured-1`, starting with full0. No measurement result
or gain is claimed yet. Desktop projection remains staged and there is no new
native result.

D shared23 and fixture13 preparations are separately source-review CLEAR; their
checker/runner is not frozen. They are not covered by the completed current gates.

Earlier snapshots below preserve their original gate status.

Combined core full passes **1,755 / 5 ignored**, plus one doctest (0.21s), after
**42.34s compile and 23.17s tests**, in `slash-completion-thin-core-full.log`.
All **20 completion tests**, including corrected remote v2, and all **six thin
namespace tests** (three new parity / three frozen original oracle) pass. The
increase from1,729 to1,755 is26 tests. Root/core formatting passes; both earlier
completion failures remain preserved.

Full desktop is running in `slash-completion-thin-desktop-full.log`. The thin
desktop projection remains unpromoted; no new benchmark measurement or native
result exists yet. Earlier desktop1,690/7 and native988abc4f retain their prior-
cohort attribution. Passing core behavior tests do not establish a performance
gain or production activation.

Earlier snapshots below preserve their original gate status.

Test-only fixture v2 is installed exactly, SHA
`52bdcfe70a19b5cef2d4d91c125f4c0f40a44950dc35392b463e257d4ebdded2`,
with reviewed backups/receipt. Its full-record-to-successor path correctly expects
**epoch0 / Unverified** and the matching guard message. No production behavior
changes; both earlier failure logs remain preserved.

The thin-catalog package719ba74a... is independently CLEAR. Root promotes only
**six core/benchmark targets (one replacement / five new)** after exact pins,
bases and artifact checks, with backups/fresh mtimes. Root/core formatting passes.
The seventh desktop target is **not promoted**, pending measurements. Core now
contains thin Catalog compatibility changes; canonical workspace state is unchanged.

Combined core full tests are running in `slash-completion-thin-core-full.log`,
covering completion20, three new parity tests and frozen-oracle tests. No corrected
combined pass, new benchmark result or native result is claimed yet.

Earlier snapshots below preserve their original gate status.

Protected Slash completion5f712cc... is integrated exactly (nine targets: four new,
five hooks), with package/base/absence/patch and root/core formatting checks passing.
Fresh WriteAllBytes mtimes avoid the prior copy issue. Full core fails
1,748 passed / 1 failed / 5 ignored (56.23s compile, 23.33s tests): the new
acknowledged-remote-base test gets ErrStorage at f.claim().unwrap before c.complete
(completion_tests.rs:22). Installation and 19 other new tests pass, not the full
gate. First fixture-fix focused rerun also fails (0 passed / 1 failed / 1,753
filtered;23.42s compile,0.02s test): line399 sees Unverified, not expected Local.
The earlier Local explanation was incorrect/incomplete. Successor logic at
mod.rs:245-255 invalidates basis on acknowledged identity change, setting
Unverified/epoch0 and requiring a new positive baseline. Both failure logs are
preserved. Test-only v2 is staged with the proper guard message; no corrected
pass or production guard change is claimed. It remains natively unwired, with source-reviewed exact local ACK clearing and
preserved remote/newer/unknown state. Later rejected local-only archival is distinct
from NotSent and separately reviewed. No native process is active. Startup cs4/cs5
independent audit is complete within the limits recorded in RESUME/worklog.

Emoji lifecycle baseline is independently audited CLEAR (132 samples / 1,584
phase values / 12 warmups / 48 rows); this is synthetic component wall time, not
an optimization or live-frame result. Thin name/value work is tmp-only. Startup
cs4/cs5 is independently CLEAR with both PNGs read; the one-process/probe-enabled
two-RAF observations establish no speedup or causal/live/idle claim. Legacy
cleanup evidence is limited to verified process closure and pointer restoration.
See RESUME/worklog for exact measured costs and report hashes.

The actual rebuilt lifecycle focused gate passes **20 tests / 1,677 filtered**
after **3m28s compile and 6.63s tests**, in
`protected-lifecycle-focused-rebuilt.log`. Full desktop passes **1,690 / 7 ignored**
in **8.29s**, docs0, in `protected-lifecycle-desktop-full.log`. Root formatting
passes. The initial zero-test invocation remains excluded from validation.

Core source is unchanged; **1,729 / 5 ignored plus one doctest PASS** remains
current. No new native build exists: immutable988abc4f predates lifecycle A-C.
D implementation is released only into tmp from its reviewed plan, without
factory, selector, autosave or realtime activation.

The pure emoji lifecycle benchmark plan2040b... has source equality and pins
verified by two independent agents, but has not run. No benchmark result or
performance improvement is claimed.

Earlier snapshots below preserve their original gate status.

Lists lr4 is independently **CLEAR** at normal and narrow sizes: all **16 PNGs**
(eight stages each) read, without clipping or obstruction. Final captures show
the exact Delivery title focus ring and correct inline Material icon.

Each process records **eight exact requests: seven reads / one synthetic mutation**,
with no replay. Lost ACK and stale checks are unapplied; the fresh check applies
at serial12 across visits1 to15. The only schema delta is the name; unknown fields,
two full records and cursor `fixture-opaque-unloaded+/=` remain exact. Each has
one readiness,16 edges and one terminal. PNG CRC/IDAT/dimensions, actual988abc4f
binary hash and process creation/exit checks pass; cleanup/auditErrors are empty.
Normal/narrow retain **47 / 46 pointer checks**, restored to [1546,637], with owned
foreground and sampled taskbar clearance. Runner exits0; expected owned-process
cleanup code1 is controlled termination, not crash-durability evidence.

This accepts the bounded disconnected Lists rename workflow, not live mutation,
global pagination, trusted OS input or crash recovery. Preserve lr1-lr3 failures
unchanged. Lifecycle focused rebuild is still compiling; no lifecycle pass yet.

Earlier snapshots below preserve their original gate status.

Lists lr4 passes at both sizes with the root process closed. Root personally
read both final PNGs: focus ring is on the exact renamed header, and the inline
Material icon is correct in normal/narrow layouts. Independent acceptance_audit
review of all **16 PNGs and the complete ledger** is still active; final acceptance
awaits that audit.

Lifecycle A-C is installed exactly across **eight targets**, with backups/receipt
and formatting PASS. The first `protected-lifecycle-focused.log` invocation ran
**zero tests (1,677 filtered)** because Copy-Item retained candidate mtimes older
than the last test build. That run supplies **no coverage**. Root touched all
eight files to current UTC without changing bytes; real compilation is now running
in `protected-lifecycle-focused-rebuilt.log`. No lifecycle test pass is claimed.
Native988abc4f remains pre-lifecycle evidence.

Earlier snapshots below preserve their original gate status.

The feature build prints Cargo Finished after **4m44s**, but PowerShell stderr
redirection returns wrapper exit1 with NativeCommandError. An explicit Cargo
repeat that exits with $LASTEXITCODE confirms **exit0 / 0.91s**. Preserve both
logs; the wrapper failure is not silently rewritten as success. Immutable
`tmp/completion-baseline-binaries/native-focus-paint-protected-slash.exe` has SHA
`988abc4fdfd3503e4526406e8919f26a112e8462cb5ae5e8f9bc15e924a5c70a`.

Lists lr4 is running at both sizes, logged in
`list-column-rename-native-run4.log`. No acceptance is claimed before terminal
results and independent audit. Lifecycle A-C final source is independently CLEAR,
manifest1c3175ec..., across **eight targets / 20 authored tests unrun**, and remains
staged. This does not change production activation or current gate coverage.

Earlier snapshots below preserve their original gate status.

The combined desktop gate passes **1,670 / 7 ignored** after **3m49s compile
and 7.10s tests**, doctests0, in
`focus-paint-protected-slash-desktop-full.log`. All three performance::paint_tests
pass: empty acknowledgment avoids render while normal writes render; real elapsed
notification occurs once; and one pending update preserves the other. These are
behavior regressions, not measured native performance gains.

The feature build is running in `focus-paint-protected-slash-native-build.log`.
No new executable/native evidence or Lists acceptance is established yet. Core
remains **1,729 / 5 ignored plus one doctest PASS**; prior native receipts retain
their original binary attribution.

Earlier snapshots below preserve their original gate status.

The Lists focus/header correction95a7cc... is integrated exactly across **seven
targets (five replacements / two additions)** after all base, absence and
candidate checks. Backups and receipt are retained at
`tmp/list-rename-focus-return-prep/root-integration-receipt.json`. Root formatting
passes. Installed **12 actual-source Node tests pass (78ms)**; independent review
also ran all 12 successfully. The strict native final assertion is unchanged;
failure diagnostics and ledger evidence are expanded. README/failure-audit notes
are additional documentation outside the frozen source manifest.

Combined desktop tests are running in
`focus-paint-protected-slash-desktop-full.log`, covering atomic Slash, the three
previously unrun paint VDom regressions and the focus correction. Core source is
unchanged, so **1,729 / 5 ignored plus one doctest PASS** remains current. There
is no new native binary or Lists acceptance. Preserve pe2 acceptance on076ce07d
and original lr2/lr3 failures without reattribution.

Earlier snapshots below preserve their original gate status.

The two-target paint candidate99dd... is now integrated exactly after independent
current-base review and root verification of both candidates and all nine source
audit checks. Root-before backups and `root-integration-receipt.json` are retained
in `tmp/completion-startup-idle-prep`; frozen manifest-listed files are unchanged.
Root formatting passes; the **three actual VDom regressions have not run**.
No startup, CPU or memory improvement is claimed. Core formatting passes and
atomic core1,729/5 plus doctest remains the current core result.

Independent lr2/lr3 failure review has read all **16 PNGs** (seven stages plus
diagnostic for each). Binary/process checks and **76 / 74 restored pointer checks**
pass; no other layout blocker is identified. The failed terminal lacks the raw
eight-request ledger, so exact wire proof remains pending. Final focus return
still fails; a minor header-icon wrap fix was also requested with narrow scope.
No Lists workflow acceptance follows from the completed failure-image audit.

Root awaits the Lists focus fix before combined desktop tests/build. No Cargo
or native process is active.

Earlier snapshots below preserve their original gate status.

Protected Slash atomic core full passes **1,729 / 5 ignored** after **38.43s
compile and 23.29s tests**, plus one doctest (0.23s), in
`protected-slash-intent-core-full.log`. All **16 new tests pass**. Desktop1,667/7
remains a pre-atomic gate and needs a later rerun.

The user supplied a quiet window. Lists lr2 normal and lr3 narrow then ran
sequentially on unchanged076ce07d; both **FAIL the same final focus-return
assertion after the observed stage**. Each retains seven admitted-stage PNGs
plus one diagnostic, **16 PNGs total across the two failed cohorts**; these do
not establish the completed planned matrix. The native assertion combines a
closed rename overlay, Delivery title header and activeElement data=C_NAME.
Root's normal failure PNG shows the correct title and closed overlay but no
header focus ring. The author is investigating/fixing; acceptance_audit is
reading both failure cohorts. No Lists acceptance is claimed.

Original lr1 pointer interruption is preserved. Captures are finished and the
user was told the mouse is available again. No Cargo/native process is active.

Earlier snapshots below preserve their original gate status.

Enrollment pe2 is independently **CLEAR across all 12 cases / 80 PNGs** on
076ce07d. Both normal40 and narrow40 captures are read; complete validators,
closed SQLite/log/readiness evidence, unique PID creation/exit times, exact hash
and pointer checks pass. Normal audit retains **219 pointer checks and 80 capture
checks**, with restoration; target clocks are 5/4, 4/5, 7/4, 4/4, 6/4 and 5/4.
The narrow audit passes the same full oracle and identity/hash/pointer contracts.

Positive and canceled scenarios enqueue exactly once. Late typing, owner change
and failed Editor enqueue zero times. Native clear never occurs; newer failed
input survives. Canceled completion reports currentfalse, followed by explicit
reload without replay. The panel sits above the composer and saved/pending
sections are fully readable. Scope is isolated local state and synthetic input,
not restart, live Slack, global HTTP instrumentation or production activation.

Separately, protected Slash atomic296a692b... is integrated across **10 targets
(three new / seven hooks)**. All 21 package hashes, current bases/absences and
backups are verified; receipt:
`tmp/slash-protected-intent-prep/root-integration-receipt.json`. Root formatting
passes. Core full is running in `protected-slash-intent-core-full.log`; **16 new
tests have no result yet**. Earlier core1,713/5, desktop1,667/7 and native076ce07d
validate the pre-atomic cohort only.

Lists retry awaits the user's quiet-window reply; no native run is active.
Lifecycle A-C preparation is being staged after G0 passed, with no active-source
writes or additional production activation implied.

Earlier gate snapshots below retain their original source/binary attribution.

The combined desktop full suite passes **1,667 / 7 ignored** after **5m12s
compile and 7.93s tests**, docs0, in
`slash-journal-list-rename-desktop-full.log`. Independent core remains **1,713 /
5 ignored plus one doctest PASS**; root formatting already passes.

The feature build passes in 5m46s. Immutable native-list-rename-enrollment-fixed.exe
has SHA `076ce07d194fc40c5bee2e6393e2bca7af5ab831c25c16dadf785575660b79fa`.
Pe2 completes all 12 runs with exit0; all80 PNGs are under independent normal40/
narrow40 audit. Lists lr1 fails before admission/capture due to pointer movement,
with its owned process closed and only a waiting receipt, no terminal or requests.
No UI defect is inferred. Dependent Lists captures await a requested quiet-window
reply; no Cargo/native process is active. Full enrollment acceptance is pending; the source/test passes do not establish native
acceptance or production activation.

Earlier gate snapshots below retain their original status.

The combined core suite passes **1,713 / 5 ignored** after **56.32s compile and
23.67s tests**, plus one doctest (0.19s), in
`slash-journal-list-rename-core-full.log`. All **51 new core tests** are included.

Runner failure correction689580... is now installed exactly across two targets,
with backups/receipt retained. Installed enrollment Python checks pass **27 tests
in 5.545s** (prior24 plus three new tests). The active source cohort is journal14,
Lists37, fixture-status2 and runner2. Full desktop is running in
`slash-journal-list-rename-desktop-full.log`; enrollment pe2 and Lists rename
native verification remain unrun. The core pass is not native acceptance.

Earlier integration snapshots below preserve their original gate status.

Root integrated the reviewed durable Slash journal9a657... across **14 writes**.
The original seven-file inert core remains unchanged except reviewed hooks; no
runtime activation occurs. Root reviewed the installer. Applied receipt:
`tmp/slash-root-integration-prep/runs/d7c107cb882140468904e2e86e074be8/applied.json`.

Combined Lists core936c... and desktopc4bbe... integrate exactly across **37
targets**, with **36 effective new Rust tests (18 core / 18 desktop)**. Receipt:
`tmp/list-rename-root-integration-prep/runs/20261015T014838Z-1fb3042da354/receipt.json`.
Installed Python checks pass **10 tests (0.051s)**. An initial wrong unittest
filename matched zero tests and exited1; it was immediately corrected to the
actual file. The zero-test invocation provides no coverage.

Root/core formatting passes. Enrollment status two-target fix457157... is also
integrated exactly, with **seven adapted actual-JS checks passing**. No CSS,
shared-toast behavior or hit guard is weakened. Runner failure two-target
fix689580... is independently CLEAR with eight tests, but remains staged and
uninstalled.

The combined core suite is running in `slash-journal-list-rename-core-full.log`.
All newly added Rust/native validation is pending; prior core1,662/5 plus doctest
and desktop1,649/7 are historical pre-cohort gates. Next: complete core, install
the reviewed runner fix when released, then desktop/build gates, enrollment pe2
and the 16-run Lists rename native plan. No acceptance follows from integration.

Earlier receipts below retain pre-cohort source/binary attribution.

The isolated enrollment package is integrated exactly across **32 targets**,
manifest `1c5a8d0cf5dea35a99ed314d773d64110a5a7641505c081b1fa8ec5c1aa92b0d`,
with backups/receipt and all 120 dependencies verified. Installed **24 Python
checks pass (5.464s)**, **17 actual-JavaScript checks pass**, and formatting passes.
Feature cargo check passes in **1m02s**, recorded in
`protected-enrollment-feature-check.log`. No enrollment native or feature-off
runtime result is established by that compile check.

The four-target Canvas visible-controls correction is also integrated exactly,
manifest `4cbc39aada206acb6f65b6c0d45c97873444fcc8e87584e21b2d9996a9d1fa1e`,
with backup/receipt. Installed **583 Python checks** and **17 exact-JavaScript
geometry checks** pass, along with formatting. Combined default full desktop
passes **1,649 / 7 ignored** (2m48s compile, 6.52s tests, docs0); root formatting
passes. Default executable build passes in 5m46s; immutable
native-enrollment-canvas-default.exe SHA is
`29851c547fa8a506681fb413423be82d00e54d298d296a66fe88badb643a2b88`.
All 18 actual feature-off probes pass owned PID/creation/exit, exit2, exact stderr,
empty stdout and unchanged hash checks; no ordinary boot was requested. Receipt:
`tmp/protected-enrollment-negative-off1/receipt.json`. Core production code is unchanged;
independent core **1,662 / 5 ignored plus one doctest PASS** remains applicable.

Corrected Canvas cl2 is independently CLEAR: all 32 PNGs read, eight distinct
closed processes with creation/exit proof, exact29851c54 hash, clean logs/cleanup
and 112 restored pointer checks. Narrow controls are fully visible/readable with
correct enabled states; clipping/center-hit checks pass. Both read-only SQLite v2
files pass integrity/unchanged-audit-SHA, zero active/one unknown-outcome archive,
and exact original body/raw inventory/observation/timestamp/decision through the
fourth process. Seed2/3/2; later phases0/0/0; no Session or Transport.
Feature build passes in 4m59s on immutable77bd9abc (full hash in the dated ledger).
All 16 actual feature-on preflight probes pass exit2/exact stderr/PID-time/hash
checks in `tmp/protected-enrollment-negative-on1/receipt.json`. Enrollment pe1
fails its first normal case before stage captures/consent; all others are unrun.
The sole failure PNG is inspected: status remains hidden because the fixture
action row does not match shared toast-dismiss height measurement. Fixture-only
selector/JS regression and runner original-error preservation are staged; no
guards are weakened and no enrollment acceptance is claimed.
The earlier
cl1 semantic/storage/process audit retains its original visual limit; cl2 now
closes the narrow gap for disconnected controlled restarts, not crash durability,
live Slack or trusted OS input. Protected production
activation remains OFF.

Earlier Canvas local-review source/native evidence below retains its original
revision and binary attribution.

Canvas local review is integrated exactly across **15 targets**, manifest
`4ed191444b6268ef93da2fd511e2339e1f11f5c8d7c13f924677060d44ec550a`,
with root-before backups and integration receipt retained. Root/core formatting
passes. Independent source review is CLEAR, with **406 pure checks** passing
before integration; the installed Python suite also passes all 406 checks.
Corrected independent core full2 passes **1,662 / 5 ignored + one doctest**
(24.95s compile, 23.24s tests, 0.19s doctest); both formatting checks pass.
The original 1,661/1/5 failure is preserved. The sole cfg(test) correction uses
from_responses and valid Inventory to reach intended owner rejection, without
production changes. Its exact SHA is
`2c738abda4861218153719f92bcd73945277751603f7dda58e662e594539680a`.
Full desktop passes **1,649 / 7 ignored** (4m22s compile, 8.04s tests, docs0) in
`tmp/windows-bootstrap/canvas-local-review-desktop-full.log`. The native feature
build passes in 5m12s. Full --local-review at `tmp/agent-ui/cl1` exits 0 with all eight phases and
30 PNGs read. Independent semantic/storage/process audit passes: eight closed
processes, 108 restored pointer checks, both read-only SQLite v2 files intact and
unchanged, zero active/one archive, exact operation/body and fourth-process
outcome_unknown bytes/timestamp preserved. Seed2/3/2, later phases0/0/0. This is
bounded success, not full narrow visual acceptance: checkbox/Cancel/Remove are below
fold in both confirmation captures, and query clicks lack scroll/hit proof. A
minimal extra-stage visible-control/geometry correction is staged; corrected
32-PNG verification is pending and original cl1 artifacts remain unchanged.
The immutable native-canvas-local-review.exe SHA is
`91fb820df3159b37a5a3b7edb04db3a507c6260230fe247edc6b0babbb2886e8`.
Earlier core 1,654/5 plus doctest and desktop 1,644/7 belong to the prior source.

Before integration, root found and fixed review::open adoption checking only the
serial before loading. The admitted result now checks original owner, file,
visit and receipt plus the exact new Read; the regression is included. This is
reviewed source behavior, not a completed Rust/native gate.

The native run completes **eight phases / 30 PNGs** with the limits above; no local-
unlock acceptance is claimed. Archive v2 migrates with limits of **32 records /
64 MiB**, preserves unknown outcomes and does not prune. Capacity management is
not implemented. Broader production activation boundaries remain unchanged.

Earlier source and native receipts below retain their original attribution.

The corrected feature build passes in **4m12s**. Immutable
`tmp/completion-baseline-binaries/native-canvas-hydration-recovery-fixed.exe` has
SHA `0b5c379a9d21cedb5d1a417b7cb6b10fe4b096f392978a461b37b46fc9a34f75`.
The later test-only allocator change does not alter this production binary.

Canvas cu2 on 15d19670 is independently CLEAR: all **22 PNGs** read, full process
and hash evidence, **107 pointer checks** and closed logs. Raw request/ACK behavior
remains source-asserted. Canvas cr2 on 0b5c379a is independently CLEAR at both sizes:
**four processes / 16 PNGs**, exact pending SQLite body unchanged from seed through
restart/final and actual read-only database inspection, **56 pointer checks**, four
identities and U1 scope. Seed counts are 2 dispatch / 3 reserve / 2 retire; restart
counts are 0 / 0 / 0. Controlled termination exits 1; this is not crash or live proof.

Lists lh1 on 0b5c379a is independently CLEAR at both sizes: all **14 PNGs** read,
three exact builders including records.open, distinct opens 10/16/18 with serials
11/17/19 and finish results [true,false,true], and **zero mutations**. Raw
populations/responses retain the opaque +/= cursor unchanged, exclude stale
results, and preserve omitted children and unrelated records. Closed logs, exact
hash/process evidence and **83 pointer checks** pass without Session/Transport.
The final narrow unrelated row is below fold, supported by the raw receipt and
visible in normal; required controls remain visible. This accepts the bounded
synthetic-response workflow, not live HTTP or global pagination coverage.

Desktop full3 passes **1,628 tests / 7 ignored**, with **2m35s compile and 6.36s
tests**, in `tmp/windows-bootstrap/canvas-hydration-desktop-full3.log`. Root and
independently locked core formatting checks pass. Core remains **1,654 / 5 ignored
plus one doctest PASS**. Preserve the earlier desktop failures and the distinction
between the source-based filepath-collision inference and logged evidence.

The default build passes in 6m28s. Three actual feature-off requested-fixture
probes (alias + CHECK1, alias only, CHECK1 only) pass with exit 2, exact stderr,
empty stdout and unchanged SHA; no ordinary launch is claimed. See
`tmp/protected-native-negative-default/receipt.json`. The five-target inactive protected discovery package is now integrated exactly,
manifest `9752fceda0e2139ce5c1e931651e5216e33bc92df05b1ecb691ccc8a6f85ca66`.
This replaces400210... only for the reviewed documentation dependency refresh;
all five source files and the patch are unchanged. Root-before backups and
`root-integration-receipt.json` are retained. Root formatting passes. Focused2 passes **16 / 0** (3m49s compile, 0.43s tests);
full desktop passes **1,644 / 7 ignored** (0.86s compile, 7.93s tests, docs0) in
`tmp/windows-bootstrap/protected-discovery-desktop-full.log`. Core remains
1,654 / 5 plus one doctest; no core source changed. No build is active and
there is no native discovery/enrollment evidence yet.

The first promotion attempt stopped before writes because a documentation hash
had changed. Accidental shell continuation then ran zero matching tests in
`protected-discovery-focused.log`; that run is **not validation**. Preserve the
log and do not count it as a successful focused gate. No production discovery
activation is implied by this inactive integration.

An isolated enrollment native fixture is being staged. Canvas local review,
List column rename and the Slash journal extension remain staged/review work.
The original inert Slash core (seven files plus two declarations) is already
integrated and compiled, with 27 tests included in passing core suites from 1,572
through 1,662. Only journal9a657... and its 33 new tests remain uncompiled;
journal-only integration is being prepared. No new Slash runtime activation is
claimed.

Quiet completion performance is independently audited with descriptive whole-
binary limits; see [RESUME](RESUME.md) and the dated worklog.

Detailed earlier source manifests, frame measurements and failed gate logs remain
in the [dated execution ledger](parity-worklog.md); evidence below retains its
original source/binary attribution.

Prior source gates remain core **1,572 / 5 ignored** plus one compile-fail doctest,
desktop **1,527 / 7 ignored**, and the5m16s main dev build; they do not validate this
new cohort. Prior Canvas corrections preserve unknown encoded bytes/order while
normalizing only known identity tags1/6/7. The earlier main SHA
`e3d70217d2d5ac60fd487cf66bb9a9197087f55cbe6e7ac9c7a6499e87d66a29`
is retained as immutable `native-schedule-reviewed.exe`. The corrected Schedule
matrix `ms9/` passes all **22 scenario/restart pairs / 44 closed processes** on
this binary. All **154 PNGs** are independently read (normal42+35, narrow42+35),
with exact full SQLite and pointer/process contracts. Total prepared exercise
requests are20; restart requests are zero. This verifies the retained host,
fixture ownership and stale-retirement corrections within disconnected synthetic
DOM/HTTP and controlled-restart scope. It does not prove live Slack, trusted OS
input or crash durability. Earlier failures remain retained under their binaries.

On the earlier `497d9587` binary, all four typing-recovery cases in `sr3/` pass independent audit,
with all 20 PNGs inspected: managed and Editor views are settled at revision 3
before the read and revision 4 afterward, exact SQLite inventories match, four
`drafts.info` reads and zero mutations are prepared, and 183 pointer samples pass.
This verifies the bounded publication wait without weakening the strict validator.
Creation `ms2/` normal rich-channel and rich-thread pass both restart pairs with
14 PNGs; independent acceptance audit is still underway. Narrow rich-channel
fails because the persistent fixture success toast overlaps one panel hit point
(six failure-run PNGs). This is fixture feedback obstruction, not a CSS defect;
actual toast-dismissal and host corrections subsequently pass the strict ms9
matrix above. Those earlier failed runs retain their original failed status.

Earlier `7796e904` retains 31 recovery cases / 35 successful processes / 153
inspected successful PNGs, six inspected publication-lag failure PNGs and two
inspected creation-positioning failure PNGs. Binary `068eae94` retains 24 channel
recovery cases / 28 processes / 120 inspected successful PNGs and four inspected
thread-remount failure PNGs. Acknowledged Send remains on `cd881dcc` with 72
inspected PNGs; explicit Update recovery remains on `da7ebcd0` with 52. Managed
raw-rich Schedule creation and rejected Retry/Change UI are integrated. These
isolated results use synthetic input/responses and do not establish live Slack
behavior. Production observation ingress and ordinary remote composer autosave
remain disabled; the broader parity goal remains open. Earlier gates below retain
their original scope.

`integration::route` checkpoints existing managed editors or delegates to the
legacy local writer; it does not enroll ordinary composers or schedule network
dispatch. Realtime draft frames still do not enter the captured observation owner
pipeline. Existing explicit conflict review still rejects pending operations.
The new explicit recovery path checks a frozen uncertain Send or Update; it does
not enter conflict review or enroll ordinary composers.

The following recovery source changes pass independently locked core tests
(1,368), desktop tests (1,401), formatting and Windows dev builds. The earlier
recovery binary `151e9a95` is retained as failure evidence. Native `ur-final3/` passes all six normal pairs
(26 inspected PNGs) and four narrow pairs (18 inspected PNGs). Narrow cooldown
fails because duplicate toast feedback covers the inline status; all three
failure PNGs were inspected. Narrow pending and cooldown restart were not run
on that binary; the succeeding complete da7ebcd0 gate is below. See the parity worklog for exact evidence and retained failures.

The pure `sync::owned_index` module is now compiled: independently locked core
tests pass 1,384 / 5 ignored, including sixteen new routing tests. It indexes
only durable acknowledged IDs and pending Create UUIDs, with historical identities
as collision barriers and full-row/instance revalidation. Host invalidation on
account visits, release, re-enrollment and inventory replacement remains required.
This helper has no production ingress hook. The subsequent desktop suite passes
1,405 / 7 ignored, including four feedback tests; the dev build passes in 4m37s
on `da7ebcd0`. Recovery now passes all twelve normal/narrow scenario/restart pairs
in `ur-final4/` (normal) and `ur-n4/` (narrow), with all 52 successful PNGs inspected
and exact durable/process/SHA/pointer receipts. The earlier pointer abort remains
separate. These are synthetic inputs/responses and controlled process termination,
not live Slack, OS IME or crash durability. Terminal-local preservation is now
integrated and independently reviewed: fourteen new core/store and four desktop
regressions pass within core 1,398 / 5 ignored and desktop 1,427 / 7 ignored.
Exact terminal Send/Schedule provenance permits newer local input without changing
the frozen request; legacy format-5 rows remain loadable without gaining authority.
Acknowledged supported-text Send is integrated and reviewed, with its seven new
tests passing in the subsequent desktop 1,434 / 7 ignored gate. Formatting and
whitespace checks pass. Managed Schedule and ordinary autosave remain separate work.

1. Raw server denial envelopes are separated from trusted transport failures in
   `drafts/network.rs`. A raw server error named
   `draft_mutation_not_sent_offline` cannot reach the trusted predispatch
   sentinel classifier. Clean known
   denials may release ownership; contradictory mutation evidence, malformed
   acknowledgments and unexpected outcome fields must retain raw uncertainty.
2. `network/update_readback.rs` uses a single `drafts.info` POST form read of the
   acknowledged server draft ID. Positive confirmation requires exact captured
   account/session, ID, client UUID, destination, supported lifecycle, frozen
   payload and a strictly newer version. `recovery/ui.rs` preserves current typing
   and checks the exact optional editor identity at projection. Update confirmation
   never releases its editor, clears its composer or uses Send delivery cleanup.
3. Format 5 persists a full-operation check ticket before requesting data. An
   unresolved ticket remains blocked indefinitely across restart. Completion,
   monotonic cooldown and any positive receipt commit in one SQLite transaction.
   Failed receipt storage retains a process-local completion for an explicit
   storage-only retry; after restart, the unresolved durable ticket remains
   blocked. There is no automatic request replay or continuous polling.

Required regression cases include the server-spoofed offline sentinel, clean
versus contradictory denial, wrong identity/content/version, stale callbacks,
typing during readback, restart-preserved uncertainty/cooldown and failed storage.
Uncertain Create has no acknowledged ID: do not invent a UUID lookup API.
Missing info/list results do not confirm Delete. These prerequisites alone do
not establish ordinary autosave or live synchronization parity.

The strict decoder deliberately refuses unsupported extra draft fields, even
when their values are empty or zero. Production `Transport::execute` preserves
positive raw responses but converts negative read envelopes into API errors;
those errors never confirm an Update. The isolated native fixture covers actual
prepared POST forms and shared persistence/UI code with synthetic responses; it
does not prove live Slack payload compatibility or make a live request. Existing
frozen Reading/Escape captures do not verify this new draft cohort.

The subsequent ordinary observation decoder alignment and pure reviewed Schedule
helper pass core **1,408 / 5 ignored** and desktop **1,439 / 7 ignored**, with both
format checks. All fifteen focused observation tests pass. Optional absent/false
`is_from_composer` now matches the ACK/review ordinary contract; unknown/partial/
true or malformed lifecycle evidence stays blocked. The Schedule helper preserves
raw reviewed blocks against full predecessor equality but has no production
caller. Exact Editor/native dialog handoff and Schedule-specific recovery remain
required. Retained primary source: B SHA1681ecc18bbfead52efb391b1ccfb3e15f1b4a1093a8475d847b221ce196c537,
Id6n6810246 for existing-ID schedule and UYdH6818245 for exact draft lookup;
S SHAf085df09fb4f938caf425ea66fb0244c8f92a8012eea8eb298e4f629b377dbf7,
uGgu1594178 for request argument conversion. These establish positive known-ID
lookup, not absent-result failure, server CAS or a UUID-search API.

The next combined source gate passes independently locked core **1,418 / 5 ignored**
and desktop **1,468 / 7 ignored**, both format checks and whitespace checks.
Known-ID Schedule readback and full-inventory Editor/Owner handoff are integrated,
with no Schedule creation or recovery UI caller yet. The acknowledged-Send native
fixture passes 28 Python receipt and 11 actual-JS tests; all twelve native
normal/narrow restart pairs pass on cd881dcc with all72PNG inspected, exact durable
inventory and zero restart requests. See the worklog for complete attribution.
Production ingress, ordinary autosave, durable continuity and Schedule UI activation
remain separate work.

## Existing boundaries

- `observation::WorkerSource` now captures immutable credentials and exact socket
  parameters before connect, with a distinct per-team worker allocation. Its
  one-use parameters launch that worker. In one shell write, call `can_accept`
  before changing the core generation, then `accept` to derive the generation's
  `Source`. Retain it before awaiting frames; never recapture credentials around
  a queued frame. `Source::prepare` also requires the original frame generation
  and checks account, epoch, transport, credentials, authenticated workspace user
  and worker identity/liveness. Disconnect, retirement and drop invalidate old
  proofs. The source helper is integrated but not hooked into realtime.rs;
  its ten new desktop tests pass in the combined desktop **1,468 / 7 ignored** gate.
- Its conservative decoder produces a private `Prepared` observation. It does
  not locate or enroll a destination owner. Unverified lifecycle/content fields
  on a matched draft remain raw unsupported evidence rather than a narrowed
  active draft. This decoder is not proof of complete live payload coverage.
- `Owner::observe_event` rechecks current durable ownership under the account/key
  gate and commits the observation with its paired local value. It returns
  `Observed { before, after, changed }` after persistence. It does not rebase an
  editor or mutate a composer.
- A delete without a client UUID resolves only from the exact scoped,
  acknowledged server draft ID under that gate. Pending creates and the visible
  destination cannot supply the UUID. Already recorded duplicate/out-of-order
  retirements can be recognized as inert without deriving fresh identity.
- `crates/super-platinum-core/src/drafts/sync/observation.rs` orders events per
  owned server identity. Event timestamps require six fractional digits and are
  compared only with event timestamps; payload revisions use their own ordering.
  Contradictory equal-clock events and unsupported owned evidence block writes.
  A delete event never confirms a Send.
- `crates/super-platinum-core/src/drafts/store/sync.rs` writes format 5 and reads
  formats 1 through 4. Event evidence, ordering, read-check tickets and local
  content share the existing SQLite transaction. Older binaries reject format 5
  rather than discard guards.
- `src/desktop/drafts/checkpoint.rs` already serializes local checkpoint,
  consumption, release and explicit settlement. A submitted edit can arrive
  while a storage await is in progress. `rebased` accepts only a changed storage
  revision whose paired local content and semantic revision are unchanged.
- `src/desktop/drafts/integration.rs::poll` updates recovery metadata. It
  deliberately does not replace the DraftBook or visible composer.

## Implemented editor slice: retained design contract

Add `Editor::observe_event` in a focused
`src/desktop/drafts/checkpoint_observation.rs` module. Keep realtime ingress and
native DOM replacement out of this slice. Test it with isolated account storage
and deterministic injected observation waits.

### Required invariants

1. A remote observation never silently rebases input that was composed against
   another local value.
2. Input submitted before observation is durably checkpointed first. Input
   submitted during or after an observation handoff remains recoverable.
3. Updating a storage revision does not prove the native field has accepted the
   new remote content.
4. Persisted remote content and competing local input remain separately owned;
   no merge or overwrite is inferred from matching destination alone.
5. The operation survives cancellation of its foreground waiter. Its result
   belongs to the captured account/key, not a later active visit.
6. No network request, enrollment, automatic retry or send is introduced.

### Algorithm

1. Start a retained runtime task, following `Editor::consume` and
   `Editor::settle`, so dropping a component waiter cannot interrupt handoff.
2. Flush earlier submitted input, acquire `Editor::gate`, then recheck whether a
   run began between flush and acquisition. If so, release the gate and retry
   the local serialization step. Never await a run while holding its gate.
3. Reject released ownership, an existing settlement or an unresolved
   observation handoff. Under the short state mutex, capture the editor serial,
   authoritative base, semantic local revision and writer identity. Mark the
   observation active for polling. Do not hold that mutex across storage awaits.
4. Call `Owner::observe_event` while retaining the editor gate. `submit` may
   replace the pending value and increment its serial, but its run task waits
   on the same gate.
5. Compare the returned `before` view with the captured editor base. Require the
   paired local value and semantic local revision to agree. Storage revision
   alone is insufficient: another owner may have committed during the wait.
6. Complete one of these handoffs before releasing the gate:

   | Result | Editor treatment |
   | --- | --- |
   | Metadata-only change with matching predecessor | Advance storage metadata; preserve pending input and serial so it can checkpoint against unchanged content. |
   | Paired local content/revision changed, or predecessor mismatch | Install a projection barrier. Preserve the durable result as metadata, but park pending and subsequent input as writer-owned alternates. |
   | Remote conflict with unchanged local value | Retain both versions and advance metadata. Local input may continue; the reducer conflict still prevents consumption and dispatch. |
   | Storage failure or uncertain handoff | Retain pending input and park authoritative writes. Do not assume the older editor base remains authoritative. |

7. Notify waiters after finishing the handoff. A returned projection proposal is
   not authorization to clear the barrier.

The projection barrier should contain an immutable proposal identity, exact
owner/key, local predecessor, resulting storage revision and captured editor
serial. Polls, reloads and later observations must not clear it implicitly.
Subsequent input remains a durable alternate until an explicit projection receipt
or reviewed conflict decision resolves the handoff. Across restart, canonical
storage and committed alternatives remain authoritative; the previous DOM no
longer exists.

Checking only whether the serial changed during SQLite I/O is insufficient.
Even with no submission during the await, the DOM still displays the old draft
until projection. The user's next keystroke must not overwrite the newly
installed remote base.

Do not copy `checkpoint_settlement.rs`'s successful reset indiscriminately.
Explicit settlement authorizes a reviewed version choice. An unsolicited remote
event does not.

### Files and regression checks

- Add `checkpoint_observation.rs` and `checkpoint_observation_tests.rs`.
- Wire the module from `checkpoint.rs`; add only the needed state/snapshot
  fields and observation activity/proposal identity in `checkpoint_poll.rs`.
- Reuse or extract `Editor::run`'s writer-owned alternate preservation rather
  than duplicating its persistence loop.
- Keep `integration::poll` metadata-only. Preserve the existing cheap unchanged
  signature comparison.

Use channel/barrier-controlled futures rather than sleeps to verify:

- Input pending before observation commits first; a differing remote value
  becomes a conflict rather than replacing it.
- Input during the storage await remains pending and then durable as an
  alternate, with remote canonical content intact.
- Input after observation returns but before projection acknowledgment is also
  parked. Test this separately from the in-flight submission race.
- Multiple submissions retain the latest value for that writer without
  replacing another writer's version.
- Metadata-only observations permit later normal checkpointing without a false
  content conflict.
- A different owner changing the paired predecessor prevents silent rebasing.
- Foreground cancellation/account navigation neither cancels the durable
  transition nor redirects its result.
- Storage failure retains input and blocks subsequent authoritative writes.
- Consume, release and explicit settlement cannot cross an unresolved handoff.
- Restart preserves canonical remote content and committed local alternatives.
- Polling emits barrier changes and does not repeatedly clone unchanged payloads.

Run the locked desktop suite and formatting checks for the editor change. If the
implementation changes core persistence/reducers, also run the independently
locked core suite. Report actual results; the listed checks are requirements,
not evidence that the planned slice has passed.

## Later gate: native projection receipt

Implement this separately before connecting unsolicited events to visible
composers. A proposal needs exact field scope/visit, unchanged native wire text,
selection and composition checks, followed by a typed receipt. Rust signal
equality alone cannot establish that native input is unchanged.

If the native field contains newer text, recapture that text into the editor's
preservation path. Active composition leaves the field untouched and postpones
replacement. Only a matching successful receipt, unchanged editor serial and
unchanged storage revision may authorize clearing the projection barrier.
`view/scheduled/composer_clear.rs` demonstrates a typed native receipt boundary;
the fire-and-forget reviewed-field replacement is not sufficient authorization
for unsolicited remote content.

Add normal/narrow isolated fixtures covering composition, a native edit ahead of
Rust state, navigation/remount, channel/thread separation and restart. Inspect
every captured PNG. Synthetic composition does not establish actual OS IME
behavior.

## Later gate: realtime ingress and reconciliation

Keep the captured `Source` with its accepted socket generation. Resolve only
existing ownership through an account-scoped server-ID/client-UUID index, then
route to the owning editor or the appropriate offscreen persistence path. Do
not depend on `active_team` or infer ownership from a current composer.

An observation deferred because alternatives or a projection handoff block the
editor must not be silently considered processed. Retain its ordering for
reconciliation or mark reconciliation necessary. Do not coalesce away a
delete-before-create sequence. Define bounded ingress overflow behavior before
enabling the bridge.

Production autosave remains a further gate: shared managed Send/Schedule
ownership, ordinary mutation/readback coverage, persisted cooldowns, explicit
remote-conflict recovery and supported-to-unsupported edit handling are still
required. `client_last_updated_ts` is not a server compare-and-swap guarantee.


## Ordinary positive readback contract recovered (2026-10-14)

Read-only inspection of the retained primary bundles identified a direct
ordinary draft lookup. Bundle aliases/hashes are the same as the source table in
[slack-external-select-protocol.md](slack-external-select-protocol.md); offsets
count decoded JavaScript characters.

- B `UYdH` module at 6816000 invokes `s.oV({draftId:v,
  reason:'resolve_draft_conflict'})` at 6818245 and consumes `response.draft`.
- V `zZt+` at 9756496, with binding at 9756598, resolves that call to
  `drafts.info`.
- S `uGgu` at 1594178 removes instrumentation fields including `reason`, abort
  and timeout before S `LYxw.VA` at 738282 converts remaining argument keys to
  snake case. The proven request field is `draft_id`; reason is not sent.
- B `h/H5` starts at 6792414; active payload fetch at 6794008 uses
  `drafts.list`, default `is_active:true`, limit 25 and optional
  `last_updated_ts`/`next_ts`. Recursion uses the last draft's update timestamp
  and caps payload loading at 200. Its separate active-ID loop at 6794981 uses a
  wrapper absent from the retained bundles, so its endpoint/completeness are
  not established.
- S `Bxxi` at 3724536 handles `is_from_composer` and `client_msg_id` in unified
  ordinary/channel draft flows. Earlier worklog language that active listing
  excluded unscheduled drafts is not an established API contract; the scheduled
  UI's filtering does not prove that exclusion.

Only positive exact-identity readback is supported by this evidence. Missing
responses, partial listings and the 200-item cap cannot prove deletion. The
existing conservative decoder treats `is_from_composer` as unknown; its exact
shape still requires review before broadening support. No production ingress,
new storage format or remote writes were enabled by this audit. The next core
request/decoder implementation and reconciliation lifecycle require their own
tests and ownership review. Update timestamps remain metadata, not server CAS.
