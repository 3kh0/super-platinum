"""Independent v7 clearing: original nonliteral history, empty boundary, ordered input.

Only the independently pinned canonical source cohorts are admitted. Existing v5/v6 validators
remain separate; final text cannot substitute for the actual clearing history.
"""
import base64
from dataclasses import dataclass
import hashlib
import re

import canvas_end_typing_oracle as typed
import canvas_end_split_oracle as end
import canvas_end_reuse_oracle as reuse
import canvas_split_oracle as shared
import canvas_reuse_base as bases
from canvas_heading import require, wire, decoded, field, text

# Digests of executed original Session projections, excluding only host
# identity and independent fixture lease. Repeated fields retain their order.
SOURCE_DIGESTS = {
    ('positive', 'spaces'): '041670361f506842cd07ee782eab11f44c84c448c3d94494b23833061dfdf3ce',
    ('positive', 'nbsp'): 'f1066bfd698afcd3449efea7219a9ed4632f872e001f84890d1cf5da59728da8',
    ('positive', 'br'): 'ba3a3d28001cc2e815b60eae81b87e902d5108f7a132e05cd2fce020552f873a',
    ('zero', 'spaces'): 'fad04ccf48a8641fe34b67ffb1e89e731605bf8c887e2b954e6863ef63640805',
    ('zero', 'nbsp'): '28d1a220eaef40ff4e82cae53f2acbffb88dbdefba5d25528ec182782eed0f09',
    ('zero', 'br'): '188ecc7a597233bfe7e0d9d670e13d4716202c4e695757a4783cc61784f93421',
    ('positive', 'mixed'): '9244b4800b7df7c2280c53a91cacc159ffdc7261ea6387785d65874fa15b0a41',
    ('positive', 'mixed-br'): '2300c6dae75e97fd1855d69ce64ecb0b14cffc55ec1bc64de065d216ef437991',
    ('positive', 'separators'): 'f37f3e09428f9cc7f477997b3bee1e2d0d310fd7d09b7c950cdf35c60cc49b08',
    ('positive', 'feff-leading'): 'de46cbe56869ff88173269346079bd2763c8f329cd05c90463a2bac2938089d3',
    ('positive', 'feff-interior'): '05e3771cde751b70d63cea156060cfc6b42d3e3a473e3b86899a3dfe2323399a',
    ('positive', 'feff-post-br'): '30750b02d74969d3fae63aef88b0674d49db12cc6cfdc62b5ad821117bf6c851',
    ('zero', 'mixed'): '4236e2e3a56d4b1be38b4b7e700af4b976d0539e16364e2a9b15b9dafa3482ef',
    ('zero', 'mixed-br'): '1f535dfe4a4b37760875cf7846a73cf233140ed2c695134339e3ea9e669ee35b',
    ('zero', 'separators'): '32e461dfd54670665d7d04e48055df73c403715eee3c5995017f5301d4bd4e1e',
    ('zero', 'feff-leading'): 'dd9e0e89827667c4a38ee3a2c4c3397929ad99b90aa964eb9030516a8cecb085',
    ('zero', 'feff-interior'): '816262c756c559ac929530adbe1458683ab616a2678a4d5e6a051925bfcb1807',
    ('zero', 'feff-post-br'): 'a95e151f24656eb0d1a7c7e34e286349a689331ca4472318797872369c870993',
}

# Exact finite fixtures, not a Unicode trim/normalization predicate.
MIXED = " \u00a0\u1680" + "".join(chr(n) for n in range(0x2000,0x200c)) + "\u202f\u205f\u3000"
SOURCES = {"spaces":"   ", "nbsp":"\u00a0", "br":"<br>",
           "mixed":MIXED, "mixed-br":MIXED+"<br>"+MIXED[::-1],
           "separators":" \u2028\u00a0\u2029\u200b", "feff-leading":"\ufeff ",
           "feff-interior":" \ufeff\u00a0", "feff-post-br":"<br>\ufeff"}

@dataclass(frozen=True)
class Config(typed.Config):
    reuse_content: str
    source_text: str
    source_plain: str
    source_label: str
    source_kind: str
    user_updates: tuple

    @property
    def reader_text(self):
        # blocks::split_section_on_newlines makes sibling paragraphs, dropping
        # empty segments; DOM textContent concatenates their text without LF.
        # LS/PS and FEFF are literal text; only exact BR produces this LF.
        return "".join(self.source_plain.split("\n"))


def config(corpus='ascii', typing='none', reuse_base='positive', reuse_content='spaces'):
    require(typing in ('none','correction','erase') and reuse_content in SOURCES, 'Unpinned clearing cohort')
    c = typed.config(corpus, 'reuse', typing if typing != 'none' else 'erase', reuse_base)
    source = SOURCES[reuse_content]
    kind = reuse_content if reuse_content in ('spaces','nbsp','br') else 'whitespace-br' if '<br>' in source else 'whitespace'
    label = {'spaces':'3 spaces','nbsp':'Non-breaking space','br':'Line break','whitespace':'Whitespace','whitespace-br':'Whitespace and line break'}[kind]
    updates = () if typing == 'none' else c.updates
    return Config(corpus, 'reuse', typing, reuse_base, c.child, c.base, c.seed, c.wire_seed, c.offset,
                  updates or ('',), end.corpus_config(corpus).edited if typing == 'none' else c.continued,
                  reuse_content, source, source.replace('<br>','\n'), label, kind, updates)


def labels(value, expected, recovery=False):
    typed.labels(value, expected, recovery)
    require(value.get('reuseContent') == expected.reuse_content, 'Clearing content selector mismatch')


def source_session(row, expected):
    require(text(row,1) == expected.child and shared.body(row,False) == expected.source_text, 'Clearing source literal/identity changed')
    session = wire(field(row,38,2))
    require(text(session,1) == expected.child and field(session,2,0) == expected.base and field(session,3,0) == 1000
            and field(session,4,0) == 8_000_000_000_000_000, 'Clearing canonical source base/lease changed')
    canonical = b''.join(f[3] for f in sorted(session,key=lambda f:f[0]) if f[0] not in (1,4))
    require(sum(f[0]==1 for f in session)==sum(f[0]==4 for f in session)==1
            and hashlib.sha256(canonical).hexdigest() == SOURCE_DIGESTS[(expected.reuse_base,expected.reuse_content)],
            'Clearing source is not the exact retained original session/history')
    return session


def inventory(value, expected, original=None, child=None, accepted=False, continued=False, stale=False):
    if accepted: return typed.inventory(value,expected,original,child,True,continued,stale)
    cohort=bases.config(expected.reuse_base); ids=cohort.ids
    require(isinstance(value,dict) and tuple(value.get(k) for k in ('file','thread','document')) == ('Ffixture','thread','document'), 'Clearing source owner changed')
    thread,document=decoded(value.get('threadRaw')),decoded(value.get('documentRaw'))
    require(text(thread,1)=='thread' and text(thread,10)=='document' and text(document,1)=='document' and text(document,6)=='thread'
            and text(document,42)=='xyz' and text(wire(field(document,69,2)),1)=='Fixture title', 'Clearing raw document owner changed')
    rows=value.get('sections')
    require(isinstance(rows,list) and [r.get('id') for r in rows]==list(ids), 'Clearing lost stable four-row inventory/order')
    result={}
    for item,identity,position in zip(rows,ids,reuse.POSITIONS):
        row=decoded(item.get('raw'));result[identity]=row
        require(text(row,1)==identity and text(row,6)=='thread' and text(row,7)=='document' and text(row,8)==position
                and type(item.get('sequence')) is int and item['sequence']==field(row,2,0), 'Clearing source row association changed')
    title,neighbor,parent,tail=(result[k] for k in ids)
    for row in (parent,tail):
        for tag in (3,9,10,35):
            if any(f[0]==tag for f in row):require(field(row,tag,0)==0,'Clearing source not ordinary editable paragraph')
        if any(f[0]==13 for f in row):require(field(row,13,2)==b'','Clearing source gained parent')
    require(field(title,9,0)==64 and field(title,10,0)==48 and field(title,2,0)==1000
            and text(wire(field(wire(field(title,12,2)),58,2)),1)=='Fixture title', 'Clearing title changed')
    require(shared.body(neighbor)=='Another paragraph' and field(neighbor,2,0)==1000
            and next((f[3] for f in neighbor if f[0]==1001),None)==b'\xc8\x3e\x81\x00','Clearing neighbor changed')
    require(shared.body(parent)==expected.wire_seed and field(parent,2,0)==1000 and field(tail,2,0)==(3000 if stale else 1000),'Clearing source revision/body changed')
    source_session(tail,expected)
    if original:
        require(all(value.get(k)==original[k] for k in original if k!='sections'),'Clearing owner/document changed')
        before={r['id']:r for r in original['sections']}
        for item in rows:
            if item['id']==expected.child and stale:
                require(shared.stable(decoded(item['raw']),(2,))==shared.stable(decoded(before[item['id']]['raw']),(2,)),'Clearing stale changed beyond child revision')
            else:require(item==before[item['id']],'Clearing pre-acceptance source changed')
    return result


tags=typed.tags
decode_history=typed.decode_history
ack=typed.ack
continuation=typed.continuation

def semantic_history(current, source, expected, session, *, new=False):
    """Walk real historical views; only the known plain, linear cohort is allowed."""
    require(new is False and source is not None, "Clearing cannot allocate/bootstrap a new child")
    doc = field(current, 5, 2); actor = session + "00000000"
    reports = decode_history(doc, None if new else field(source, 5, 2))
    report = reports[0]; changes = report.get("changes")
    require(isinstance(changes, list) and changes, "Missing actual Automerge history")
    prefix = changes[:1] if new else reports[1]["changes"]
    require(prefix and len(changes) > len(prefix), "Typed operation lost all effective edits")
    if not new:
        require([c["raw"] for c in changes[:len(prefix)]] == [c["raw"] for c in prefix], "Typed child reset/reordered original history")
        require(reports[1]["heads"] == [prefix[-1]["hash"]], "Original child is not the admitted clean linear history")
        require(text(current, 8) == text(source, 8) and text(source, 8) in {c["hash"] for c in prefix}
                and all(f[2].decode() in {c["hash"] for c in prefix} for f in source if f[0] == 6), "Typed original metadata heads do not name retained history")
    require(prefix[-1]["text"] == expected.source_text + "\ufeff" and prefix[-1]["deleted"] is False and prefix[-1]["marks"] == [], "Clearing history source literal differs")
    if new:
        first = prefix[0]
        require(first["deps"] == [] and first["seq"] == 1 and first["actor"] == actor and first["keys"] == ["deleted", "text"], "New child bootstrap owner/shape changed")
        require(text(current, 8) == first["hash"], "New child base head is not bootstrap")
    actual = changes[len(prefix):]
    # Filled from source-equivalent splice boundaries: a replacement may have a
    # deletion and insertion change, both of which must remain in the document.
    expected_states = [""] + (typed.mutation_states(expected.user_updates) if expected.user_updates else [])
    require([c.get("text") for c in actual] == [s + "\ufeff" for s in expected_states], "Typed ordered history omitted/reordered/squashed a mutation")
    require([c.get("patches") for c in actual] == [[p] for p in clearing_patches(expected)], "Typed history changed original per-patch mutation order")
    previous = prefix[-1]; actor_seq = max((c["seq"] for c in prefix if c["actor"] == actor), default=0)
    for change in actual:
        actor_seq += 1
        require(change["actor"] == actor and change["seq"] == actor_seq and change["deps"] == [previous["hash"]]
                and change["deleted"] is False and change["keys"] == ["deleted", "text"] and change["marks"] == [], "Typed actor/dependency/plain-text custody changed")
        require(change["ops"] and all(op.get("action") in ("set", "del") and op.get("obj") != "_root" for op in change["ops"]), "Typed history changed document structure")
        previous = change
    require(report["heads"] == [previous["hash"]] and text(current, 12) == previous["hash"], "Typed current head not final actual history head")
    return report


def clearing_patches(expected):
    # Original updateText preserves the first matching FEFF for the leading case.
    first=dict(action='del',path=['text',1 if expected.reuse_content=='feff-leading' else 0])
    if len(expected.source_text)>1:first['length']=len(expected.source_text)
    return [first]+(typed.mutation_patches(expected.user_updates) if expected.user_updates else [])


def typed_write(value, original, expected):
    require(value.get("clearsChild") is True and value.get("kind") == expected.mode and value.get("httpFactoryExecuted") is False and value.get("formTitle") == "Fixture title"
            and value.get("parent") == "paragraph" and type(value.get("sequence")) is int and value["sequence"] == 1
            and re.fullmatch(r"[0-9a-f]{12}", value.get("session", "")), "Typed pair request identity changed")
    child = value.get("child"); is_reuse = expected.mode == "reuse"
    require(child == expected.child if is_reuse else isinstance(child, str) and re.fullmatch(r"temp:C:xyz[0-9a-f]{25}", child), "Typed child identity changed")
    data = decoded(value.get("documentData")); shared.rich_title(data)
    require(tags(data) == [1, 1, 14], "Typed Save is not one whole pair")
    parent, tail = shared.messages(data, 1)
    require(tags(parent) == [1, 2, 13, 19, 20, 33, 38] and text(parent, 1) == "paragraph" and field(parent, 2, 0) == 1000, "Typed parent not unchanged STATUS projection")
    require(tags(tail) == ([1, 2, 9, 12, 13, 19, 20, 33, 38] if is_reuse else [1, 2, 3, 8, 9, 10, 11, 12, 13, 16, 19, 20, 21, 29, 33, 35, 38])
            and text(tail, 1) == child and field(tail, 2, 0) == (1000 if is_reuse else 0)
            and field(tail, 9, 0) == 0 and shared.body(tail, False) == expected.updates[-1], "Typed child full CONTENT projection/body changed")
    for row in (parent, tail):
        require(field(row, 13, 2) == b"" and field(row, 19, 0) == field(row, 20, 0) == 1 and text(row, 33) == value["session"] + "-0", "Typed trace/status changed")
    if not is_reuse:
        require(text(tail, 8) == text(tail, 21) == "aad:temp" and field(tail, 16, 2) == b"\x20\x00"
                and all(field(tail, tag, 0) == 0 for tag in (3, 10, 11, 29, 35)), "Typed new child creation fields changed")
    before = wire(field(original["paragraph"], 38, 2)); parent_crdt = wire(field(parent, 38, 2)); current = wire(field(tail, 38, 2))
    require(shared.stable(parent_crdt) == shared.stable(before, (4, 7, 12)) and not any(f[0] in (4, 7, 10, 12) for f in parent_crdt), "Typed pair fabricated parent history")
    require(text(current, 1) == child and field(current, 2, 0) == (expected.base if is_reuse else 0) and field(current, 3, 0) == (1000 if is_reuse else 0)
            and not any(f[0] in (4, 7, 10) for f in current), "Typed child CRDT owner/base changed")
    source = wire(field(original[child], 38, 2)) if is_reuse else None
    source_session(original[child],expected)
    processed = [f[2] for f in current if f[0] == 6]
    if is_reuse:
        require(shared.stable(current, (5, 6, 12)) == shared.stable(source, (4, 5, 6, 7, 12)), "Typed reuse lost original session/base metadata")
        prior = [f[2] for f in source if f[0] == 6]
        require(processed == prior + [field(current, 12, 2)] + prior, "Typed reuse lost ordered duplicate save heads")
    else:
        require(tags(current) == [1, 2, 3, 5, 6, 8, 12] and processed == [field(current, 12, 2)], "Typed new child head projection changed")
    semantic_history(current, source, expected, value["session"], new=not is_reuse)
    return child, parent, tail


def journal(value, expected, write=None, original=None):
    require(isinstance(value, dict) and set(value) == {"scope", "receipt"} and value["scope"] == shared.SCOPE, "Typed durable scope changed")
    if write is None:
        require(value["receipt"] is None, "Unexpected retained typed lock"); return
    receipt = value["receipt"]
    require(isinstance(receipt, dict) and isinstance(receipt.get("operation"), str) and receipt["operation"], "Typed operation identity missing")
    c = receipt.get("checkpoint", {})
    require(set(c) == {"version", "scope", "thread", "document", "target", "session", "sequence", "action", "draft", "sections", "request"}
            and type(c.get("version")) is int and c["version"] == 7 and c.get("scope") == shared.SCOPE and c.get("thread") == "thread"
            and c.get("document") == "document" and c.get("target") == "paragraph" and c.get("session") == write["session"]
            and type(c.get("sequence")) is int and c["sequence"] == 1 and c.get("draft") == expected.wire_seed, "Clearing v7 checkpoint owner/original parent changed")
    require(c["action"] == {"EndClearing": dict(child=write["child"], position="aag" if expected.mode == "reuse" else "aad:temp",
              offset_utf16=expected.offset, child_updates=list(expected.user_updates))}, "Typed checkpoint lost ordered updates/end intent")
    action = c["action"]["EndClearing"]
    require(type(action["offset_utf16"]) is int
            and isinstance(action["child_updates"], list) and all(type(s) is str for s in action["child_updates"]), "Typed checkpoint action scalar types changed")
    require(shared.byte_vector(c["request"]) == base64.b64decode(write["documentData"], validate=True), "Typed durable request differs from sent whole pair")
    prior = {r["id"]: base64.b64decode(r["raw"], validate=True) for r in original["sections"]}
    require(isinstance(c["sections"], list) and len(c["sections"]) == len(prior)
            and {text(wire(shared.byte_vector(r)), 1): shared.byte_vector(r) for r in c["sections"]} == prior, "Typed durable original inventory changed")


def witness(value,expected):
    if expected.typing!='none':return typed.witness(value,expected)
    end.witness(value,expected.corpus)
    require(value.get('typing') is None,'Untyped clearing gained user input ledger')
    return value


def validate(receipt, corpus="ascii", typing="none", reuse_base="positive", reuse_content="spaces"):
    mode="reuse"; expected = config(corpus, typing, reuse_base, reuse_content); labels(receipt, expected)
    scenario = receipt.get("scenario")
    require(scenario in shared.STAGES and all(receipt.get(k) is True for k in ("ok", "noSession", "noTransport", "trustedNativeEnter"))
            and receipt.get("httpFactoryExecuted") is False, "Typed scenario isolation failed")
    original = receipt.get("original", {}); labels(original, expected)
    initial = original.get("fresh"); initial_rows = inventory(initial, expected)
    require(original.get("visible") == initial and original.get("pending") is False and original.get("operation") is None
            and original.get("stale") is False and original.get("loading") is False and original.get("reservations") == 0
            and original.get("capacityReason") is None, "Typed original state not ready")
    keys = ("preflights", "writes", "acknowledgements", "readbacks")
    require(all(original.get(k) == [] for k in keys), "Typed fixture already dispatched")
    journal(receipt.get("initialJournal"), expected)
    cancelled, saved = receipt.get("cancelledWitness"), receipt.get("savedWitness")
    witness(cancelled, expected); witness(saved, expected)
    require(cancelled["visit"] == saved["visit"] and cancelled["generation"] != saved["generation"]
            and receipt.get("duplicate") == {"ok": True, "clicks": 2}, "Typed native owner/duplicate Save guard missing")
    names = (("split" if typing=="none" else "typed-child"),) + shared.STAGES[scenario][1:]; stages = receipt.get("stages")
    require(isinstance(stages, list) and len(stages) == len(names), "Typed lifecycle stages incomplete")
    previous = {k: [] for k in keys}; owners = {cancelled["generation"], saved["generation"]}
    child = expected.child if mode == "reuse" else None
    sent = pair_rows = first_journal = None
    for index, (stage, name) in enumerate(zip(stages, names)):
        require(stage.get("stage") == name and type(stage.get("ordinal")) is int and stage["ordinal"] == index
                and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport")), "Typed stage identity changed")
        source, geometry = stage.get("fixture", {}), stage.get("geometry", {}); labels(source, expected)
        counts = (0, 0, 0, 0) if index < 2 else ((1, 0, 0, 0) if scenario == "stale" else
                 (1, 1, 0, 0) if index == 2 else (1, 1, 1, 0) if scenario == "partial" else
                 (1, 1, 1, 1) if index < 5 else (2, 2, 1, 1) if index == 5 else (2, 2, 2, 2))
        for key, count in zip(keys, counts):
            values = source.get(key)
            require(isinstance(values, list) and len(values) == count and values[:len(previous[key])] == previous[key], "Typed immutable ledger changed: " + key)
            previous[key] = values
        require(source.get("scenario") == scenario and source.get("visit") == original.get("visit") and type(source.get("read")) is int
                and source["read"] > 0 and source.get("loading") is False and type(source.get("reservations")) is int
                and source["reservations"] == int(scenario != "stale" and index >= 2), "Typed ownership/dispatch reservation changed")
        if source["preflights"]:
            first = source["preflights"][0]
            require(first.get("source") == initial and first.get("outcome") == ("Changed" if scenario == "stale" else "prepared"), "Typed preflight not original source")
            fresh = inventory(first.get("fresh"), expected, initial, stale=scenario == "stale")
            if scenario != "stale": require(first["fresh"] == initial, "Typed source changed before dispatch")
            else:
                identity = expected.child if mode == "reuse" else "paragraph"
                require(shared.stable(fresh[identity], (2,)) == shared.stable(initial_rows[identity], (2,))
                        and all(shared.raw(fresh[k]) == shared.raw(v) for k, v in initial_rows.items() if k != identity), "Typed stale source changed beyond intended revision")
        if source["writes"] and sent is None:
            child, parent, tail = typed_write(source["writes"][0], initial_rows, expected); sent = [parent, tail]
        accepted = scenario == "success" and index >= 3
        current = inventory(source.get("fresh"), expected, initial, child, accepted, index == 6, scenario == "stale" and index >= 2)
        inventory(source.get("visible"), expected, initial, child, accepted, index == 6)
        if not accepted:
            require(source["visible"] == initial and source["fresh"] == (source["preflights"][0]["fresh"] if scenario == "stale" and index >= 2 else initial), "Typed unaccepted operation changed canonical/visible source")
        if source["acknowledgements"]:
            pair_rows = ack(source["acknowledgements"][0], sent[:1] if scenario == "partial" else sent, initial_rows, expected, partial=scenario == "partial")
        if accepted:
            first_read = inventory(source["readbacks"][0], expected, initial, child, True)
            require(all(shared.raw(first_read[text(row, 1)]) == shared.raw(row) for row in pair_rows), "Typed independent readback differs from ACK")
            require(source["fresh"] == source["visible"] == source["readbacks"][-1], "Typed readback source custody differs")
        if index >= 5:
            pair = {text(row, 1): row for row in pair_rows}; row = continuation(source["writes"][1], child, pair[child], expected)
            require(source["preflights"][1].get("source") == source["preflights"][1].get("fresh") == source["readbacks"][0]
                    and source["preflights"][1].get("outcome") == "prepared", "Typed continuation preflight changed")
            if index == 6:
                rows = ack(source["acknowledgements"][1], [row], pair, expected, continued=True)
                require(shared.raw(current[child]) == shared.raw(rows[0]) and shared.raw(current["paragraph"]) == shared.raw(pair["paragraph"]), "Typed ordinary child continuation changed parent/readback")
        pending, uncertain = name in ("saving", "continued-saving"), name == "uncertain"
        require(source.get("pending") is pending and source.get("stale") is (pending or uncertain), "Typed pending/stale state changed")
        if pending or uncertain: require((source.get("operation") or {}).get("stage") == ("Saving" if pending else "Uncertain"), "Typed operation lock missing")
        else: require(source.get("operation") is None, "Typed unexpected retained operation")
        held = name == "saving" or uncertain
        journal(stage.get("journal"), expected, source["writes"][0] if held else None, initial)
        if name == "saving": first_journal = stage["journal"]
        if uncertain: require(stage["journal"] == first_journal, "Typed unknown outcome lost whole ordered operation")
        require(geometry.get("ok") is True and geometry.get("noOverflow") is True, "Typed layout overflow")
        typed_editor = name in ("split", "typed-child", "saving", "refused", "uncertain")
        parts = [expected.seed, expected.updates[-1]] if typed_editor else [expected.updates[-1]] if name == "child-editing" else [expected.continued] if name == "continued-saving" else None
        if parts is not None:
            editor = geometry.get("native")
            shared.native(editor, parts, not (pending or uncertain), shared.ERROR if name == "refused" else None)
            if typed_editor:
                owner = cancelled if index == 0 else saved; typed = owner if typing=="none" else owner["typing"]
                require(all(editor.get(k) == owner[k] for k in ("generation", "visit")) and editor.get("typing") == (None if typing=="none" else typed)
                        and editor.get("blocks") == typed["after"] and editor.get("rawBlocks") == typed["rawAfter"]
                        and editor.get("placeholder") == typed["placeholder"], "Typed held draft lost sealed native event association")
            else:
                require(editor.get("rawBlocks") == editor.get("blocks") and editor.get("placeholder") is None and editor.get("typing") is None, "Ordinary child reopened with stale typed witness")
                generation = editor.get("generation")
                require(isinstance(generation, str) and generation.isascii() and generation.isdigit() and generation not in owners
                        and editor.get("visit") == saved["visit"], "Typed child editor recycled old owner")
                owners.add(generation)
        else:
            paragraphs = [dict(id="paragraph", text=expected.seed)]
            if accepted or mode == "reuse": paragraphs.append(dict(id=child, text=expected.continued if index == 6 else expected.updates[-1] if accepted else expected.reader_text))
            neighbor = dict(id="neighbor", text="Another paragraph")
            paragraphs = [neighbor, *paragraphs] if mode == "reuse" else [*paragraphs, neighbor]
            require(geometry.get("native") is None and geometry.get("paragraphs") == paragraphs, "Typed reader content/order changed")
        if name == "cancelled": require(source == original, "Typed Cancel mutated source or dispatched")
    require(receipt.get("fixture") == stages[-1]["fixture"] and receipt.get("layout") == stages[-1]["geometry"]
            and receipt.get("finalJournal") == stages[-1]["journal"], "Typed terminal differs from final original stage")
