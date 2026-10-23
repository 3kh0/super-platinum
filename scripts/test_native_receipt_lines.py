"""NDJSON uses LF framing; Unicode separators inside JSON are content."""
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest

from native_fixture import NativeFixture
from canvas_evidence import closed_receipts


class ReceiptLines(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.log = Path(self.temp.name) / 'fixture.log'
        self.native = SimpleNamespace(log=self.log)
        self.prefix = 'fixture-stage: '

    def live(self):
        return NativeFixture.receipts(self.native, self.prefix)

    def test_unicode_separators_are_content_for_lf_and_crlf_records(self):
        rows = [{'text': '\u0085\u2028\u2029\ufeff'}, {'text': 'escaped\nnewline'}]
        for ending in ('\n', '\r\n'):
            self.log.write_bytes(('noise' + ending + ''.join(
                self.prefix + json.dumps(row, ensure_ascii=False) + ending for row in rows)).encode('utf-8'))
            self.assertEqual(self.live(), rows)
            self.assertEqual(closed_receipts(self.log, self.prefix), rows)

    def test_live_ignores_incomplete_record_closed_reader_refuses_it(self):
        first = {'text': '\u2028'}
        self.log.write_bytes((self.prefix + json.dumps(first, ensure_ascii=False) + '\n'
                              + self.prefix + '{"text":"partial\u2029').encode('utf-8'))
        self.assertEqual(self.live(), [first])
        with self.assertRaisesRegex(RuntimeError, 'Incomplete final log receipt'):
            closed_receipts(self.log, self.prefix)

    def test_complete_malformed_json_still_fails(self):
        self.log.write_bytes((self.prefix + '{broken}\n').encode())
        with self.assertRaises(json.JSONDecodeError): self.live()
        with self.assertRaises(json.JSONDecodeError): closed_receipts(self.log, self.prefix)

    def test_duplicate_receipts_remain_visible_to_ownership_validator(self):
        row = {'ok': True}
        self.log.write_bytes(((self.prefix + json.dumps(row) + '\n') * 2).encode())
        self.assertEqual(self.live(), [row, row])
        self.assertEqual(closed_receipts(self.log, self.prefix), [row, row])


if __name__ == '__main__':
    unittest.main()
