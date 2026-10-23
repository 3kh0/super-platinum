"""Independent immutable expected content for the two bounded split corpora."""
from dataclasses import dataclass
from types import MappingProxyType


@dataclass(frozen=True)
class Corpus:
    name: str
    seed: str
    left: str
    right: str
    edited: str
    wire_seed: str
    wire_left: str
    wire_right: str
    wire_edited: str
    visible_offset: int
    wire_offset: int


CORPORA = MappingProxyType({
    "ascii": Corpus("ascii", "Keep thisSecond paragraph", "Keep this", "Second paragraph", "Keep this edited",
                    "Keep thisSecond paragraph", "Keep this", "Second paragraph", "Keep this edited", 9, 9),
    "unicode": Corpus("unicode", "Café & 日本😀Second paragraph", "Café & 日本😀", "Second paragraph", "Café & 日本😀 edited",
                      "Café &amp; 日本😀Second paragraph", "Café &amp; 日本😀", "Second paragraph", "Café &amp; 日本😀 edited", 11, 15),
})


def corpus_config(name="ascii"):
    if not isinstance(name, str) or name not in CORPORA:
        raise RuntimeError("Unknown pinned split corpus")
    return CORPORA[name]


def require_corpus(value, name):
    # Old accepted native receipts predate the explicit ASCII corpus label.
    if value.get("corpus", "ascii") != name:
        raise RuntimeError("Receipt differs from independently selected split corpus")
