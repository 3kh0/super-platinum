# Super Platinum: resume on the Windows laptop

## Windows continuation, 2026-10-14

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

### Superseded execution snapshot (historical)

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

The following snapshot predates the current gates above. Its pending/failure
statements describe that earlier point, not current acceptance.

**Current source:** durable Slash journal (**14 writes**) and Lists rename
(**37 targets; 18 core / 18 desktop new tests**) are integrated exactly. The
original inert Slash core retains its reviewed hooks and no runtime activation.
Root/core formatting and **10 installed Lists Python checks** pass; an earlier
wrong-filename zero-test invocation is retained and provides no validation.
Enrollment status fix457157... is installed with **7 actual-JS checks passing**,
without CSS/shared-toast changes or weakened hit guards. Runner failure fix689580...
is now installed exactly with backups/receipt; **27 installed enrollment Python
tests pass in 5.545s** (prior 24 plus three new tests).

The pre-atomic cohort passes core **1,713 / 5 + doctest** and desktop **1,667 / 7**.
Protected Slash atomic296a692b... is now integrated across **10 targets (3 new /
7 hooks)**, with all 21 package hashes/bases/absences verified, backups/receipt and
root formatting passing. Core now passes **1,729 / 5 ignored + one doctest**
(38.43s compile, 23.29s tests, 0.23s doctest), including all **16 new tests**.
Combined desktop now passes **1,670 / 7 ignored** (3m49s compile, 7.10s tests,
doctests0), including all three paint regressions. Native076ce07d remains
pre-atomic evidence. Cargo finishes the feature build in **4m44s**, but the
PowerShell wrapper reports exit1/NativeCommandError; an explicit repeat confirms
**exit0 in 0.91s**. Both logs remain. Lists lr4 passes at both sizes on
immutable988abc4f and is independently **CLEAR: all 16 PNGs**, eight stages per
size, exact Delivery focus ring/inline icon and no clipping/obstruction. Each
process has **8 exact requests (7 reads / 1 synthetic mutation)**, no replay,
stale/lost-ACK checks unapplied and fresh check applied. Only name changes;
unknown fields, full records and opaque cursor survive. Hash/process/PNG checks,
47/46 restored pointer checks and cleanup pass. This is disconnected acceptance;
controlled cleanup code1 is not crash proof. Preserve lr1-lr3.

Enrollment pe2 is independently **CLEAR: all 12 cases / 80 PNGs**, full SQLite,
closed logs/readiness, unique process times, exact hash and restored pointer
contracts at both sizes. Normal audit includes 219 pointer and 80 capture checks.
Positive/canceled enqueue exactly once; late typing/owner change/failed Editor
zero times. No native clear; newer failed input survives; canceled completion is
not current and explicit reload does not replay. Panels/sections are readable.
This is isolated local synthetic input, not restart/live/global HTTP evidence.
After the user supplied a quiet window, Lists lr2 normal/lr3 narrow both fail
the final focus-return assertion on076ce07d. Each has seven admitted-stage PNGs
plus one diagnostic (16 total), with no completed-matrix acceptance. The normal
failure shows the correct title and closed overlay but no header focus ring.
All 16 PNGs are independently read; binary/process and 76/74 restored pointer
checks pass, with no other layout blocker. Failed terminals omit the raw eight-
request ledger, so exact wire proof is pending. Focus and a minor header-icon wrap
correction is now integrated across seven targets, with **12 actual-source Node
tests passing (78ms)** and formatting passing. Strict native final assertion is
unchanged; failure diagnostics/ledger are expanded. No Lists acceptance. Preserve
lr1-lr3. Those failed captures and the passing lr4 run are closed. Combined desktop
passes in `focus-paint-protected-slash-desktop-full.log`; confirmed feature build
988abc4f records lr4 in `list-column-rename-native-run4.log`. Lifecycle A-C is now
installed across eight targets, with formatting passing. Its first focused run
matched **zero tests (1,677 filtered)** due to copied old mtimes, so provides no
coverage. Root updated all eight mtimes without byte changes; the actual rebuilt
focused gate passes **20 tests / 1,677 filtered** (3m28s compile, 6.63s tests).
Full desktop passes **1,690 / 7 ignored** (8.29s tests, docs0); formatting passes.
Core **1,729 / 5 + doctest** is unchanged; native988abc4f remains pre-A-C.

**Current native evidence:** the corrected feature build passes in **4m12s**.
On immutable `0b5c379a`, Canvas cr2 recovery is independently CLEAR at both sizes:
**4 processes / 16 PNGs**, exact pending SQLite contents through seed/restart and
actual database inspection, U1 scope, 56 pointer checks and zero restart work.
Seed counts are 2 dispatch / 3 reserve / 2 retire. Controlled exit 1 is not crash
or live evidence. Canvas cu2 on `15d19670` has **22 independently inspected PNGs**,
full hash/process/closed-log proof and 107 pointer checks; raw requests/ACKs remain
source-asserted. Lists lh1 on `0b5c379a` is independently CLEAR with **14 PNGs**,
three exact builders, unchanged opaque cursor, stale exclusion, retained omitted/
unrelated records, zero mutations and 83 pointer checks. This is a bounded
synthetic-response workflow, not live HTTP or global pagination proof.

Protected scenarios remain accepted across pd3/pd4 rather than a fresh whole
matrix. Earlier `e4c09596` retains emoji en2's **32 audited PNGs** and complete
ledger/hash/process/pointer evidence; its renderer frame reductions use the same
1 MiB PE stack. All prior failures remain preserved under their original binaries.

**Performance:** the quiet 9284265e-to-e4c09596 comparison is independently
complete: **432 rows, 4,320 samples, 12,960 durations and 72 newly inspected PNGs**.
All **84 Person accepted-result median/p95 cases improve**; 64k @alex changes
from **70.5/80.3 to 29.9/35.7 ms** normal and **70.7/84.8 to 28.9/35.9 ms** narrow.
All 51 raw-increase rows remain reported (41 at least 0.05 ms, 10 float residuals).
This is descriptive whole-binary evidence; pooled p95 is the maximum of 15 samples,
not physical-paint or causal proof. Baseline PNGs retain prior visual audits and
current hashes/dimensions, without proof of byte identity since prior inspection.
See `tmp/completion-quiet-report-prep/quiet-summary.txt` for two-RAF results,
full hashes, regressions and limits.

**Emoji lifecycle baseline:** all values independently recomputed. At 64k,
median/p95 ms are capture **174.2121/187.7039**, replacement **207.0996/224.3218**,
old drop **133.0172/152.3163**, replacement drop **177.7622/197.8818**. Full clone
runs under state.write; build/sort is already off-thread. This is synthetic
component wall time, not live frames, CPU, memory or improvement. Report:
`tmp/emoji-lifecycle-report/report.json`. Startup cs4/cs5 is independently CLEAR
(two PNGs): first-paint/conversation **803.4081/803.4584 ms** control versus
**670.3938/670.4407 ms** candidate. Single-process, probe-enabled disconnected
two-RAF observations establish no speedup/causality/idle/live/stall claim. Legacy
receipts lack cleanupErrors[]; only process closure/pointer restoration is proved.

Protected Slash completion5f712cc... is now integrated exactly: **nine targets
(4 new / 5 hooks)**, all 20 package hashes/bases/absences/patch/format checks pass,
with backups/receipt and fresh write mtimes. Core full **fails: 1,748 / 1 / 5**
(56.23s compile, 23.33s tests): acknowledged_remote_base_is_preserved_and_delete_stays_guarded
gets ErrStorage at f.claim().unwrap, completion_tests.rs:22, **before c.complete**.
Installation and 19 other new tests pass, not the full gate. First fixture-fix
focused rerun **fails 0 / 1 / 1,753 filtered** (23.42s compile, 0.02s test), line399:
actual basis is **Unverified**, not expected Local. The earlier Local explanation
was wrong/incomplete: successor logic invalidates basis on acknowledged identity
change, setting **Unverified / epoch0**, so a new positive baseline is required.
Both failure logs remain. Test-only v2 is installed exactly (52bdcfe7...), with
reviewed full-record/successor epoch0-Unverified expectation and guard message.
No production guard change. Corrected combined core now passes **1,755 / 5
ignored + one doctest** (42.34s compile, 23.17s tests, 0.21s doctest): all20
completion and six thin tests (3 new parity / 3 frozen oracle) pass. Both earlier
failures remain preserved. It
remains natively unwired: source-reviewed exact local ACK clearing preserves
remote/newer/unknown state and guarded binding. Later rejected local-only archival
is distinct from NotSent with a separate review receipt. No native process is active.

**Next:** finish the independent matched-benchmark report audit and record its
final hash before considering the unpromoted desktop projection. Combined desktop passes
**1,690 / 7 ignored** (5m45s compile, 8.48s tests, docs0). The explicit locked dev
benchmark build passes exit0 in1.18s. All six matched processes complete exit0 at
`tmp/emoji-thin-lifecycle-measured-1`: **264 samples /3,168 phase values /24 warmups**.
Independent audit/final report SHA remain pending; component timings are
preliminary, not accepted native/frame gains. Preserve the original failure. Implement D
only in tmp from the reviewed plan, without factory/selector/
autosave/realtime activation. Emoji lifecycle baseline is audited CLEAR: 132 samples / 1,584 phase values,
12 warmups / 48 rows. Thin package719ba74a... has six core/benchmark targets installed (1 replacement /
5 new), with reviewed exact pins/bases/artifacts, fresh mtimes and formatting.
The seventh desktop target is unpromoted pending measurement; canonical workspace
state is unchanged. Combined core passes all26 added tests and full desktop
passes. Matched benchmark runs are complete, report audit pending; no native claim.
D shared23/fixture13 are separately source-CLEAR, checker ongoing. Lists view
d829c... (31 targets /21 tests) remains staged under review. Native988abc4f remains pre-A-C. Preserve pe1 and lr1-lr3
failures and their source/binary attribution.
Broader live, crash/OS-input and global-pagination coverage remains outstanding.
The two-target paint99dd... candidate is integrated exactly after current-base
review and nine source audit checks, with backups/receipt and formatting passing.
All three actual VDom paint regressions pass in the combined desktop gate;
no startup/CPU/memory gain is claimed. The confirmed feature build includes the
seven-target focus/header fix; lr4 native validation is pending. Integration receipt:
`tmp/list-rename-focus-return-prep/root-integration-receipt.json`.
Protected production selection, ordinary autosave and realtime ingress remain
off; the fixture feature alone does not activate storage. Emoji/direct Person
wiring is active; cached index is not.

Current focus/paint/atomic feature binary:
`tmp/completion-baseline-binaries/native-focus-paint-protected-slash.exe`, SHA
`988abc4fdfd3503e4526406e8919f26a112e8462cb5ae5e8f9bc15e924a5c70a`.
Earlier fixed enrollment/Lists binary:
`tmp/completion-baseline-binaries/native-list-rename-enrollment-fixed.exe`, SHA
`076ce07d194fc40c5bee2e6393e2bca7af5ab831c25c16dadf785575660b79fa`.
Protected Slash atomic296a692b... is integrated with core validation passing;
receipt: `tmp/slash-protected-intent-prep/root-integration-receipt.json`.
Lifecycle A-C focused/full gates pass after G0, without production activation;
D remains tmp-only and no new native build exists.
Earlier enrollment feature binary:
`tmp/completion-baseline-binaries/native-enrollment-canvas-fixture.exe`, SHA
`77bd9abc08331fb74766138a1eff83f3b339645c45b8c01e1664da8e58958d94`.
Feature-on refusal receipt: `tmp/protected-enrollment-negative-on1/receipt.json`.
Current default Canvas-controls binary:
`tmp/completion-baseline-binaries/native-enrollment-canvas-default.exe`, SHA
`29851c547fa8a506681fb413423be82d00e54d298d296a66fe88badb643a2b88`.
Earlier Canvas local-review binary:
`tmp/completion-baseline-binaries/native-canvas-local-review.exe`, SHA
`91fb820df3159b37a5a3b7edb04db3a507c6260230fe247edc6b0babbb2886e8`.
Earlier Canvas recovery/Lists native artifact:
`tmp/completion-baseline-binaries/native-canvas-hydration-recovery-fixed.exe`, SHA
`0b5c379a9d21cedb5d1a417b7cb6b10fe4b096f392978a461b37b46fc9a34f75`.
The last allocator edit is test-only, so native checks use unchanged production code.
Default refusal-probe binary: `native-canvas-hydration-default.exe`, SHA
`d631dda62ee6bda105d381203708697b6dc078e9dbdc62630407adbe366d869c`.
Receipt: `tmp/protected-native-negative-default/receipt.json`.
See the [dated execution ledger](parity-worklog.md) for exact source manifests,
report hashes, build logs, frame measurements, failure history and prior receipts.
The historical evidence below remains attributed to its original source/binary.

### Earlier source and native evidence

Prior source revision gates remain core **1,572 / 5 ignored** (21.50s), one
compile-fail doctest (0.18s), and desktop **1,527 / 7 ignored** (5m12s compile,
6.26s tests); these do not validate the current cohort. Its earlier20 Canvas
structural/API tests pass after reviewed normalization limited to known identity
tags1/6/7, retaining exact unknown bytes/order. Preserve all earlier failure logs.
The prior index example build passed0.80s and main build5m16s.
Main SHA `e3d70217d2d5ac60fd487cf66bb9a9197087f55cbe6e7ac9c7a6499e87d66a29`
is retained as `tmp/completion-baseline-binaries/native-schedule-reviewed.exe`.
Benchmark SHA `4933499169c1953595d3591955e97dacede895bb6daefd8e852842af502738ee`
is retained as `tmp/completion-baseline-binaries/index-candidate.exe`.
The pure checked/index/scan report is complete and independently audited by
completion_performance: `tmp/completion-report-prep/index-report.json`, SHA
`974f1c2385040463d4e0bac05ddebfd421a78acddbe4aee55cca7465cc999c2f`.
All42 Person cases improve median/p95; all ten 64k channel cases regress, so
channel triggers must bypass the index. This cached index remains unwired; the
separate direct Person path above is now installed for the next build.

The ten-target Schedule retained-host / fixture-ownership / stale-retirement fix
is integrated and matches reviewed candidates; 40 Python and eight actual-JS
checks plus root formatting and the full desktop gate pass. Final `ms9/` native
matrix passes all22 normal/narrow scenario/restart pairs / 44 closed processes
on e3d70217. All154 PNGs are independently read: normal first6 42, normal last5 35,
narrow first6 42, narrow last5 35. Exact full SQLite and pointer/process contracts
pass; total prepared exercise requests are20, restart requests zero. This is
disconnected synthetic DOM/HTTP and controlled restart evidence, not live Slack,
trusted OS input or crash durability. The production storage selector remains unchanged; the new protected cohort is not activated.
Earlier setup/cleanup checks retain ten passing Python tests. Earlier native evidence
belongs to the 5m23s build, SHA
`9284265ea21390955089707acca1de6944187a012bd48ded1ad65f34344f4680`,
preserved as `tmp/completion-baseline-binaries/native-before-setup.exe`.

On this binary, `cb4/` 64k normal/narrow runs pass independent audit: five rounds,
240 samples, eight inspected PNGs, 164 pointer samples and `cleanupErrors: []`.
@alex DOM / next-RAF medians are **70.5 / 73.7ms** normal and **74.2 / 77.4ms**
narrow. No optimization is active; do not pool this new setup binary with the
older 2k/10k cohort; the completed same-binary aggregate is recorded below.

`ms3/` narrow rich-channel/thread pass two restart pairs / four processes with
14 PNGs, independently audited clear: exact SQLite inventories, one drafts.update
per exercise, zero restart requests and actual notice-dismissal UI proof.
`ms4/` normal rich-channel/thread pairs also pass independent audit with 14 PNGs.
Its retry-channel failure has four inspected PNGs and an audited real transient
UI lifetime bug: the conditional RejectedSchedule child unmounts on consumption,
dropping the dialog before HTTP acknowledgment. The retained host fix is now integrated;
its corrected ms9 native gate now passes; the original failed artifacts remain.

`ms5/` newer-channel/thread normal pairs pass independent audit with 14 PNGs,
exact SQLite inventories, one request per exercise, zero restart requests and
143 pointer samples. Its stale-owner case fails (four PNGs, all read): the footer
correctly retires stale context, but the fixture expects the stale modal to remain.
`ms6/` lost-ACK fails with six PNGs, all read by review_layout. The fixture-only
unconditional bypass in integration::legacy_allowed permits the legacy modal;
the production managed-owner gate already rejects it. The narrow fixture correction
is now integrated; failed runs do not establish a complete request ledger.
`ms7/` remains overall FAIL, with all six PNGs read by review_layout. Its complete
held exercise confirms one prepared request with pending delivery false; restart
fails the same fixture legacy bypass, so no restart request ledger is claimed.
Independent SQLite evidence retains thread InFlight to Uncertain, storage revision
4 to 5, local revision 2, exact newer/frozen content and unchanged sibling.
`ms8/` narrow newer-channel/thread passes independent audit: two restart pairs /
four processes / 14 inspected PNGs, one request per exercise, zero restart requests
and exact SQLite inventories.

The native completion descriptive baseline is now complete on immutable9284265e.
cb7 independent acceptance audit is CLEAR: all24 PNGs read,720 exact samples,
six closed processes,349 pointer checkpoints and empty cleanupErrors. Combined
cb4+cb5+cb6+cb7 totals **18 unique processes / 72 inspected PNGs / 2,160 samples**,
three processes per fixture and1,056 pointer samples. Aggregate
`tmp/completion-report-prep/cb4567-report.json` passes, SHA
`5f361af20f081edf0c9a25685198dca7b2aae88ddde7222039a76898b0b8eb66`.
This is a descriptive synthetic baseline, not optimization, statistical significance
or physical-paint proof. Startup cs1/cs2/cs3 are independently audited CLEAR by read_preferences, with all
three PNGs read and exact closed log/process/hash/pointer/progress proof. First
paint is673.6556/798.1069/659.7641ms; conversation is673.7418/798.1878/659.8732ms.
These are probe-on, focused20s observations only, not idle or live-stall-fix proof.
Guarded idle ci2/ci3/ci5 is independently audited CLEAR: all three PNGs read,
21 process identities,255 observer records,2,295 hits and pointer restoration.
On immutable9284265e with5s settle / 15s quiet, total CPU median is0.078125s
(range0.015625-0.125); private commit median329,580,544 bytes
(range327,311,360-336,060,416); shared-inclusive working set median550,215,680
(range548,421,632-556,474,368); quiet duration median15.0014575s. Observer queries
may add implicit hit-test overhead; private commit is not RSS, shared pages can
be counted twice, and coverage is sampled/non-atomic. No improvement is claimed.
ci1 remains failed/inconclusive due incomplete counters and overlay. ci4 correctly
refuses a foreign bottom obstruction before quiet sampling, with no PNG/counters.
The six-file collector is promoted exactly and25 installed mocked tests pass;
this does not establish that the historical denied-PID root cause is fixed.
Refreshed pure before2 is independently audited CLEAR:
five processes / 4,320 samples, full ranking equality, empty stderr and unchanged
3ac5794f. Keep pure-model and native measurements separate.

Cached-index runtime wiring is blocked on recurring capture ownership, documented in
`tmp/completion-runtime-prep/refresh-boundary-audit.json`. At64k, cohort medians
span capture20.2959-34.9739ms (max46.2554), build63.0575-72.4727ms (max95.3533),
publication scan7.7808-9.3001ms (max12.7337), replacement retirement18.9994-21.8497ms
(max26.9447). These costs must not move onto the serial UI path. Boot/persistence
alone cannot refresh unsaved canonical changes such as UserChanged, search
hydration or remote Person adoption; exact checks can otherwise fall back forever.
Next: finish native emoji/protected behavior verification and the feature-off
requested-fixture refusal check, then measure paired direct Person latency. A future
cached-index path still needs a complete independently owned recurring metadata
source with eventual-refresh and retirement/cancellation proof. No native
optimized improvement, physical-paint or full parity claim follows. Slack CDP
last responded with one client renderer; no restart was needed at that check.
Protected production selection, autosave and realtime ingress remain off.
Preserve all artifacts.

Earlier `497d9587` retains sr3's four audited typing-recovery cases / 20 PNGs,
settled managed/Editor revision 3 before read and revision 4 afterward, exact SQLite,
four reads / zero mutations and 183 pointer samples. cb1 plus four cb2 2k/10k runs
retain five audited closed receipts / 20 PNGs / 504 queries / 209 pointer samples.
DOM / next-RAF medians remain 2k 8.8/11.6ms normal, 8.6/12.1ms narrow; 10k
17.7/22.7ms normal, 16.1/19.1ms narrow. Preserve cb2/cb3 premeasurement 64k failures
and cb3 cleanup traceback, plus the ms2 toast obstruction. No optimized runtime,
physical-paint improvement or broad parity claim follows.

**Earlier acknowledged-Send source/native gate:** independently locked core **1,418 passed / 5 ignored**
and desktop **1,468 passed / 7 ignored**, both formatting and whitespace checks.
Focused drafts pass **272 / 1 ignored**. The final checker passes **28 Python
receipt tests** and **11 extracted actual-JavaScript tests**. Locked dev build
passes in **3m31s**, SHA:
`cd881dcc690e54e9fa799ab1d18f4e64648c9a800657e7e00e4274ffbb4352d9`.
Acknowledged supported-text Send passes all **12 normal/narrow scenario/restart
pairs**, **24 owned processes**, and **all 72 PNGs inspected** in `tmp/agent-ui/as2/`.
The exact prepared request count is twelve, with zero requests on restart.
Closed logs, unchanged executable hashes, process identities, 713 pointer samples
and restoration pass; all twelve SQLite databases independently match exact
restart inventories. Newer Unicode input/selection/focus survives; acknowledgement
releases managed ownership, while lost/held responses remain uncertain without
replay. This is synthetic DOM/HTTP evidence and controlled termination, not live
Slack, OS IME or crash durability. The pre-Send rounded-hit false failure on
`70998090` remains in `as1/`, with its sole failure PNG inspected.

At that earlier gate, source included ordinary observation flags, pure rich Schedule helper, known-ID
Schedule readback, whole-inventory Editor/Owner handoff and pre-connect socket
provenance. Managed Schedule UI was not yet integrated; production draft ingress
remains inactive.
The subsequent Schedule recovery UI (19 tests), rejected-Schedule retry/replace
helpers (15 tests) and neutral metadata allowance (9 tests) are now integrated
and actual-review clear. The separately reviewed continuity candidate is installed
only behind cfg(test), with no production v6/ingress activation. Their combined
independently locked core suite passes **1,447 / 5 ignored**, including all18
continuity regressions. The desktop focused gate passes **304 / 1 ignored** and
the full suite passes **1,500 / 7 ignored** (5.79s). Root/core formatting and
whitespace checks pass. The initial RSX compile error was corrected by wrapping
the conditional label expression in braces. That historical gate preceded the later
build/native evidence summarized above. After the reboot, Slack CDP on port9222 is reachable with its client renderer;
no Slack restart was needed.

The reviewed raw-rich Schedule creation/rejection UI (17 targets, eleven tests)
and Schedule recovery native harness (18 targets, three Rust tests) are now
integrated. Actual integration reviews are clear; promoted receipt tests pass19.
The initial focused Rust compile found a private helper called by the recovery
fixture's successor interleaving. Its reviewed correction uses public Editor
submit/flush through a local Queue, leaving shell writer identity unchanged.
Focused tests now pass **318 / 1 ignored**, full desktop **1,514 / 7 ignored**,
formatting and whitespace checks pass. The locked Windows dev build passes in
5m49s, SHA `068eae94f4d2257a46b611744f77964890d96df2e4e1fd45e53b03135dca845b`.
Schedule recovery positive-channel passes both sizes with all14 PNGs inspected,
exact SQLite/closed-process/hash evidence and133 pointer samples. The subsequent
positive-thread run stops after its result capture on a timeline-anchor remount
check; all4 failure-run PNGs are inspected. All24 channel cases now pass in
`tmp/agent-ui/sr1/`, with all120 successful PNGs inspected and28 owned processes.
The remount correction and creation/rejection native fixtures are now integrated;
recovery22/creation24 Python checks and both10-case JS suites pass. Initial Rust
compile exposed fixture-only private API calls, now corrected using Queue and
a fixture-specific isolation predicate. The corrected focused suite passes
**322 / 1 ignored** and the full desktop suite passes **1,518 / 7 ignored**
(5.71s); formatting and whitespace checks pass. The locked dev build passes in
4m38s, SHA `7796e904b3acb259dd18e328b3569a60b25e7c81c035377f96e7cb3b04870c9a`.
The recovery sweep in `sr2/` has31 passing cases with all153 successful PNGs
independently inspected. The narrow typing-commit case fails its intermediate
publication check; all six failure PNGs are inspected, and final durable/newer
text is intact. Its reviewed fixture publication barrier is now integrated,
without weakening the receipt validator. Creation pilot `ms1/` fails because
the dialog is positioned against the composer instead of the viewport; both
PNGs are inspected and the bounded modal/body-scroll correction is now integrated.
No full new Schedule matrix claim yet. Slack CDP remains reachable
with its client renderer after the reboot notification; no restart was needed.
Continuity still
needs complete production adapters/source/queue proof and usable explicit unknown
read/archive recovery. Preserve the full parity/design/performance matrix.

The reviewed14-target native completion baseline is integrated and test-compiled;
its measurements will be warmed-directory synthetic input latency, not physical
paint. The independently reviewed v7 protected-recovery candidate is installed
only behind cfg(test), with33 passing tests and no production backend change.
Initial core compilation found an unsupported SQLite u64 read; it now reads i64
and checks conversion, rejecting negatives. The corrected independently locked
core suite passes **1,480 / 5 ignored** (41.60s compile,22.83s tests), including
all33 new regressions. Full desktop tests for the completion instrumentation and
publication barrier pass **1,523 / 7 ignored** (6.64s tests). The reviewed seven-
target Schedule dialog/layout-check correction is now integrated; promoted31
Python and8 actual-JS tests plus formatting pass. The next full desktop suite
passes **1,523 / 7 ignored** (6.11s) in `schedule-layout-desktop-full.log`.
The locked dev build passes in4m43s in `schedule-layout-native-build.log`
(SHA497d9587, full hash above). Keep source frozen.
The four typing cases now pass independently audited sr3 captures on497d9587.
Continue the creation/rejection matrix after the strict toast-dismissal fixture
correction, finish the independent ms2 audit, and complete/audit cb2. Preserve
sr2/ms1/ms2 failure evidence; do not infer a full matrix from the passed pilots.

The bounded real Lists probe rejected public `slackLists.items.list` with
`not_allowed_token_type` after the exact private channel/List guards passed.
Four read calls, zero writes/retries; temporary example/helper sources removed.
Internal continuation remains open. The isolated completion baseline now measures
64k person search at57.884ms median/68.685ms p95; exact rankings match five processes.
Preserve `tmp/completion-baseline-binaries/before.exe` and the raw receipts for
future paired comparison. This is pure model cost, not input/startup speedup.

**Previous Update-recovery Windows build gate:** independently locked core **1,384 passed /
5 ignored**, desktop **1,405 passed / 7 ignored**, formatting/whitespace and
locked dev build **4m37s** pass. Executable SHA:
`da7ebcd098b04f1c704a1e4f17f2975d81a5c6caf6d1dd5da4689cccf0e5daec`.
Update recovery passes all twelve normal/narrow scenario/restart pairs in
`ur-final4/` (normal) and `ur-n4/` (narrow), with **all 52 successful PNGs inspected**.
Exact durable restart and closed process/SHA/pointer receipts pass. The duplicate
cooldown toast is fixed; the prior two-PNG pointer abort is retained separately.
These are synthetic inputs/responses and controlled owned process termination,
not live Slack, OS IME or crash durability. Previous gates retain the hashes below.
The corrected remote-review gate passes all 12 scenario/restart pairs with
**all 112 PNGs inspected** on the prior `6a77d24e` executable. Conversation headers and review
rows are readable across their internal scroll positions; second-choice bodies
and review actions are not simultaneously visible in every capture. Exact durable
restart dictionaries and owned process/pointer receipts pass. Completion evidence
is synthetic geometry, not actual suggestion interaction. The prior clipping
failures are retained in the worklog. Post-reboot capture accepts successful empty
taskbar enumeration while rejecting failed probes; no OS settings changed.
All ten attachment native scenarios pass on `271d07ba`, with all 48 successful
stage PNGs and two retained failure PNGs inspected. Narrow scroll positions show
the rows, status and footer; upper content is not simultaneously visible.
Reading settings passes both sizes on `91009832`, with all twenty PNGs inspected,
including canonical selection, focus preservation and bounded focus rings.
Escape now passes both sizes on `50c8ca39`, with all twenty PNGs inspected.
Shift+Escape now passes both sizes on `13e28b39` in `br-final3/`, with all 28 PNGs
inspected, exact request/Undo/expiry/focus/lifecycle checks passed. Prior status
overflow and incorrect expiry-oracle failures remain preserved. Corrected reading navigation passes both
sizes on `271d07ba` with all twenty PNGs inspected.
User-link evidence uses the prior f0415fe6 executable.

Board lane Add passes both Windows sizes with **all 14 PNGs inspected** in
`tmp/agent-ui/windows-board-add-pilot/`. Exact ordered typed seeds, four actual
requests per run, stale-preflight refusal, pending duplicate suppression and
uncertain no-replay checks pass. Header/status/actions are readable; narrow
cards and lanes use bounded scrolling. The runner independently verifies closed
logs, unchanged executable, owned process exit and pointer restoration. This
is disconnected synthetic evidence, not live create interoperability.

Native draft projection passes **20/20 normal/narrow scenarios**, across 40
processes, with **all 120 PNGs inspected** in
`tmp/agent-ui/windows-draft-projection-verified/`. Exact durable dictionaries
match across restart; newer input stays recoverable, stale mounts/navigation
cannot clear the barrier, and all owned processes exit with observed pointer
restoration. Corrected navigation also has 12 separately inspected pilot PNGs.
The prior artificial-remount blank transcript is resolved. These are isolated
storage and synthetic input/composition checks, not live synchronization, OS IME,
crash recovery or durable backward-caret proof. Production realtime draft ingress
and ordinary remote autosave remain disabled. See
[draft-native-projection.md](draft-native-projection.md).

The prior single-external selector gate passed all four native fixtures with
**38 inspected PNGs**, exact raw/full sibling state, debounce, cooldown and
uncertainty without replay on `df0017fa...`. Checker-isolation/notification
presentation added 18 inspected PNGs. The subsequent regression found a changed
single clear-button accessible label; all 24 failed-run PNGs were inspected and
the original label is restored. On the latest binary all four corrected checks
pass, with all 38 PNGs inspected in `windows-single-external-regression-corrected/`.
That run's narrow loading captures already show results. A subsequent held-loading
gate on `dd5a3aa0...` passes all four checks with all 38 PNGs inspected in
`windows-single-external-held-loading/`. Both narrow loading captions are visible
with no rows; all eight raw requests independently preserve options and full
sibling state. Legacy per-process lifecycle receipts remain incomplete; see the
worklog for exact limits.

**Multi-external native gate complete:** all four final fixtures pass on the
`e3cb05e3...`, with **all 50 PNGs inspected** in
`tmp/agent-ui/windows-multi-external-select-verified/`. The toast now clears the
whole narrow picker; footer hit tests and measured nonoverlap pass. Held loading,
saved-empty reopening, ordered raw selections, full sibling state, modal inline
edits, cooldown and uncertain no-replay checks pass. Narrow modal maximum and
dirty-sibling copy lie below the captured fold; their assertions are not pixel
proof. Fourteen toast JS and fifteen runner tests pass. These are disconnected
fixtures, not live callback interoperability. Root owns every Cargo/build/native
launch.

**Current work:** source-backed conversation reading modes, synchronized Reading
settings, bounded resumable cold unread loading, and renderer scrollmarks are
implemented and independently reviewed. Core/desktop tests and the new dev build pass below;
navigation, scheduled attachment editing and draft-review native gates pass with
all captures inspected. Reading settings focus verification also passes.
Escape native integration passes after the reviewed bridge correction; thirteen
Node regressions and both native sizes pass. Shift+Escape's corrected expiry
receipt expectation and both-size native gate also pass. Production feed/history
reconciliation now passes the disconnected full-pipeline suite; live read
acknowledgements and full workspace-history coverage remain separate gaps.
Known-human DM/profile links pass the prior frozen native gate. Ordinary
bidirectional remote autosave and production draft ingress remain disabled.
Keep the broad remaining matrix open.

The subsequent independently locked core gate passes **1,289 / 5 ignored** in
20.13s after 20.04s compilation. It includes serialized remote review, exact
known-unsent handling, definite-rejection recovery and raw-marker collision
regressions. The subsequent desktop gate passes **1,273 / 7 ignored** in 4.44s
after 41.72s compilation; formatting passes. It covers the remote-review UI,
worker/Editor adapters, isolated fixtures and attachment lifecycle integration.
The locked dev build passes in 4m49s. Ten remote-review runner tests pass, including
the corrected exact request URL/query validation. Attachment disconnected
adapter/fixtures and user-link work are now being integrated for the next build,
while native checks retain the frozen executable. No native outcome is claimed for these changes
yet. The user rebooted Windows; restart Slack with CDP if live reference work
requires it. Real sessions, clipboard and drafts remain untouched in these checks.
Slack CDP9222 is restored on Windows; the client renderer endpoint is verified.

The next independently locked core gate passes **1,294 / 5 ignored** in 19.98s,
compilation 31.92s. Attachment Source/fixtures and legacy compatibility/queue
guards, plus known-human DM-and-profile user links, are independently reviewed
and source-frozen. Desktop now passes **1,296 / 7 ignored** in 4.71s after 2m45s
compilation; formatting and whitespace checks pass. This includes the review
layout correction preserving suggestion visibility; the dev build passes in
4m19s. On that frozen executable, the narrow review pilot fails full-row
readability. Attachment ordered-rich passes at normal size; empty-body-cancel
fails a closed-dialog assertion. Both failures are under investigation. User-link
native checks pass at both sizes with all 16 PNGs inspected; narrow profiles
cover the composer as an overlay, and their Message button is below the fold. Read preferences are now being integrated into later
source, outside this frozen executable. Broad acceptance
remains open; ordinary bidirectional remote autosave is still disabled.

The later read-preference core gate passes **1,322 / 5 ignored**, formatting
passed, compilation 24.71s/tests 20.36s. Desktop settings/reading source reviews
are clear. Final desktop gate passes **1,332 / 7 ignored** (compilation 1m57s, tests
5.06s), formatting and whitespace pass. The earlier stale unread-hold test is
corrected. The locked dev build passes in 4m30s (6a77d24e above). The corrected review
pilot is running; attachment/settings/navigation native gates remain next.

Follow-up fixture/focus and idle-reload corrections pass **1,336 desktop tests /
7 ignored** (compilation 1m50s, tests 4.58s). The locked dev build passes in
3m37s; SHA is recorded above and in `read-followup-final-sha.log`. Core remains
unchanged at 1,322 / 5 ignored. Attachment ten-case native verification is
complete across `windows-scheduled-attachments-final/`, `att-final/`, and
`att-last/`; retained capture-path and pointer-placement failures are detailed
in the worklog. Finish corrected Reading settings/navigation gates next.
Preserve the complete corrected draft-review gate on 6a77d24e. New Escape and
Shift+Escape source is being integrated outside the frozen executable, with
independent core/renderer/confirmation/reconciliation and fixture ownership.
It requires a new compile/test/build gate after the current native runs finish.

Final shortcut review and both formatting checks are clear. Core now passes
**1,353 / 5 ignored** (22.61s compilation, 20.34s tests); desktop passes
**1,377 / 7 ignored** (3m02s compilation, 5.00s tests). The locked dev build passes
in 4m36s in the `read-shortcuts-freeze-*` pipeline, producing `91009832` above.
Reading settings passes both sizes in `rp-final2/`, all twenty PNGs inspected.
First normal Escape and bulk runs fail before a prepared request and confirmation
dialog respectively (`rs-first/`, `br-first/`); all four PNGs are inspected.
Rebuild the reviewed bridge fix and diagnostics, retry both shortcut sizes, and
inspect every PNG. Keep the earlier 271d07ba
attachment/navigation and 6a77d24e draft-review receipts attributed separately.

The next draft prerequisite is active source work outside `91009832`: harden
raw mutation acknowledgments/denials, add explicit positive frozen Update readback,
and persist operation-bound read tickets/deadlines across restart. The reviewed
classifier keeps a raw server `draft_mutation_not_sent_offline` denial dispatched
and uncertain; only trusted transport evidence can establish known-unsent.
Contradictory receipts retain raw uncertainty. Additional unrepresented draft fields
remain conservatively unsupported; full live acknowledgement compatibility is open.
Core format 5 and Update adapter/UI/fixtures need the next integrated test/build gate.
The final independently locked core gate now passes **1,368 / 5 ignored**
(20.65s compilation, 20.51s tests), including fifteen durable-ticket regressions.
Desktop compilation initially found a fixture `Owner` glob-import collision;
its return type is now explicitly `crate::drafts::Owner`. The next locked check
passes in30.50s. A missing RemoteDraft import in a new test is also corrected.
Full desktop tests now pass **1,401 / 7 ignored** (3m08s compilation, 5.18s tests),
formatting passes, and the locked dev build passes in4m29s, producing `50c8ca39`
above. Escape passes in `rs-next/` with all twenty PNGs inspected. Bulk stops at
its first success notice in `br-next/` (six inspected PNGs); a scoped grid fix is
reviewed. Update recovery has three normal pairs passing (positive in `ur-first/`,
cooldown/pending in `ur-tail/`). Typing-before restart shows stale fixture seed
text while exact durable dictionaries remain correct; its helper now uses the
existing real unmount/remount and Latest anchor flow. Owner rotation exercise
passes in `ur-more/`, but restart aborts on pointer placement before readiness.
Rebuild the fixture/CSS corrections and complete all six Update scenarios at both
sizes and both phases, plus bulk at both sizes. Prior Reading settings remains
attributed to `91009832`; keep all failure receipts.

The subsequent renderer/fixture gate passes **1,401 / 7 ignored** again
(1m57s compilation, 4.66s tests), formatting/whitespace and dev build3m28s.
Latest SHA is `13e28b39` above. The complete Update matrix is running in
`ur-final2/`; first five normal pairs pass, including both newer-typing cases.
Independent normal/narrow PNG/closed-receipt audits are assigned. Bulk
`br-final2/` has a native-success receipt with fourteen captures, but remains an
outer-runner failure pending the expiry surface/control expectation correction.

Both corrected bulk runs pass in `br-final3/` with all 28 PNGs inspected and exact
closed-receipt/lifecycle audit. Update `ur-final2/` passes all 12 pairs with all 52
PNGs inspected, but narrow thread overlays hide recovery rows. The scoped fixture
fix closes the thread through the existing checkpointing flow, waits for DOM
absence, and requires 3x3 unobscured samples plus complete row/button bounds.
Sixteen strengthened Python tests pass. The `recovery-visibility-final-*` gate
passes formatting, 1,401 desktop tests / 7 ignored (1m50s compilation, 4.58s tests),
locked dev build 3m26s and whitespace checks. Fresh `ur-final3/` on `151e9a95`
passes all six normal pairs (all 26 PNGs independently inspected) and four narrow
pairs, then fails narrow cooldown: its toast covers the inline row feedback.
The thread overlay is gone; the stronger visibility check correctly rejects the
remaining obstruction. Fix duplicate feedback before a fresh native gate; pending
narrow and cooldown restart remain unrun. Prior native gates retain their original
executable attribution. The pure owned-index helper was excluded from this gate;
module integration is now released for the next compile, with ingress disabled.
Preserve ordinary autosave and realtime ingress as disabled until their full
integration contract is implemented and verified; this classifier slice alone
does not establish draft-sync parity.

The duplicate-feedback correction now passes four new tests within the complete
1,405 / 7 ignored desktop suite (4m09s compilation, 4.90s tests), formatting,
whitespace and dev build 4m37s. It publishes to the originating live row before
suppressing an unconfirmed Update toast; confirmation and unavailable-row cases
retain notification. Full normal `ur-final4/` and narrow `ur-n4/` captures and
independent audits pass, with all 52 successful PNGs inspected. The first narrow
attempt detected pointer movement and correctly skipped restoration; its two
images and failed receipt remain preserved. No guard was disabled for the retry.

The pure durable owned-draft identity index is now integrated and passes all
sixteen new tests within the independently locked core suite: **1,384 passed /
5 ignored**, compilation40.40s, tests20.08s. Desktop compilation also passes. It resolves
acknowledged IDs or pending Create UUIDs within exact account/team scopes, with
full-row and inventory-instance revalidation. Do not infer ownership from the
visible channel. Production ingress still needs retained
socket provenance, ordered/deferred delivery, durable reconciliation-gap guards,
offscreen Editor routing and the existing native projection handshake. Before
enabling ordinary enrollment/autosave, extend shared consumption. Acknowledged
supported-text Send is integrated and passes all seven regressions; legacy
Schedule/upload paths still refuse managed ownership. Keep this dependency explicit.

The reviewed bulk reconciliation execution seam, subscription/cursor authority
correction and eighteen async tests are integrated and pass. Exact terminal
Send/Schedule newer-local preservation and optional completion provenance pass
fourteen new core/store and four desktop regressions. Source gates are 1,398 core
and 1,427 desktop; the preparation archive is `tmp/bulk-reconcile-prep-validated/`.
Acknowledged Send passes the subsequent 1,434-test desktop gate. The later
Schedule handoff, observation alignment and native Send completion are recorded
at the top; retain this paragraph as historical attribution. Ordinary autosave and realtime
ingress remain disabled; Schedule/upload eligibility is not activated.

Trusted keyboard/file-paste regressions pass on `ad6f18f2...`, with all **22 PNGs
inspected**. Normal keyboard captures have taskbar overlap, but edit results are
legible; foreground restoration was requested but not observed. File paste is
synthetic and never reads the OS clipboard. Three normalized before/after idle
runs have inspected PNGs: tree CPU median **0.046875 → 0.0625s per 15s**, private
commit **260.94 → 262.06 MiB**, first paint **577.88 → 626.51ms**. Mixed small
samples establish no general speedup. Exact limitations and failure history are
in the worklog. Continue the broad remaining acceptance matrix after these slices.

Previous complete native checkpoint: **1,203 core tests / 5 ignored**, **1,130 desktop
tests / 7 ignored**, both format checks and whitespace check pass. Twelve
keyboard, seven paste-runner and two selector receipt tests pass. Dev build
**4m09s**, SHA `a631f1ddb36cd81e1413ccc64325b637152a17f918eea85873254489080eae65`.
All **42 latest PNGs** were inspected: Unicode attachment names (6), ordinary/
modal multi-conversation (26), and legacy single-conversation modal (10), in
`windows-file-names-final/`, `windows-modal-origin-final/`, and
`windows-modal-origin-single-regression-final/`. All native assertions pass.
Original names now reach display and upload independently of opaque disk paths.
Bounded modal defaults use the verified viewed conversation and preserve it
across navigation. The later ordinary-default gate is recorded below; broader
missing/filtered/IM origin behavior remains bounded by the protocol document.
Owned draft event ordering/identity/conflicts now persist atomically, including
UUID-less bound deletes and inert retired replays; production realtime and
autosave remain unwired.

**Current new source work (not covered by the preceding gate):** single external
selectors in ordinary messages/modals, and serialized Editor observation with a
barrier before native projection. See
[slack-external-select-protocol.md](slack-external-select-protocol.md) and
[draft-observation-integration.md](draft-observation-integration.md). Root owns
Cargo/build/native launches; these new slices require their own final gates.

The new core slice now passes **1,218 tests / 5 ignored** and its format check.
It covers single external option parsing/routing/state and ordinary boolean
multi-conversation defaults without an invented origin. Desktop tests now pass
**1,155 / 7 ignored**, with formatting and whitespace checks passing. This
includes all eleven Editor observation tests. The narrow external-modal fix
requires the additional native gate described below.

The first full desktop run passed 1,152 tests and failed two external-selector
tests (7 ignored). All eleven Editor observation tests passed. A focused rerun
identified the synthetic workspace's user as the message author `U1` rather
than the fixture's current user `U0`; the fixture is corrected and an explicit
wrong-owner regression retained. The final desktop suite passes; the dev build
passes in **3m58s**, SHA `1c599acd331c92d36419d4f7ee535dbe51a37aa6131e7ca487c76a3c344b5886`.
External selectors passed both ordinary sizes and the normal modal, but narrow
modal Done was clipped behind the footer. Its source fix moves Done beside
search and strengthens hit-testing/focus-return checks. A new test/build gate
is running before rerunning all four external fixtures; do not call that slice
native-complete yet.

Ordinary/modal multi-conversation defaults pass all four fixtures on `1c599ac...`
with all **26 PNGs inspected** in `windows-ordinary-default-final/`. Bookmarks
and recurring notification schedules pass both sizes with four inspected final
PNGs, and the separate folder-removal copy capture confirms the singular noun.
Profile/title/status workflows also pass six fixtures with all six PNGs inspected
in `windows-profile-final/`. These are disconnected request/UI checks, not real
profile/preference mutations. See the worklog for exact counts and boundaries.

Global notification defaults now pass the portable Windows native checks at
both sizes on `a631f1d...`; both PNGs were inspected in
`windows-global-notifications-a631/`. Seven controls are reached, with nine
synthetic writes and twelve reads per size, exact request and no-transport
assertions. No server preferences or OS settings were changed.

An additional static Windows search/navigation sweep on `a631f1d...` passes
21 fixtures. All **22 PNGs** (including one clean replacement) were inspected:
`windows-visual-search-a631/` and
`windows-visual-search-a631-clean/new-dm-selected/`. Search/filter/status,
palette and DM layouts are readable in the visible regions. Six final captures
have taskbar overlap on background client content; central dialogs remain clear.
This is layout evidence, not live search/DM workflow or measured contrast proof.

Trusted Windows keyboard editing and synthetic file-paste checks passed both
sizes with all 22 earlier PNGs inspected in
`windows-{keyboard,file-paste}-window-final/`. Foreground restoration was requested
but not observed; OS IME and the system clipboard are not covered. Compact
multi-channel/multi-conversation triggers passed eight normal/narrow fixtures
with all 50 PNGs inspected in `windows-multi-{channels,conversation}-compact-final/`.
Initial input fixture failures remain retained; full-row ownership checks fixed
the mutable viewport comparison without weakening source-row invariants.

The previous `9089c0f...` build passed normal/narrow multi-channel checks with
all 24 PNGs inspected in `windows-multi-channels-final/`, all four scoped contrast
fixtures with 12 inspected PNGs in `windows-contrast-final/`, and all 16 shared
picker regressions with 46 inspected PNGs in `windows-picker-regression-final/`.
Contrast evidence covers named actual/probe controls, not whole-theme certification.
Rejected-thread restore/restart now passes normal and narrow with all ten PNGs
inspected in `windows-rejected-thread-focused-final/`; earlier notice failures
are resolved for these flows. Original SQLite snapshot/broadcast and unrelated
drafts survive separate processes; no send occurs. Actual later-edit autosave,
OS IME and live/crash fault proof remain open. Canvas text/checklist native
checks now pass at both sizes with all 20 staged/final PNGs inspected in
`windows-canvas-final/`. File-operation recovery also
passes normal/narrow three-process checks with all six PNGs inspected in
`windows-file-recovery-final/`; exact rename data remains unchanged and only
explicit deletion retirement changes its receipt.

The user resumed on native Windows at `37d71bb` (`resync`). The checkout was
clean and all six named capture/Dock transfer artifacts below are tracked and
present. The native Rust host is `x86_64-pc-windows-msvc` (Rust 1.98.1).
Native dev compilation, isolated TCP ping/state, and full DPI-correct screenshot
capture now pass. PowerShell argument parsing and DPI coordinate bugs were fixed.
The first inspected captures also exposed broken Windows media URLs; the desktop
now adapts canonical IDs to Wry's exact synthetic HTTP origins and CSP sources.
Native checks prove ready/pending decode and same-element generation refresh.

Activity cursor reconciliation below is implemented, including revision guards
for channel cursor ABA (forward then backward while an HTTP mark is pending).
Activity Windows gates: **1,101 core tests / 5 ignored**, **959 desktop tests /
6 ignored**, both format checks, whitespace/Python syntax checks, and locked dev
build (**3m14s**). Normal/narrow Activity navigation, cursor/page reconciliation,
and media checks passed; root inspected their PNGs. Receipts are under
`tmp/windows-bootstrap/` and `tmp/agent-ui/windows-{activity,media}-final/`.
The earlier 947-desktop/1,098-core runs preceded final edits and are not final gates.

Ordinary-message `multi_users_select` and overflow are now implemented. Final
logic gates passed **1,112 core / 5 ignored** and **971 desktop / 6 ignored**;
the dev build passed in **3m01s**, followed by a **1m56s** compact-control layout
build. All five control families passed normal/narrow native checks; root inspected
all ten PNGs, then rechecked both corrected multi-user captures. Receipts are in
`tmp/agent-ui/windows-controls-final/`. Three before/after synthetic channel-scroll
runs show a 6.1ms median in both builds; no whole-app improvement is established.

List **Duplicate item** is now implemented through the source-verified create
endpoint, canonical adjacency and readback, with retained delivery locks across
same-user credential/transport rotation. Final gates: **1,118 core / 5 ignored**,
**981 desktop / 6 ignored**, both format checks and dev build **3m03s**. Normal
and narrow List interaction/duplicate and delete/undo checks passed; the team
inspected all **60 PNGs** in `tmp/agent-ui/windows-lists-final/`. Root additionally
inspected all four new duplicate screens and five narrow deletion captures.
Live server copy/subtask semantics and unsupported/incomplete List orderings
remain explicit boundaries.

Readable List timestamps are now implemented from captured client code, including
relative labels, tooltips, locale/time24 and visible minute refresh. The final
independently locked core suite passed **1,124 / 5 ignored**. Normal/narrow List
checks passed and the team inspected all **50 PNGs** in
`tmp/agent-ui/windows-timestamps-final/`. Windows taskbar badges now use a direct
HRESULT-reporting sink with retained recreation handling. Eleven native stages
passed; visible taskbar captures verify numeric/unread/clear states. The first
capture attempt exposed auto-hide, so the runner now offers temporary edge hover
with cursor restoration, without changing taskbar settings.

Windows toast identity and direct WinRT submission are implemented. The first
native attempt found `Setting` unavailable before the first `Show`; the targeted
fix passed **996 desktop tests / 6 ignored** and a **2m33s** dev build. Native
submission/history/removal and registration restoration passed; root inspected
the actual banner at 0.5 and 5 seconds in `windows-notifications-timed/`.
The immediate capture preceded the animation. Production taskbar projection now
passes six native stages with all six taskbar PNGs inspected (6/clear/4/clear/4/clear).
Account-owned synthetic COM routing passes queued latest destination, exact thread
reply visibility and foreign-account rejection; root inspected its PNG. Actual
warm OS toast activation now passes: one exact fixture Invoke produces one COM
callback and restores the same minimized PID/HWND to the foreground. Root
inspected the nonce-bearing banner and activated-window PNG. Exact registration
and preview cleanup pass. Receipts: `windows-notification-activation-final/`.
Cold activation now passes **1,029 desktop tests / 6 ignored**, format and a
**3m30s** dev build, followed by actual Windows OS activation. One exact fixture
toast click launches a new PID after its publisher exits, delivers one callback,
and brings the child HWND forward. Root inspected the nonce-bearing banner and
fully rendered channel at one and three seconds. Child cleanup restores all four
registration artifacts, removes the preview and releases the lease. Warm activation
and synthetic routing regressions also pass with inspected PNGs. A first-attempt
runner bug querying a terminated image was fixed; exact recovery and idempotent
recovery both passed before the successful fresh-nonce runs. Evidence:
`tmp/agent-ui/windows-notifications-cold-painted/`. Selected custom sounds, real
Explorer restart, live account-owned routing and live delivery remain open.

Modal multi-user inputs are implemented. The independently locked core suite is
now **1,131 passed / 5 ignored**; the desktop suite is **1,013 / 6 ignored**.
Native testing found and fixed two concrete issues: by-value bootstrap future
copies overflowed Windows' 1 MiB main stack, and Escape from inline member choices
also closed the modal. Startup now boxes its future at the Dioxus hook boundary;
the largest startup spawn frame falls from 623,000 to 440 bytes, and plain-channel
TCP startup passes. The modal Escape fix uses the existing local Escape marker;
its full desktop tests, **2m28s** dev build and normal/narrow native checks pass.
All **ten** final modal PNGs were inspected, including a separate validation
capture with verified visible error text and hit testing at both sizes. The
final validation dev build passed in **2m07s**, with 1,013 desktop tests unchanged. All six
shared user/multi-user/conversation normal/narrow checks pass with inspected PNGs.
An additional 15s idle sample records host/WebView2 CPU **0.03125/0.0625s**,
total owned working-set/private-commit **530.71/311.04 MiB**, and first paint
**560.86ms**. It does not demonstrate improvement over the earlier single sample;
WebView2 memory is higher. Receipts: `tmp/agent-ui/windows-idle-modal-os/`.
Single-user modal support now passes **1,137 core / 5 ignored**, **1,017 desktop /
6 ignored**, both format checks and a **2m58s** dev build. Single/multi-user modal
checks pass at both sizes with all **18 PNGs** inspected; shared message pickers
add six passing checks and inspected PNGs. Receipts: `windows-modal-user-final/`
and `windows-single-shared-final/`.
Fixtures now use a unique Windows WebView2 directory and in-memory picture cache;
the pinned runtime otherwise shares the production profile despite TMP isolation.
Conversation and public-channel modal selectors are implemented with retained
opening-channel defaults, distinct state keys, full-state response URL flags and
Enterprise-context submission guards. **1,153 core / 5 ignored**, **1,059 desktop / 7 ignored**, both format checks
and 11 JavaScript tests pass; the dev build passes in **3m28s**. Scheduling now uses
a typed DOM receipt before local settlement, merges attachment-only edits, and
rejects stale same-channel returns. These guards passed independent review. Draft polling compares
metadata before cloning retained content; its paired component benchmark improves
unchanged/sparse-change cases and reports slower all-emitted cases in the worklog.
Both selector families now pass normal/narrow Windows checks with **20 inspected
PNGs**; modal user regressions and shared message pickers add **24 inspected PNGs**.
Scheduling passes all three fixtures with root-inspected captures. GUI-subsystem
PE=2, cold/warm activation, synthetic exact-reply routing and completed recovery
all pass with inspected captures and unchanged registration. Final binary SHA:
`02f559d717f400e36c83676bd0fb25dbc620cafe21a533fdfa121fbd680ddc32`.
Three interleaved idle pairs now confirm the GUI build has no owned console host;
CPU, private memory and first-paint differences vary across pairs. Scroll p50 is
6.1ms in both builds, but a blank transcript in A3 and a hover card in B1 prevent
clean visual comparison. A separate cursor-normalized repeat now passes all six
runs and 18 inspected immediate/delayed PNGs: both builds have 6.1ms p50 and
6.2ms median p95. No improvement is demonstrated; the original blank pane did
not reproduce and its cause remains unknown. Preserve both sets of artifacts.
An earlier slice added rejected thread draft restoration with isolated
restart proof and ordinary-message public-channel selection.
That checkpoint passed **1,157 core / 5 ignored**, **1,074 desktop / 7 ignored**,
both format checks and a **3m14s** dev build, SHA
`4574d647738d5d329262ac14e06bca77eec9fa4511d1901ba967d4a73a190c59`.
Message channel selectors pass normal/narrow Windows checks with both PNGs read;
large channel/five thread fixtures and upload/forward recovery also pass with
inspected captures. Thread-return native verification exposed a success toast
covering its composer. The first placement repair passes normal restore/restart
on build SHA `a0425878533e4366b311d333a34e379ca7bc5a1aeb90146bdce904f26e30755e`
(1,074 desktop tests, ten JS tests, 2m54s dev build), but narrow restoration
exposes a notice covering focused Continue writing before the composer opens.
Those notice failures were subsequently fixed: normal/narrow focused thread
restoration and restart pass in `windows-rejected-thread-focused-final/`, with
all ten captures inspected. Canvas text/checklist checks also pass at both sizes
in `windows-canvas-final/`, with all twenty captures inspected. The opening
checkpoint above identifies the latest tested binary and subsequent source work.
Continue the full acceptance backlog. Windows session files exist (names
only inspected), but no live sign-in/reference or cross-client proof is established.
Private cursor/app-control/duplicate proof and the broad acceptance matrix remain open.
No real Slack/session/draft writes, commit, push or packaging
occurred. Keep the dirty tree and new source/script files; HEAD alone omits this
continuation. Historical Mac details below remain useful context.

Handoff prepared on **2026-10-13** from `/Users/sh/Documents/GitHub/snack` on the
user's MacBook. The user explicitly **paused** the long-running goal to move
machines. Resume implementation when the user asks to resume on the new machine.
The project is **not complete**, and no defensible overall completion percentage
has been established.

## Start here

1. Read this file, the repository `AGENTS.md`, and `docs/design-system.md`.
2. Confirm that the **working tree**, including untracked source files, arrived.
   A fresh clone of the current commit alone loses the latest work.
3. Preserve the verified native Windows dev/TCP/WebView2/capture checkpoints
   above. Recheck processes after reboot and rebuild when source changes require
   it; old macOS evidence alone never establishes Windows behavior.
4. Preserve the completed Windows checkpoints above, including modal user
   selectors, warm/cold OS toast activation, taskbar projection, and List work.
   Continue the managed Schedule and draft-observation work described at the
   top, then the remaining acceptance matrix. Private cross-client verification
   and broader Windows workflow coverage remain open requirements.
5. Keep the full parity/design/performance objective intact. Use the acceptance
   table and chronological receipts in `docs/parity-worklog.md`; later entries
   supersede earlier hypotheses and open items.

## What the user wants and how to work

The original objective is full Slack feature parity, a premium compact interface
inspired by [Vercel Geist](https://vercel.com/geist/introduction), and measured
performance improvements. This is a native Rust/Dioxus Slack client, not an
Electron wrapper or a static visual clone.

- **Use dev builds.** The user explicitly asked us to stop spending time building
  signed app bundles. Use `cargo build --locked` and the built executable for
  normal iteration. Old README/worklog references to signed Mac builds do not
  override this preference. Packaging is not an ordinary UI verification gate.
- Work autonomously, implement scoped fixes, and distinguish source evidence,
  unit tests, native fixtures, and live cross-client proof.
- The user requested parallel agents, up to **20 maximum**. The last runtime
  actually allowed only root plus three workers. Use the available limit; do not
  assume 20 concurrent slots. Divide file ownership and keep one integration
  owner. Previously root alone ran Cargo/builds/native/live checks to avoid
  competing builds and disk pressure.
- Real message sends are authorized **only in private `#echotestchamber`,
  channel ID `C0AQDL3V5GU`**. Other test writes must also stay in private artifacts.
  Do not use arbitrary public channels for testing. Reading/focusing conversations
  can mark them read; do not navigate the user's active client casually.
- Real huddle joining needs a human in the loop. Global preferences, notification
  schedules, profile changes, and unrelated production state are not blanket
  write-testing authorization. Implement and verify those in disconnected
  fixtures unless separately authorized.
- Preserve user drafts, sessions, unrelated edits, and the active app. Do not
  commit/push, deploy, or kill unrelated processes without authorization.
- The user offered `/Users/sh/Documents/GitHub/multiSlack` as inspiration.
  It is an optional separate reference repository, not a dependency. Its path
  will need replacing on Windows if it is transferred.

## Exact checkpoint and transfer requirements

At handoff, Git branch is **`main`**, HEAD **`26003ce` (`sync`)**. There are tracked
modifications and untracked source trees. Earlier parity work is already present
in the checkout/commit history; the newest capture/Dock work is still dirty.
No commit or push was made for this handoff.

Run `git status --short` on both machines and compare. In particular, transfer:

- `crates/super-platinum-core/src/state/dock_badge.rs` and its directory.
- `src/desktop/notification/badge.rs` and its directory.
- `src/desktop/state/dock_badge.rs` and its directory.
- `src/desktop/agent/capture.rs` and its directory.
- `scripts/check-dock-badge-native.py` and `scripts/check-capture-responsiveness.py`.
- All tracked edits accompanying those modules: manifests, module declarations,
  constructors, counts/realtime/read paths, notification adoption hooks, tests,
  and docs. The appendix lists the dirty tracked paths at handoff.
- This `docs/RESUME.md` file itself.

Do not rely on `git diff` alone to transfer work: it omits untracked files.
Do not copy the Mac `target/` or `crates/super-platinum-core/target/` directories
to Windows; rebuild native artifacts there. Do not copy stale socket markers as
if they were live endpoints.

`tmp/` and `captures/` are ignored local evidence, not guaranteed to accompany a
clone. Some contain real Slack messages, cookies, signed URLs, or tokens. Keep
them private; never commit or paste raw network captures. The static JavaScript
reference bundles and selected sanitized logs/fixture PNGs are useful to transfer
separately. Protocol docs preserve many findings if local captures are absent.

Authentication is not source code. Sign in normally on Windows rather than
assuming the Mac Keychain/session transfers with Git. In debug builds the current
config code uses `session.secrets.dev.json` under the platform config directory;
release builds use the configured keyring. Do not log or commit either session
secrets or raw captures. `STORAGE_QUALIFIER` and `KEYRING_SERVICE` deliberately
still say **snack**; changing them without migration orphans existing data.

## Windows bootstrap and verification

Current repository CI defines native `x86_64-pc-windows-msvc` and
`aarch64-pc-windows-msvc` builds in `.github/workflows/ci.yml`. Match the laptop's
architecture and native MSVC/C++ build environment. The ARM64 BoringSSL toolchain
configuration is `.github/cmake/windows-arm64.cmake`. Check the actual Windows
toolchain and current Dioxus platform documentation if prerequisites are missing.
Do not treat WSL/Linux validation as native Windows/WebView2 proof.

The Dioxus revision is pinned exactly to
`57d6794ad60b949e5bd8aa282f6f8c3dc97a365e`. Check
[Dioxus 0.7 documentation](https://dioxuslabs.com/learn/0.7/) before changing its
APIs; pinned source and compiler win when docs differ. Use Dioxus's Tao/Wry
reexports, not incompatible separate native types. Keep the existing optimized
dev profile unless measurements justify changing it.

From the repo root in PowerShell, after toolchain setup:

```powershell
cargo fmt --check
cargo fmt --check --manifest-path crates/super-platinum-core/Cargo.toml
cargo test --locked --manifest-path crates/super-platinum-core/Cargo.toml
cargo test --locked
cargo build --locked

$env:SUPER_PLATINUM_AGENT = "1"
$env:SUPER_PLATINUM_FIXTURE = "channel"
.\target\debug\super-platinum.exe
```

The core crate is independently locked (`[workspace]` in its own manifest).
Run its suite separately for domain changes. These commands describe the native
machine's gates; the continuation receipts above name the verified checkpoints.

On Windows the agent server uses loopback TCP, default `127.0.0.1:0`, not a Unix
socket. Optional `SUPER_PLATINUM_AGENT_ADDR` controls the listen address; keep it
loopback. It writes the actual `tcp://host:port` endpoint to
`[IO.Path]::GetTempPath()/super-platinum-agent.sock.path`.
`SUPER_PLATINUM_AGENT_SOCK` is the CLI's endpoint override, not the Windows
server bind setting.

A minimal second-terminal probe, assuming the Python launcher `py` is available:

```powershell
@'
import json, pathlib, socket, tempfile
endpoint = (pathlib.Path(tempfile.gettempdir()) / "super-platinum-agent.sock.path").read_text().strip()
assert endpoint.startswith("tcp://127.0.0.1:")
host, port = endpoint[6:].rsplit(":", 1)
request = {"id": 1, "cmd": "ping"}
with socket.create_connection((host, int(port)), timeout=35) as connection:
    connection.sendall((json.dumps(request) + "\n").encode())
    with connection.makefile("rb") as response:
        print(json.dumps(json.loads(response.readline()), indent=2))
'@ | py -
```

Change the request to `{"id":2,"cmd":"state"}` or
`{"id":3,"cmd":"screenshot","path":"tmp/agent-ui/windows-channel.png"}`
for structural and image checks. Read the generated PNG yourself. The PowerShell
launch/probe recipe was checked against repository code, not executed on Windows
during this handoff. `scripts/agentctl.sh` also supports TCP when run through an
appropriate Bash/Python environment, but its default `/tmp` marker lookup can
differ from native Windows temp; use the explicit endpoint if needed.

Stop only the fixture instance you launched. Remove the fixture variable before
launching a real session:

```powershell
Remove-Item Env:\SUPER_PLATINUM_FIXTURE -ErrorAction SilentlyContinue
$env:SUPER_PLATINUM_AGENT = "1"
.\target\debug\super-platinum.exe
```

Leave destructive agent actions disabled by default. The agent's `help` command
and `crates/super-platinum-core/src/agent_protocol.rs` describe the typed protocol.
There is no general-purpose live DOM-eval agent command.

Important platform gaps:

- Many `scripts/check-*-native.py` runners assume a Unix executable path, `/tmp`,
  Unix sockets, and the shell CLI. Adapt process/endpoint handling before using
  them on Windows; their current existence is not Windows test coverage.
- `src/desktop/agent/capture.rs` has a Windows PowerShell/native-window capture
  branch. Its latest async refactor was exercised on macOS, not Windows.
- The new Dock label and attention modules are macOS-gated. They do not implement
  Windows taskbar badges or prove Windows toast delivery.
- The offline fixture names and in-app check logic remain useful. See
  `scripts/agent-ui-check.sh`, `src/desktop/state/boot.rs`, and the relevant check
  module. Prefer focused fixtures over launching the entire large sweep.

## Architecture and design boundaries

| Area | Ownership |
| --- | --- |
| `crates/super-platinum-core/` | Slack API/realtime/models, cache/config, reducers, domain state, agent protocol |
| `src/desktop/main.rs` | Dioxus application and root hooks |
| `src/desktop/state/` | Serial shell state, projections, bounded windows, selection, composer, fixtures |
| `src/desktop/bootstrap/`, `messaging.rs`, `realtime.rs` | Async Slack orchestration, guarded adoption and persistence |
| `src/desktop/view/`, `styles/`, `overlays.rs` | Typed UI and CSS |
| `src/desktop/media.rs` | Opaque native media protocol and loading |
| `src/desktop/agent.rs` | Optional local control plane |

The visual source of truth is `docs/design-system.md`: local Geist/Geist Mono,
neutral graphite surfaces, compact controls, restrained borders, stable layout,
clear secondary text. Reuse CSS tokens in `base.css` and `design-system.css`.
The user's selected appearance can differ from the default design palette.

All interface icons must use **Google Material Symbols Rounded** via
`src/desktop/icons.rs`. No emoji/text glyph substitutes or hand-drawn SVG icons.
User-authored emoji, avatars, brand marks, and actual message content are content.

## What has been implemented during the larger run

This is an orientation summary, not a fresh claim that every workflow passed on
every platform. Detailed receipts, limits, and historical tests live in the
7,000+ line `docs/parity-worklog.md`. Its top acceptance table is useful but some
rows lag the latest dated entries.

| Workstream | Implemented work and evidence boundaries |
| --- | --- |
| Visual system | Geist fonts/tokens, denser shared surfaces, hover/selection cleanup, normal/narrow fixtures. Full visual parity and accessibility coverage remain incomplete. |
| System messages | Explicit lifecycle/pin subtype allowlist; body uses `--muted`, author remains bright. Shared timeline/thread renderer and native fixture verified. Ordinary chat/app/broadcast content retains normal contrast. |
| Dependencies | Requested updates already present. Current root lock has Tokio 1.53.2 and Rustls 0.23.45; objc2 0.6.5, Tao 0.34.8/Wry 0.53.5 aligned with pinned Dioxus. Historical remote CI covered an older committed dependency baseline, not this dirty tree. |
| Messages/composers | Formatting, Unicode/selection handling, emoji/mention work, thread broadcasts, message controls, retries, scheduling/drafts and local composer recovery. Complex destination/queue and cross-client races still need coverage. |
| Navigation/read windows | Channel/thread anchoring, older exact reply targets, bounded rich rendering with complete cheap row metadata, thread pagination and geometry improvements. Keep canonical data separate from rendered windows. |
| DMs/profiles/channels | Recipient discovery and DM/group-DM opening, presence/profile panes, status text/emoji/expiry, own-profile text/title editors, channel create/about/members/archive/join/leave/invitation actions. Some real mutation round trips remain unverified. |
| Search | Filters, sort, scoped pagination, shortcuts, cold metadata resolution and exact channel/thread destinations. Broader filters and multi-account/outage/live cases remain. |
| Files/Later/Pins/bookmarks | Browsing/paging, upload/download, code/text/audio/PDF previews, own-file rename/delete, saved messages/files/reminders, pin navigation, bookmark links/folders/order. Private API and many native checks exist; full file-format and realtime coverage does not. |
| Recovery | Account/destination-owned drafts; persistent uncertain upload delivery journal; explicit readback/recovery rather than automatic duplicate sends. Preserve newer local edits and retained operation ownership. |
| Forwarding | Rich notes and destination selection, plus a durable account/source/destination journal for uncertain delivery. Restart recovery offers receipt checks, explicit resend and stop tracking; opening recovery never sends automatically. Native recovery checks passed historically; broader live and scheduling parity remains unfinished. |
| Canvases | Reader/create/rename plus bounded checkbox and existing-paragraph/rich-text edits using the actual collaboration protocol; private save/restore proof. Full authoring, comments and collaboration remain incomplete. |
| Lists | Saved tables/boards, record detail, ordinary cell editing, create/delete and same-ID Undo/Redo. Source-backed Duplicate, stable delivery locks and readable timestamps now pass Windows normal/narrow checks. Server continuation, live duplicate semantics, view writes and richer fields/actions remain. |
| App interactions | Typed app modals and ordinary message controls with retained ownership. Modal multi-user and ordinary/modal multi-channel/conversation controls pass Windows normal/narrow checks. Single external selectors are in the current build/native gate; live app receipt/response and broader workflow coverage remain incomplete. |
| Notifications | Channel settings, recurring schedules, global defaults/keywords, presentation and canonical badges. Windows OS preview delivery and actual warm/cold activation pass bounded checks; global defaults pass disconnected Windows workflows. Real message delivery, broader routing and several global live writes remain unverified. |
| Huddles | Slack room + Chime state/media bridge, camera/sharing/device UI. Real call/media proof still requires a controlled human-approved call. |
| Performance | Measured palette, message snapshot, DM lookup, geometry/thread-index and reaction projection improvements; bounded rendering, media loading and timer fixes. Component CPU wins are not whole-app FPS or Windows latency proof. |

## Latest completed and verified slices

### Async screenshots and a disproven rendering hypothesis

The old agent screenshot path blocked the Dioxus main thread while waiting for a
child process. It now awaits bounded `tokio::process` capture, keeps native-handle
access on the main thread, performs file I/O off-thread, preserves serial agent
responses, and kills/reaps timed-out or cancelled children. Files:
`src/desktop/agent.rs`, `agent/io.rs`, `agent/capture.rs`, and its tests.

In an isolated Mac fixture with an injected two-second capture delay:

- Baseline AX resize failed after 1.695s; screenshot finished in 2.142s at the old
  2200×1480 window size.
- Patched resize succeeded in 0.201s while capture was pending; screenshot took
  2.174s and had the new 2000×1400 size.
- An injected 30-second child returned timeout at 10.004s and the exact child
  PID was gone. This is scoped responsiveness/fault proof, not general FPS.

Evidence: `scripts/check-capture-responsiveness.py`,
`docs/runtime-responsiveness.md`, and ignored
`tmp/agent-ui/capture-responsiveness-{baseline-idle,patched}.json`.

**Do not reopen the alleged missing-text repaint bug based on the image tool
alone.** Three separately captured PNGs had identical decoded RGB bytes while
the image viewer presented them differently. The hypothesis was disproven;
`tmp/capture-pixel-audit.json` records the comparison. No repaint hack or generic
DOM-eval command was added. If this recurs, compare actual pixels/independent
display before changing rendering code.

### Activity/DM policy and Dock badge

- Activity Inbox badges use the exact allowed categories and experiment gates;
  reactions and unknown categories are not blindly added. Known Inbox with
  missing authoritative counts does not fall back to a guessed raw total.
- **DM rail and Dock DM are different selectors.** The rail uses eligible
  per-conversation highlights/forced-unread state; the Dock opt-out contribution
  uses the Activity `dm` category. Do not conflate them.
- Canonical HTTP counts and realtime events have revision/owner guards. Omitted
  DM `mention_count` in authoritative counts clears an old numeric count before
  forced-unread fallback; partial metadata refreshes retain merge semantics.
- New core Dock projections consume canonical Activity, legacy channel-badge,
  thread and saved overdue data. Numeric counts win regardless of bullet opt-in.
  Later eligibility/visibility and raw JavaScript preference truthiness matter.
- Saved counts update background workspaces and reject older HTTP replacement.
  A valid saved object missing overdue retires its authority; malformed/null
  input preserves prior authority. Legacy maps merge; Activity maps replace.
- Shell caches update at mutation boundaries. The native hook reads a cached
  scalar/owner, not all channels on every render/tick. Ownership retirement
  clears the badge. Known numeric zero cannot retain a stale number merely
  because ordinary-unread/Enterprise context is unknown.
- **Remaining gap:** authentic unified-user/MRU/org-channel Enterprise context
  is not ingested. Legacy numeric and ordinary-unread branches needing it are
  explicitly unknown. Do not infer it from enterprise ID or current tab.
- The macOS adapter handles decimal label, U+2022 bullet, unchanged suppression,
  account transitions and drop cleanup. Windows taskbar behavior is not added.

Read `docs/notification-presentation-protocol.md` for exact category gates,
defaults, source modules, normalization and aggregation formulas.

### Last full gates at the code checkpoint

| Check | Result / local receipt |
| --- | --- |
| Independently locked core suite | **1,090 passed, 5 ignored**, `tmp/dock-badge-core-tests.log` |
| Locked desktop suite | **964 passed, 6 ignored**, `tmp/dock-badge-desktop-tests.log` |
| Both formatting checks / diff whitespace | Passed |
| Bare dev build | Passed in **3m00s**, `tmp/dock-badge-dev-build.log` |
| Native adapter check | Actual AppKit labels; ten writes, unchanged input zero writes; `tmp/dock-badge-native-check.log` |
| Production hook check | Activity6 excluding900 reactions → clear → Later4 → hidden Later clear → Later4 → account-retirement clear; `tmp/dock-badge-projection-check.log` |

Native badge runs used disconnected fixtures, no Slack transport or preference
writes. They establish actual AppKit label state, **not rendered Dock pixels,
live event agreement, Windows behavior, or complete Slack parity**. The build
reported the existing future-incompatibility notice for `block v0.1.6`.

## Activity cursor reconciliation: implemented Windows checkpoint

At the original Mac handoff this task had no edits. The Windows continuation now
implements the following recorded source contract and passes pure regressions
plus normal/narrow native fixtures. It does **not** establish live cross-client
cursor delivery. The original defect list and ownership plan below are retained
as context for reviewing the fix, not as unimplemented work.

Original source-backed defects (now addressed):

1. `src/desktop/realtime.rs` channel/thread marked events update conversation or
   thread cursors without reconciling matching Activity rows/category counts.
2. `crates/super-platinum-core/src/activity_feed.rs` currently has uncursored
   `mark_channel_read_categories(channel)`. It can clear entries newer than the
   acknowledged cursor and excludes only `item.is_thread()`.
3. A threaded `at_user`/keyword item with `message.thread_ts` is not necessarily
   a thread-kind item, so opening its channel can incorrectly clear it.
4. `crates/super-platinum-core/src/thread_reads.rs` updates the Threads surface,
   not all matching Activity state. Reading a thread in Slack/native can leave
   its Activity row unread until another refresh.

Captured source contract from `/DLK` (S5145254), `MdxY` E/w (S5086450 onward),
and `YfTA` (S4902019):

- Channel cursor: same channel, **no resolved thread timestamp**, mapped message
  timestamp at or before the acknowledged `lastReadCursor`.
- Thread cursor: same channel and exact root, mapped message timestamp at or
  before that cursor.
- Ordinary message mapping uses message `ts`/`thread_ts`; `thread_v2` uses
  **`min_unread_ts || latest_ts`**, not its root timestamp.
- Decrement once per previously raw-`is_unread` root item/category; clamp at zero.
  Effective unread bundle counts and raw category-count eligibility differ.
- Preserve the current direct-item-read rule: `QDAL.m` changes the row, not the
  canonical Inbox category map. Do not indiscriminately decrement on all reads.

Ownership used for the implementation:

- Core worker: pure bounded channel/thread reducers, timestamp mapping, category
  deltas, and `ActivitySync` overlays so an older in-flight page cannot resurrect
  read state. Own `activity_feed.rs` and model/helpers/tests as needed.
- Desktop worker: `realtime/activity_counts/reads.rs`, its tests, native ACK in
  `bootstrap/history.rs`, thread read paths, and realtime marked events. Pass the
  **captured acknowledged cursor**, never a newer current latest timestamp.
  Preserve account/team/socket generation and per-surface read ownership.
- Source/review worker: verify raw root/bundle and timestamp semantics against
  the captured modules before approving implementation.
- Root: integration, locked suites, native normal/narrow fixture/check extension
  (`src/desktop/activity_feed/check.rs`, `scripts/check-activity-native.py`),
  screenshots and private-only live checks if useful/authorized.

Required regressions: newly arriving rows beyond the cursor; threaded mentions
during channel reads; exact thread roots; duplicate/out-of-order marks; wrong
workspace/account/socket generation; marks while Activity pagination is in
flight; raw-false bundles; category decrement once rather than by message units.

## Other concrete findings from the final read-only audit

**Lists continuation:** 50 visible rows is local paging over all fetched entries,
not silent server truncation. Server continuation really is absent: one
`lists.records.list` request, retained cursor, explicit incomplete banner;
Refresh repeats page one. However, captured stock callers also fetch once without
cursor/limit and no available live evidence proves a cursor-bearing List. Do not
invent a continuation parameter/contract just to hide the warning. First obtain
a real read-only large-List response and verified next-page request. Missing
**Duplicate item** was the immediately source-backed next feature at the handoff:
stock uses `duplicatedRecordId` plus adjacent position in the existing creation
endpoint. It is now implemented and Windows fixture-verified. The exact original
B/S/V bundles were recovered from public Slack asset URLs; the dated source
section in `slack-lists-protocol.md` records hashes, offsets and support boundaries.

**Ordinary message `multi_users_select`:** implemented and Windows fixture-verified
after the handoff; it was inert and could disable sibling actions at the Mac pause.
Captured B module `hGZA`, offset13286237, uses
`savedUsers || initialUsers || []`, includeSelf/allowEmptyQuery, staged selection,
maxSelectedItems, Cancel restore, and submit (13288513) with
type/actionId/blockId/selectedUsers plus initialUsers when supplied. Reuse the
single-user directory and staged multi-static UI rather than inventing behavior.
Cover required/max/owner/stale/429/full-sibling state and narrow interaction.

**Ordinary message overflow:** implemented and Windows fixture-verified after the
handoff; it was an inert More affordance at the Mac pause. Captured B
`UVRG`13292917 / action13293902 sends `selectedOption:{text,value}`; option URL
handling follows confirmation separately. Add typed bounded options, exact
payload/confirmation/URL behavior and no-replay tests. Both app-control features
now have those tests and normal/narrow native prepared-request checks. Owned
private live app receipt/response proof remains open.

Other broad open areas include richer canvas authoring/comments, List view
writes/field types/actions, custom channel tabs, workflow discovery/execution,
live app receipts, controlled huddle media, Windows-specific OS integration,
accessibility/IME and full visual/performance sweeps. Do not redefine success as
only the features already implemented.

## Lessons that should prevent regressions

- Realtime messages are **partials**: merge them. A `message_replied` frame can
  omit reactions; assignment erases pills. Update channel and every thread copy.
- A surface owns its destination. Global opens go to Home; DMs/Activity keep
  their own selected conversation and empty state. Reuse the shared message row.
- Async work needs identity/generation guards after awaits. An uncertain write
  remains owned after its view closes; verification failure is not permission
  to replay it. Preserve drafts and newer edits while recovering.
- Slack API fields are not always canonical client fields. Trace the mapper
  before copying selectors. IM/MPIM membership/cross-team normalization changed
  our conclusions about DM eligibility. Prefer source-backed exact contracts.
- Canonical counts, loaded feed rows, rendered windows and UI category counts
  are different data. Do not use whichever total is easiest to access.
- Treat HTTP absence, explicit zero/empty, partial patches and malformed input
  distinctly. Guard snapshots against events and local writes during the fetch.
- Slack text is typed `RichNode` content, never raw HTML injection. Split embedded
  rich-text newlines properly; blocks win over fallback text when they render.
- Broken-image icons are bugs. Use ready gates/fallbacks or native placeholders;
  URLs must include `uri_at(media_epoch)`. Source registration can happen during
  render, so the bounded sweep is necessary. Retry transient failures, abandon
  permanent ones, and keep third-party image fetches free of Slack cookies.
- Persistent media slots are stable identities, not rotating URLs. Provisional
  old bytes can paint while updated bytes load. Webhook identity needs extra care.
- Respect offline holds, Retry-After, stale-event guards and warm caches. Use
  `ShellState::report_failure`, not many raw error toasts containing signed URLs.
- Dioxus eval lifetime matters. A JavaScript function finishing can retire the
  eval owner; keep receivers alive until the expected response. Verify native
  interactions instead of relying exclusively on pure tests or guessed APIs.
- Do not add network/disk work to render paths, full-map scans on media/typing
  ticks, or always-on observers for occasional work. Measure both the component
  cost and the actual user-visible path, labeling one-run evidence honestly.
- macOS App Nap/background suspension explains some observation delays; do not
  declare a process dead because a single agent observation timed out. Poll the
  same live process/handle before restarting. This is not automatically a Windows
  explanation.

## Reference map and local evidence

| Resource | Purpose |
| --- | --- |
| `AGENTS.md` | Architecture, safety, Slack/media/realtime invariants, verification rules |
| `docs/parity-worklog.md` | Broad acceptance matrix plus chronological implementation/validation receipts |
| `docs/design-system.md` | Geist-based visual contract |
| `docs/slack-lists-protocol.md` | Exact List cell/action payloads, ordering and mutation/recovery findings |
| `docs/slack-canvas-edit-protocol.md` | Collaboration session/Automerge/provenance/save/restore findings |
| `docs/channel-notifications-protocol.md` | Channel preference contracts |
| `docs/notification-schedule-protocol.md` | Recurring schedule behavior |
| `docs/global-notifications-protocol.md` | Global defaults/keywords |
| `docs/notification-presentation-protocol.md` | Sounds, attention, Activity/DM/Dock counts and source offsets |
| `docs/runtime-responsiveness.md` | Startup/idle/App Nap/edit/capture investigations and limits |

Source-offset shorthand used in research:

- **B**: `tmp/sb/js/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js`
- **S**: `tmp/sb/js/gantry-v2-shared.full-capture.0b81595876785d7a.min.js`
- **V**: `tmp/sb/js/gantry-v2-vendors.522a064003a98b05.min.js`
- **M/P**: installed Mac Slack4.52.178 `app.asar` main/preload bundles. That
  installation path does not exist on Windows; offsets are version-specific.

Do not run a normal line-oriented search that prints an entire minified bundle.
Use bounded substring extraction/module offsets. Do not dump raw JSON captures.
`scripts/capture-slack-cdp.mjs` and Playwright/CDP were used for the real Slack
reference. Tool availability/login/debug endpoints must be re-established on
Windows; never navigate a real Slack renderer to a non-`/client` URL casually.

Mac-only process state at handoff (historical, not a Windows endpoint):

- Bare live dev PID **98097**, socket **`/tmp/sp-dev-live.sock`** responded to ping.
  The user was in an Activity thread; it was not restarted or navigated.
- `target/debug/super-platinum` on disk contains the newest capture/Dock work;
  the already-running process is an older mapped binary. Do not infer deployed
  runtime version from the executable's current path alone.
- Latest read-only PNG: `tmp/agent-ui/parity-next-live.png`, inspected. It contains
  real conversation content and is private evidence, not a fixture to commit.
- No Cargo/rustc or feature implementation agents remained active at handoff.
- Storage was approximately11GiB free after removing only superseded incremental
  cache directories. Do not resume a broad disk cleanup; Windows needs a fresh
  build, not the accumulated Mac target trees.

## Dirty tracked paths at handoff

These are in addition to the untracked source trees listed above. Preserve them
as a coherent working-tree transfer; some include the prior capture slice.

```text
Cargo.toml
crates/super-platinum-core/src/cache.rs
crates/super-platinum-core/src/later.rs
crates/super-platinum-core/src/later/tests.rs
crates/super-platinum-core/src/palette.rs
crates/super-platinum-core/src/slack/events.rs
crates/super-platinum-core/src/slack/realtime.rs
crates/super-platinum-core/src/state/activity_counts/policy.rs
crates/super-platinum-core/src/state/mod.rs
crates/super-platinum-core/src/state/notifications.rs
crates/super-platinum-core/src/state/sidebar.rs
crates/super-platinum-core/src/state/tests/users.rs
crates/super-platinum-core/src/state/tests/workspace.rs
docs/notification-presentation-protocol.md
docs/parity-worklog.md
docs/runtime-responsiveness.md
src/desktop/agent.rs
src/desktop/agent/io.rs
src/desktop/bootstrap/session.rs
src/desktop/channel_discovery.rs
src/desktop/global_notifications/lifecycle.rs
src/desktop/later.rs
src/desktop/main.rs
src/desktop/notification.rs
src/desktop/presentation_notifications/lifecycle.rs
src/desktop/realtime.rs
src/desktop/realtime/activity_counts.rs
src/desktop/realtime/activity_counts/rules.rs
src/desktop/realtime/activity_counts/tests.rs
src/desktop/state/boot.rs
src/desktop/state/mod.rs
src/desktop/state/shell.rs
```

There is no requirement to recreate the old agent names or memory directory.
Use the repository and this handoff as the durable context. Resume with fresh
evidence, update the worklog, and keep the full goal active until its actual
requirements are implemented and verified.
