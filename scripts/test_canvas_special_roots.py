"""Portable negative evidence tests; no native process or live transport."""
import copy
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
import canvas_special_roots as O


def rect(x=10, y=10, width=100, height=20):
    return dict(x=x, y=y, width=width, height=height, right=x+width, bottom=y+height)


def read(method='history', ts=None, cursor=None):
    fields = {'channel': 'Cfixture', 'inclusive': 'true',
              'limit': '20' if method == 'history' else '28', 'cached_latest_updates': '{}'}
    if method == 'history':
        fields.update(document_stream='latest_reply', ignore_replies='true', include_pin_count='true',
                      no_user_profile='true', include_stories='true', include_free_team_extra_messages='true',
                      include_date_joined='false')
    else:
        fields['ts'] = ts
    if cursor is not None:
        fields['cursor'] = cursor
    return {'url': 'https://fixture.slack.com/api/conversations.' + method,
            'method': 'POST', 'fields': [list(p) for p in fields.items()]}


def sample(viewport=(668, 380)):
    request_list = [read('replies', O.HIDDEN) if n in (2, 3)
                    else read(cursor='special-controls' if n == 5 else None) for n in range(8)]
    text = {O.HIDDEN: O.DELETED, O.REPLY: 'Visible reply remains readable.'}
    text.update({ts: label + ' & \u4e2d\U0001f98a' for ts, label in zip(O.CONTROLS, O.LABELS)})
    stages = []
    def visible(value):
        return dict(text=value, rect=rect(), inView=True, hit=True)
    for n, (name, count) in enumerate(zip(O.STAGES, O.READS)):
        ids = ([O.HIDDEN], [O.HIDDEN, O.REPLY], [O.HIDDEN, O.REPLY], list(O.CONTROLS),
               list(O.CONTROLS), list(O.CONTROLS), [], [O.HIDDEN])[n]
        target = (O.HIDDEN, O.HIDDEN, O.HIDDEN, *O.CONTROLS, None, O.HIDDEN)[n]
        rows = [{'ts': ts, 'text': text[ts], 'body': visible(text[ts]), 'links': [], 'images': [],
                 'attachments': [], 'threadButton': None if n in (1, 2) else dict(visible('Open thread'), disabled=False)} for ts in ids]
        alerts = ['Comments could not be loaded. Your canvas is unchanged. Retry when ready.'] if n == 6 else []
        body = {'ok': True, 'targetTs': target, 'target': next((r['body'] for r in rows if r['ts'] == target), visible(alerts[0]) if alerts else None),
                'rows': rows, 'scroll': rect(0, 0, 600, 300), 'alerts': alerts}
        layout = {'ok': True, 'viewport': list(viewport), 'focused': True, 'noOverflow': True,
                  'dialog': rect(0, 0, 650, 350), 'panel': rect(0, 0, 620, 320), 'scrollHeight': 300,
                  'controls': [{'label': label, 'rect': rect(), 'hit': True} for label in ('Back to canvas', 'All comments', 'Refresh comments')],
                  'bodyProof': body}
        proof = {'hidden': O.hidden(), 'controls': O.controls(), 'requests': request_list[:count],
                 'responses': O.responses()[:count], 'media': [{'url': url, 'registered': 0} for url in O.URLS]}
        messages = ([O.hidden()], O.responses()[2]['messages'], O.responses()[3]['messages'], O.controls(),
                    O.controls(), O.controls(), [], [O.hidden()])[n]
        proof['retained'] = {'query': {'kind': 'thread' if n in (1, 2) else 'stream',
                                     'rootTs': O.HIDDEN if n in (1, 2) else None},
                             'serial': count, 'ownerCurrent': True, 'loading': False,
                             'error': alerts[0] if alerts else None,
                             'rows': [{'message': message, 'matchesOriginal': True} for message in messages],
                             'root': O.hidden() if n in (1, 2) else None}
        stages.append({'ok': True, 'ordinal': n, 'stage': name, 'noSession': True,
                       'noTransport': True, 'proof': proof, 'layout': layout})
    return copy.deepcopy({'stages': stages, 'proof': stages[-1]['proof'], 'layout': stages[-1]['layout']})


class BoundaryTests(unittest.TestCase):
    def test_exact_read_shapes(self):
        O.request(read(), 'history')
        O.request(read('history', cursor='controls'), 'history', cursor='controls')
        O.request(read('replies', '1.2'), 'replies', root_ts='1.2')

    def test_wrong_target_and_extra_form_refused(self):
        for key, value in [('channel', 'COTHER'), ('inclusive', 'false'), ('limit', '100'),
                           ('document_stream', 'unread'), ('token', 'unexpected')]:
            r = read()
            r['fields'] = [p for p in r['fields'] if p[0] != key] + [[key, value]]
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                O.request(r, 'history')
        r = read(); r['fields'].append(r['fields'][0])
        with self.assertRaises(RuntimeError):
            O.request(r, 'history')

    def test_foreign_endpoint_method_and_root_refused(self):
        for key, value in [('url', 'https://other.slack.com/api/conversations.history'),
                           ('url', 'https://fixture.slack.com/api/chat.delete'), ('method', 'GET')]:
            r = read(); r[key] = value
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                O.request(r, 'history')
        with self.assertRaises(RuntimeError):
            O.request(read('replies', '2.3'), 'replies', root_ts='1.2')

    def test_geometry_is_finite_and_within_viewport(self):
        self.assertTrue(O.bounded(rect(), [668, 380]))
        for key, value in [('x', float('nan')), ('height', 0), ('width', True),
                           ('bottom', 600), ('right', 500), ('y', -20)]:
            r = rect(); r[key] = value
            with self.subTest(key=key):
                self.assertFalse(O.bounded(r, [668, 380]))


class ReceiptTests(unittest.TestCase):
    def refuse(self, mutate):
        receipt = sample()
        mutate(receipt)
        with self.assertRaises((RuntimeError, ValueError)):
            O.validate(receipt)

    def test_complete_normal_and_narrow_samples(self):
        O.validate(sample((1280, 800)))
        O.validate(sample())

    def test_hidden_placeholder_cannot_leak_or_disappear(self):
        for text in ('', 'DECOY hidden fallback', 'DECOY hidden file', 'Visible reply remains readable.'):
            with self.subTest(text=text):
                self.refuse(lambda r: r['stages'][0]['layout']['bodyProof']['rows'][0].update(text=text))

    def test_hidden_media_and_attachment_dom_refused(self):
        for key, value in [('links', [{'text': 'hidden', 'href': 'https://example.test'}]),
                           ('images', [{'tag': 'IMG', 'src': 'hidden'}]), ('attachments', ['hidden'])]:
            with self.subTest(key=key):
                self.refuse(lambda r: r['stages'][1]['layout']['bodyProof']['rows'][0].update({key: value}))

    def test_actual_registry_must_be_zero_for_every_original_url(self):
        for n in range(7):
            self.refuse(lambda r: r['stages'][2]['proof']['media'][n].update(registered=1))
        for value in (False, None, '0'):
            self.refuse(lambda r: r['stages'][0]['proof']['media'][0].update(registered=value))
        self.refuse(lambda r: r['stages'][0]['proof']['media'].pop())
        self.refuse(lambda r: r['stages'][0]['proof']['media'][0].update(url=O.URLS[1]))

    def test_original_hidden_fields_remain_intact(self):
        for key, value in [('files', []), ('attachments', []), ('blocks', []), ('user', 'U2'), ('ts', O.REPLY)]:
            self.refuse(lambda r: r['stages'][0]['proof']['hidden'].update({key: value}))
        self.refuse(lambda r: r['stages'][0]['proof']['hidden']['document_comment'].update(is_visible=0))
        self.refuse(lambda r: r['stages'][0]['proof']['hidden']['document_comment'].update(thread_id='changed'))

    def test_visibility_null_and_absent_are_not_false(self):
        self.refuse(lambda r: r['stages'][4]['proof']['controls'][1]['document_comment'].update(is_visible=None))
        self.refuse(lambda r: r['stages'][5]['proof']['controls'][2]['document_comment'].pop('is_visible'))
        self.refuse(lambda r: r['stages'][3]['proof']['controls'][0]['document_comment'].update(is_visible=1))

    def test_visible_controls_retain_block_text_without_appended_file(self):
        for n, row in ((3, 0), (4, 1), (5, 2)):
            self.refuse(lambda r: r['stages'][n]['layout']['bodyProof']['rows'][row].update(text=O.DELETED))
            self.refuse(lambda r: r['stages'][n]['layout']['bodyProof']['rows'][row].update(text=O.LABELS[row]))
            self.refuse(lambda r: r['stages'][n]['layout']['bodyProof']['rows'][row].update(
                text=O.LABELS[row] + ' & \u4e2d\U0001f98a' + O.LABELS[row] + ' file'))

    def test_original_thread_action_and_row_identity_retained(self):
        self.refuse(lambda r: r['stages'][0]['layout']['bodyProof']['rows'][0].update(threadButton=None))
        self.refuse(lambda r: r['stages'][0]['layout']['bodyProof']['rows'][0]['threadButton'].update(disabled=True))
        self.refuse(lambda r: r['stages'][0]['layout']['bodyProof']['rows'][0].update(ts=O.REPLY))
        self.refuse(lambda r: r['stages'][1]['layout']['bodyProof']['rows'].reverse())

    def test_refreshed_original_response_cannot_be_rewritten(self):
        self.refuse(lambda r: r['stages'][2]['proof']['responses'][3]['messages'][0].update(files=[]))
        self.refuse(lambda r: r['stages'][2]['proof']['requests'][3]['fields'].append(['ts', 'wrong']))

    def test_error_is_failure_not_empty_success(self):
        self.refuse(lambda r: r['stages'][6]['proof']['responses'].__setitem__(6, {'ok': True, 'messages': []}))
        self.refuse(lambda r: r['stages'][6]['layout']['bodyProof'].update(alerts=[]))
        self.refuse(lambda r: r['stages'][6]['layout']['bodyProof']['target'].update(text='No comments'))

    def test_stage_count_order_and_isolation(self):
        self.refuse(lambda r: r['stages'].pop())
        self.refuse(lambda r: r['stages'][0].update(ordinal=False))
        self.refuse(lambda r: r['stages'][0].update(noTransport=False))
        self.refuse(lambda r: r['stages'][0]['proof']['requests'].pop())

    def test_target_actual_geometry_not_boolean_only(self):
        self.refuse(lambda r: r['stages'][0]['layout']['bodyProof']['target'].update(hit=False))
        self.refuse(lambda r: r['stages'][0]['layout']['bodyProof']['target'].update(rect=rect(y=310)))
        self.refuse(lambda r: r['stages'][0]['layout']['bodyProof']['target'].update(rect=rect(width=0)))
        self.refuse(lambda r: r['stages'][0]['layout']['bodyProof']['rows'][0]['threadButton'].update(hit=False))
        self.refuse(lambda r: r['stages'][0]['layout']['controls'][0].update(hit=False))

    def test_terminal_proof_not_replaced_after_capture(self):
        self.refuse(lambda r: r.update(proof={}))
        self.refuse(lambda r: r.update(layout=copy.deepcopy(r['stages'][3]['layout'])))

    def test_actual_retained_source_is_required(self):
        self.refuse(lambda r: r['stages'][0]['proof'].pop('retained'))
        self.refuse(lambda r: r['stages'][0]['proof']['retained']['rows'][0].update(matchesOriginal=False))
        for key, value in [('files', []), ('attachments', []), ('blocks', []), ('text', O.DELETED),
                           ('ts', O.REPLY), ('subtype', 'message'), ('user', 'U2')]:
            with self.subTest(key=key):
                self.refuse(lambda r: r['stages'][0]['proof']['retained']['rows'][0]['message'].update({key: value}))

    def test_actual_retained_owner_query_and_serial_are_bound(self):
        for key, value in [('ownerCurrent', False), ('loading', True), ('serial', True),
                           ('serial', 0), ('error', 'unknown')]:
            self.refuse(lambda r: r['stages'][1]['proof']['retained'].update({key: value}))
        self.refuse(lambda r: r['stages'][1]['proof']['retained']['query'].update(rootTs=O.CONTROLS[0]))
        self.refuse(lambda r: r['stages'][4]['proof']['retained'].update(serial=7))
        self.refuse(lambda r: r['stages'][2]['proof']['retained'].update(serial=3))

    def test_actual_thread_root_not_rebuilt_from_display_placeholder(self):
        self.refuse(lambda r: r['stages'][1]['proof']['retained'].update(root=None))
        self.refuse(lambda r: r['stages'][1]['proof']['retained']['root'].update(text=O.DELETED, files=[]))
        self.refuse(lambda r: r['stages'][1]['proof']['retained']['root']['document_comment'].update(is_visible=None))
        self.refuse(lambda r: r['stages'][0]['proof']['retained'].update(root=O.hidden()))

    def test_retained_controls_preserve_visibility_presence(self):
        self.refuse(lambda r: r['stages'][4]['proof']['retained']['rows'][1]['message']['document_comment'].update(is_visible=None))
        self.refuse(lambda r: r['stages'][5]['proof']['retained']['rows'][2]['message']['document_comment'].pop('is_visible'))

    def test_serde_defaults_allowed_without_dropping_original_fields(self):
        r = sample()
        for stage in r['stages']:
            for row in stage['proof']['retained']['rows']:
                row['message']['bot_id'] = None
                if row['message'].get('files'):
                    row['message']['files'][0]['is_external'] = None
        r['proof'] = copy.deepcopy(r['stages'][-1]['proof'])
        O.validate(r)

    def test_closed_stage_proof_exact_match(self):
        path = Path(__file__).with_name('check-canvas-special-roots-native.py')
        spec = importlib.util.spec_from_file_location('canvas_special_roots_runner', path)
        runner = importlib.util.module_from_spec(spec); spec.loader.exec_module(runner)
        receipt = sample(); stages = copy.deepcopy(receipt['stages'])
        for s in stages: s['screenshot'] = 'original.png'
        with patch.object(runner, '_base_validate_closed'):
            runner.validate_closed(runner.SPEC, 'canvas-special-roots', None, stages, receipt)
            stages[0]['proof']['hidden']['text'] = 'changed after screenshot'
            with self.assertRaises(RuntimeError):
                runner.validate_closed(runner.SPEC, 'canvas-special-roots', None, stages, receipt)


if __name__ == '__main__':
    unittest.main()
