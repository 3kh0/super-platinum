"""Input authorization is strict before any browser action is attempted."""
import unittest

from canvas_split_input import validate_request


class EnterAdmission(unittest.TestCase):
    def setUp(self):
        self.nonce = "a" * 32
        self.value = dict(ok=True, fixture="canvas-structure", nonce=self.nonce,
                          ordinal=0, fieldId="canvas-text-draft", generation="3", visit="0",
                          seed="Keep thisSecond paragraph", offset=9, revision=0)

    def test_two_owned_requests_and_zero_visit(self):
        for ordinal in (0, 1):
            validate_request(dict(self.value, ordinal=ordinal), "canvas-structure", self.nonce, ordinal)

    def test_typing_requires_independent_finite_caller_authority(self):
        for mode in ("end", "reuse"):
            for typing in ("correction", "erase"):
                value = dict(self.value, mode=mode, typing=typing, seed="Keep this")
                validate_request(value, "canvas-structure", self.nonce, 0, "ascii", mode, typing)
                for caller in ("none", "arbitrary", "erase" if typing == "correction" else "correction"):
                    with self.subTest(mode=mode, typing=typing, caller=caller), self.assertRaises(RuntimeError):
                        validate_request(value, "canvas-structure", self.nonce, 0, "ascii", mode, caller)
                with self.assertRaises(RuntimeError):
                    validate_request(dict(value, typing=None), "canvas-structure", self.nonce, 0,
                                     "ascii", mode, typing)
        validate_request(dict(self.value, typing="correction"), "canvas-structure", self.nonce,
                         0, "ascii", "middle", "correction")

    def test_middle_typing_pins_expanded_unicode_tail_and_restore_scope(self):
        for corpus, seed, offset in [("ascii", "Keep thisSecond paragraph", 9),
                ("unicode", "Caf\u00e9 & \u65e5\u672c\U0001f600Second \u65e5\u672c\U0001f600 & paragraph", 11)]:
            for typing in ("correction", "erase", "restore"):
                value = dict(self.value, corpus=corpus, mode="middle", typing=typing, seed=seed, offset=offset)
                validate_request(value, "canvas-structure", self.nonce, 0, corpus, "middle", typing)
                for key, changed in (("seed", seed+"x"), ("offset", offset+1), ("typing", "none"), ("mode", "end")):
                    with self.subTest(corpus=corpus, typing=typing, key=key), self.assertRaises(RuntimeError):
                        validate_request(dict(value, **{key:changed}), "canvas-structure", self.nonce, 0, corpus, "middle", typing)
        for mode in ("end", "reuse"):
            with self.assertRaises(RuntimeError):
                validate_request(dict(self.value, mode=mode, typing="restore", seed="Keep this"), "canvas-structure", self.nonce, 0, "ascii", mode, "restore")

    def test_end_enter_is_caller_pinned_and_cannot_borrow_middle_authority(self):
        for corpus, seed, offset in (("ascii", "Keep this", 9),
                                     ("unicode", "Caf\u00e9 & \u65e5\u672c\U0001f600", 11)):
            value = dict(self.value, mode="end", corpus=corpus, seed=seed, offset=offset)
            for ordinal in (0, 1):
                validate_request(dict(value, ordinal=ordinal), "canvas-structure", self.nonce,
                                 ordinal, corpus, "end")
            for key, changed in (("mode", "middle"), ("mode", None),
                                 ("seed", seed + "Second paragraph"),
                                 ("seed", seed + "x"), ("offset", offset - 1),
                                 ("offset", offset + 1), ("revision", 1)):
                with self.subTest(corpus=corpus, key=key), self.assertRaises(RuntimeError):
                    validate_request(dict(value, **{key: changed}), "canvas-structure",
                                     self.nonce, 0, corpus, "end")
            with self.assertRaises(RuntimeError):
                validate_request(value, "canvas-structure", self.nonce, 0, corpus)
            with self.assertRaises(RuntimeError):
                validate_request(value, "canvas-structure", self.nonce, 0, corpus, "unknown")

    def test_reuse_enter_cannot_borrow_new_child_authority(self):
        for corpus, seed, offset in (("ascii", "Keep this", 9),
                                     ("unicode", "Caf\u00e9 & \u65e5\u672c\U0001f600", 11)):
            value = dict(self.value, mode="reuse", corpus=corpus, seed=seed, offset=offset)
            for ordinal in (0, 1):
                validate_request(dict(value, ordinal=ordinal), "canvas-structure", self.nonce,
                                 ordinal, corpus, "reuse")
            for caller in ("middle", "end", "unknown"):
                with self.subTest(caller=caller), self.assertRaises(RuntimeError):
                    validate_request(value, "canvas-structure", self.nonce, 0, corpus, caller)
            for key, changed in (("mode", "end"), ("mode", None), ("seed", seed + "x"),
                                 ("offset", offset - 1), ("offset", offset + 1), ("revision", 1)):
                with self.subTest(key=key), self.assertRaises(RuntimeError):
                    validate_request(dict(value, **{key: changed}), "canvas-structure",
                                     self.nonce, 0, corpus, "reuse")

    def test_start_enter_pins_full_seed_zero_caret_and_explicit_typing(self):
        for corpus, seed in (("ascii", "Keep thisSecond paragraph"),
                             ("unicode", "Caf\u00e9 & \u65e5\u672c\U0001f600Second paragraph")):
            value = dict(self.value, corpus=corpus, mode="start", seed=seed, offset=0)
            for ordinal in (0, 1):
                validate_request(dict(value, ordinal=ordinal), "canvas-structure", self.nonce,
                                 ordinal, corpus, "start")
            for key, changed in (("offset", 1), ("offset", 9), ("seed", seed+"x"),
                                 ("mode", "middle"), ("mode", "end"), ("mode", None),
                                 ("typing", "correction"), ("revision", 1)):
                with self.subTest(corpus=corpus,key=key), self.assertRaises(RuntimeError):
                    validate_request(dict(value, **{key:changed}), "canvas-structure", self.nonce,
                                     0, corpus, "start")
            for typing in ("correction", "erase", "restore"):
                validate_request(dict(value, typing=typing), "canvas-structure", self.nonce,
                                 0, corpus, "start", typing)
                with self.assertRaises(RuntimeError):
                    validate_request(dict(value, typing=typing), "canvas-structure", self.nonce,
                                     0, corpus, "start", "none")

    def test_identity_replacement_is_not_input_authority(self):
        for key, value in (("nonce", "b" * 32), ("fixture", "home"), ("fieldId", "composer"),
                           ("generation", ""), ("visit", "-1"), ("generation", "\u0663")):
            with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                validate_request(dict(self.value, **{key: value}), "canvas-structure", self.nonce, 0)

    def test_unwitnessed_or_replayed_request_is_refused(self):
        for key, value in (("ok", False), ("seed", "Changed"), ("offset", 8), ("offset", 9.0),
                           ("revision", 1), ("revision", False), ("ordinal", True), ("ordinal", 1)):
            with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                validate_request(dict(self.value, **{key: value}), "canvas-structure", self.nonce, 0)
        with self.assertRaises(RuntimeError):
            validate_request(dict(self.value, ordinal=2), "canvas-structure", self.nonce, 2)

    def test_unicode_corpus_requires_independent_exact_visible_utf16_authority(self):
        value = dict(self.value, corpus="unicode", seed="Café & 日本😀Second paragraph", offset=11)
        for ordinal in (0, 1):
            validate_request(dict(value, ordinal=ordinal), "canvas-structure", self.nonce, ordinal, "unicode")
        for key, replacement in (("corpus", "ascii"), ("seed", "Cafe\u0301 & 日本😀Second paragraph"),
                                 ("seed", "Café &amp; 日本😀Second paragraph"), ("offset", 10),
                                 ("offset", 15), ("offset", 18), ("offset", 11.0), ("offset", True)):
            with self.subTest(key=key, value=replacement), self.assertRaises(RuntimeError):
                validate_request(dict(value, **{key: replacement}), "canvas-structure", self.nonce, 0, "unicode")
        with self.assertRaises(RuntimeError):
            validate_request(value, "canvas-structure", self.nonce, 0)
        with self.assertRaises(RuntimeError):
            validate_request(self.value, "canvas-structure", self.nonce, 0, "unicode")
        with self.assertRaises(RuntimeError):
            validate_request(value, "canvas-structure", self.nonce, 0, "unknown")


if __name__ == "__main__":
    unittest.main()
