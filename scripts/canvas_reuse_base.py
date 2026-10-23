"""Immutable, caller-pinned existing-child cohorts; zero never means creation."""
from dataclasses import dataclass
import hashlib

from canvas_heading import require, wire, field, text


ZERO_CHILD = "temp:C:Dte0000000000000000000000000"
ZERO_HEAD = "3843ec896a6912a5e7f3c9965b1e2a083e45abb2b4249b5f131c38f286c7d4ee"
ZERO_HISTORY_SHA = "85179669576fea0aeac68eea97505ec141d685e8ba2fe68ce51335a9a33554b1"


@dataclass(frozen=True)
class Cohort:
    name: str
    child: str
    base: int

    @property
    def ids(self): return ("title", "neighbor", "paragraph", self.child)

    @property
    def pair(self): return ("paragraph", self.child)


def config(reuse_base="positive"):
    if reuse_base == "positive": return Cohort("positive", "empty-final", 1000)
    if reuse_base == "zero": return Cohort("zero", ZERO_CHILD, 0)
    raise RuntimeError("Unpinned existing-child base cohort")


def labels(value, cohort):
    require(value.get("reuseBase", "positive") == cohort.name, "Existing-child base selector mismatch")
    if cohort.name == "zero":
        require(value.get("reuseBase") == "zero" and value.get("reuseChild") == ZERO_CHILD, "Zero-base cohort lacks explicit stable modern child identity")
    elif "reuseChild" in value:
        expected = cohort.child if value.get("splitMode", value.get("mode")) == "reuse" else None
        require(value["reuseChild"] == expected, "Positive-base child identity label changed")


def zero_source(row):
    """Pinned retained original Session, with explicit fixture host/lease binding."""
    require(text(row, 1) == ZERO_CHILD and field(row, 2, 0) == 1000, "Zero-base child must already be canonical at positive revision")
    current = wire(field(row, 38, 2))
    require(sorted(f[0] for f in current) == [1, 2, 3, 4, 5, 6, 8, 11]
            and text(current, 1) == ZERO_CHILD and field(current, 2, 0) == 0 and field(current, 3, 0) == 1000
            and field(current, 4, 0) == 8_000_000_000_000_000 and not any(f[0] in (7, 10, 12) for f in current), "Zero-base retained session/lease changed")
    require(hashlib.sha256(field(current, 5, 2)).hexdigest() == ZERO_HISTORY_SHA
            and text(current, 8) == ZERO_HEAD and [f[2] for f in current if f[0] == 6] == [ZERO_HEAD.encode()], "Zero-base existing history/head is not retained original empty source")
    sequences = [wire(f[2]) for f in current if f[0] == 11]
    require(len(sequences) == 1 and text(sequences[0], 1) == ZERO_HEAD and field(sequences[0], 2, 0) == 1000,
            "Zero-base source lost retained acknowledged sequence head")


def continuation_history(current, before, expected_text, session):
    # Deferred import avoids changing legacy positive-base oracle dependencies.
    # This is the same read-only original-runtime decoder used by typed checks.
    from canvas_end_typing_oracle import decode_history
    now, prior = decode_history(field(current, 5, 2), field(before, 5, 2))
    require(len(now["changes"]) > len(prior["changes"])
            and [c["raw"] for c in now["changes"][:len(prior["changes"])]] == [c["raw"] for c in prior["changes"]]
            and now["heads"] == [text(current, 12)] and now["changes"][-1]["text"] == expected_text + "\ufeff",
            "Zero-base ordinary continuation lost actual original history/final text")
    actor = session + "00000000"; previous = prior["changes"][-1]
    sequence = max((c["seq"] for c in prior["changes"] if c["actor"] == actor), default=0)
    for change in now["changes"][len(prior["changes"]):]:
        sequence += 1
        require(change["actor"] == actor and change["seq"] == sequence and change["deps"] == [previous["hash"]]
                and change["deleted"] is False and change["keys"] == ["deleted", "text"] and change["marks"] == [],
                "Zero-base ordinary continuation actor/plain history changed")
        previous = change
