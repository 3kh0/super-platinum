"""Portable adversarial ordinary-root evidence tests; no native execution."""
import copy
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

import canvas_comment_root_bodies as O


def sample():
    bodies = {}
    for ts, (text, painted, mentions) in O.expected().items():
        bodies[ts] = {'ts': ts, 'text': text, 'painted': painted, 'mentions': mentions, 'links': []}
    for ts in (O.REPLY_TS, O.MORE_TS):
        bodies[ts] = {'ts': ts, 'text': 'Ordinary reply', 'painted': [], 'mentions': [], 'links': []}
    ids = [[O.RICH_TS, O.FALLBACK_TS], [O.RICH_TS, O.REPLY_TS], [O.MORE_TS],
           [O.RICH_TS, O.REPLY_TS], [O.LITERAL_TS, O.HIDDEN_TS], [], [], [O.RICH_TS, O.FALLBACK_TS], [O.OLDER_TS]]
    return copy.deepcopy({'responses': O.responses(), 'layouts': [
        {'bodyProof': {'ok': True, 'rows': [bodies[ts] for ts in group], 'threadButtons': group if n not in (1, 2, 3) else []}} for n, group in enumerate(ids)]})


class RootBodiesTests(unittest.TestCase):
    def refuse(self, mutation):
        receipt = sample()
        mutation(receipt)
        with self.assertRaises(RuntimeError):
            O.validate(receipt)

    def test_explicit_carriers_and_all_stages(self):
        O.validate(sample())

    def test_rich_block_cannot_be_replaced_by_text_decoy(self):
        self.refuse(lambda r: r['layouts'][0]['bodyProof']['rows'][0].update(text='DECOY text must not render'))

    def test_inner_underline_is_actual_computed_paint(self):
        self.refuse(lambda r: r['layouts'][0]['bodyProof']['rows'][0]['painted'][0].__setitem__(3, False))

    def test_bold_is_actual_computed_paint(self):
        self.refuse(lambda r: r['layouts'][0]['bodyProof']['rows'][0]['painted'][0].__setitem__(1, False))

    def test_underline_does_not_spill_into_plain_suffix(self):
        self.refuse(lambda r: r['layouts'][0]['bodyProof']['rows'][0]['painted'][len(O.RICH)].__setitem__(3, True))

    def test_mention_is_a_rendered_chip(self):
        self.refuse(lambda r: r['layouts'][0]['bodyProof']['rows'][0].update(mentions=[]))

    def test_rich_identity_does_not_change_in_thread(self):
        self.refuse(lambda r: r['layouts'][1]['bodyProof']['rows'][0].update(ts=O.FALLBACK_TS))

    def test_root_body_excludes_last_reply_preview(self):
        self.refuse(lambda r: r['layouts'][0]['bodyProof']['rows'][0].update(text=O.RICH + ' for ' + O.MENTION + 'Ordinary reply'))

    def test_fallback_mrkdwn_marks_are_not_literal(self):
        self.refuse(lambda r: r['layouts'][0]['bodyProof']['rows'][1].update(text='Fallback *bold* & \u65e5\U0001f600'))

    def test_fallback_decodes_entities(self):
        self.refuse(lambda r: r['layouts'][0]['bodyProof']['rows'][1].update(text=O.FALLBACK.replace('&', '&amp;')))

    def test_markdown_text_requires_actual_italic(self):
        self.refuse(lambda r: r['layouts'][8]['bodyProof']['rows'][0]['painted'][len('An older thread with ')].__setitem__(2, False))

    def test_plain_mode_keeps_typed_mention(self):
        self.refuse(lambda r: r['layouts'][4]['bodyProof']['rows'][0].update(mentions=[]))

    def test_plain_mode_cannot_apply_formatting(self):
        self.refuse(lambda r: r['layouts'][4]['bodyProof']['rows'][0]['painted'][10].__setitem__(1, True))

    def test_plain_mode_keeps_emoji_expansion(self):
        self.refuse(lambda r: r['layouts'][4]['bodyProof']['rows'][0].update(text=O.LITERAL.replace('\U0001f44b', ':wave:')))

    def test_escaped_mention_is_not_reparsed(self):
        self.refuse(lambda r: r['layouts'][4]['bodyProof']['rows'][0].update(mentions=[O.MENTION, O.MENTION]))

    def test_hidden_root_keeps_identity_and_deleted_placeholder(self):
        self.refuse(lambda r: r['layouts'][4]['bodyProof']['rows'][1].update(text='DECOY hidden text must not render'))

    def test_hidden_root_cannot_silently_disappear(self):
        self.refuse(lambda r: r['layouts'][4]['bodyProof']['rows'][1].update(text='', painted=[]))

    def test_hidden_root_retains_thread_button(self):
        self.refuse(lambda r: r['layouts'][4]['bodyProof']['threadButtons'].pop())

    def test_trailing_space_within_italic_is_preserved(self):
        self.refuse(lambda r: r['layouts'][0]['bodyProof']['rows'][1]['painted'][-1].__setitem__(2, False))

    def test_unread_remains_empty(self):
        self.refuse(lambda r: r['layouts'][5]['bodyProof']['rows'].append(r['layouts'][0]['bodyProof']['rows'][0]))

    def test_no_mutated_synthetic_source_blocks(self):
        self.refuse(lambda r: r['responses'][0]['messages'][0].update(blocks=[]))

    def test_original_failed_read_stays_explicit(self):
        self.refuse(lambda r: r['responses'].__setitem__(8, {'ok': True, 'messages': []}))

    def test_page_and_thread_root_raw_bytes_shape_equal(self):
        self.assertEqual(O.responses()[0]['messages'][0], O.responses()[2]['messages'][0])

    def test_closed_capture_proof_must_match_terminal(self):
        path = Path(__file__).with_name('check-canvas-comments-native.py')
        spec = importlib.util.spec_from_file_location('canvas_comments_root_runner', path)
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        receipt = sample()
        stages = [{'layout': copy.deepcopy(v)} for v in receipt['layouts']]
        with patch.object(runner, '_base_validate_closed') as base:
            runner.validate_closed(runner.SPEC, 'canvas-comments', None, stages, receipt)
            base.assert_called_once()
            stages[0]['layout']['bodyProof']['rows'][0]['text'] = 'changed after capture'
            with self.assertRaises(RuntimeError):
                runner.validate_closed(runner.SPEC, 'canvas-comments', None, stages, receipt)


if __name__ == '__main__':
    unittest.main()
