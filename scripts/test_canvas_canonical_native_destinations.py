"""Adversarial portable receipts; never executes native or Slack requests."""
import copy
import importlib.util
from pathlib import Path
import unittest
import canvas_canonical_native_destinations as O
from test_canvas_canonical_file_roots import rect, read, retained, visible


def reader(id, arrived):
    result = {
        'canvas': {'requests': [], 'pairs': [], 'settlements': [], 'releases': [],
                   'held': None, 'networkCalls': 0, 'httpCapability': False},
        'list': {'requests': [], 'responses': [], 'settlements': [], 'releases': [],
                 'held': None, 'networkCalls': 0, 'httpCapability': False}}
    if not arrived:
        return result
    if id == O.CANVAS:
        result['canvas'].update(requests=O.canvas_requests(),
                                pairs=[{'serial': 1, 'held': False}],
                                settlements=[{'serial': 1, 'status': 'ready'}])
    else:
        result['list'].update(
            requests=[{'serial': 1, 'id': id, 'generation': 3, 'read': 4, 'revision': 0,
                       'requests': [{'method': 'POST', 'url': 'https://fixture.slack.com' + p,
                                     'fields': f} for p, f in O.list_wires()]}],
            responses=[{'serial': 1, 'responses': O.list_responses()}],
            settlements=[{'serial': 1, 'delivered': True, 'current': True,
                          'parsed': True, 'refreshAgain': False}])
    return result


def guidance():
    return dict(reason=O.READ_ONLY_REASON,
                visibleReasons=[dict(id='', **visible(O.READ_ONLY_REASON))],
                add=dict(text='Add item', disabled=True, title=O.READ_ONLY_REASON,
                         describedBy='slack-list-create-help', rect=rect(),
                         descriptions=[dict(id='slack-list-create-help', text=O.READ_ONLY_REASON,
                                            visible=False, matches=1)]))


def sample(id=O.CANVAS, viewport=(668, 380)):
    stages = []
    requests = [read('replies', O.D.ROOT) if i in (1, 2, 4) else read() for i in range(5)]
    for i, (name, count, resolves) in enumerate(zip(O.STAGES, O.READS, O.RESOLVES)):
        rows = [O.roots(id, i >= 3)[0], O.D.REPLY]
        value = retained(rows, True, count)
        value['pendingFile'] = i in (2, 4)
        raw = rows[0]['files'][-1]
        value['catalog'] = {'serial': count, 'rows': [
            {'id': id, 'source': raw, 'effective': raw, 'current': True}],
            'origins': [{'ts': O.D.ROOT, 'firstId': id, 'blockId': 'original-' + O.D.ROOT}]}
        proof = {'requests': requests[:count], 'responses': O.source_responses(id)[:count],
                 'retained': value, 'originals': rows,
                 'media': [{'url': url, 'registered': 0} for url in O.D.URLS],
                 'resolver': {
                     'requests': [{'method': 'POST', 'url': 'https://fixture.slack.com/api/files.info',
                                   'fields': [['file', id]]} for _ in range(resolves)],
                     'responses': [{'ok': True, 'file': O.target_file(id)} for _ in range(resolves)],
                     'settlements': [{'serial': j, 'delivered': True}
                                     for j in range(1, (0 if i < 3 else 1 if i == 3 else 2) + 1)],
                     'held': 1 if i == 2 else None,
                     'releases': [{'serial': 1, 'delivered': True}] if i >= 3 else []}}
        blocks = O.C.blocks() if i == 0 else O.C.edited()
        text = ('Keep exact \u4e2d\U0001f98a & draft link e\u0301.\nOpaque attributes retained.'
                + (' Canonical tail \u4e2d\U0001f98a' if i else ''))
        capture = 'original-capture' if i == 0 else 'edited-capture'
        reply = {'runtime': {'blocks': blocks, 'loading': False, 'saving': False,
                             'sending': False, 'error': None, 'nativePresent': i != 4,
                             'capture': capture, 'text': text},
                 'journal': {'drafts': [{'id': 'original-draft', 'blocks': blocks,
                                        'text': text, 'capture': capture, 'revision': 2 if i else 1}],
                             'attempt': None, 'history': []}}
        if i == 4:
            layout = {'ok': True, 'ready': True, 'viewport': list(viewport), 'id': id,
                      'kind': 'canvas' if id == O.CANVAS else 'list',
                      'title': visible(O.TITLE[id]), 'body': visible(O.BODY[id]),
                      'close': visible(''), 'scroll': rect(0, 0, 600, 300),
                      'generation': '4', 'native': None,
                      'guidance': guidance() if id == O.LIST else None,
                      'rowId': 'R_CANON_DESTINATION' if id == O.LIST else None,
                      'schema': [{'id': 'C_CANON_NAME', 'text': 'Item'}] if id == O.LIST else None}
        else:
            layout = {'ok': True, 'viewport': list(viewport), 'inScroll': True, 'viewer': None,
                      'target': visible('target'), 'scroll': rect(0, 0, 600, 300),
                      'targetKind': 'link' if i == 2 else 'editor-line',
                      'native': {'blocks': blocks, 'composing': False, 'running': True,
                                 'sameNode': i in (1, 2), 'sameMount': i in (1, 2),
                                 'sameBlocks': i != 0}}
        stages.append({'ok': True, 'stage': name, 'ordinal': i, 'noSession': True,
                       'noTransport': True, 'proof': proof, 'reply': reply,
                       'layout': layout, 'targetReads': reader(id, i >= 4)})
    r = copy.deepcopy({'destinationId': id, 'stages': stages, 'layout': stages[-1]['layout'],
                       'proof': stages[-1]['proof'], 'departure': {
                           'native': stages[1]['layout'], 'reply': stages[3]['reply']}})
    return r


class DestinationOracle(unittest.TestCase):
    def refuse(self, mutation, id=O.CANVAS):
        value = sample(id)
        mutation(value)
        with self.assertRaises((RuntimeError, KeyError, TypeError, ValueError)):
            O.validate(value)

    def test_both_readers_both_widths(self):
        for id in (O.CANVAS, O.LIST):
            for viewport in ((668, 380), (1088, 728)):
                O.validate(sample(id, viewport))

    def test_unicode_literal_not_matching_corrupt_fixture_constants(self):
        text = O.C.edited()[1]['elements'][0]['elements'][0]['text']
        self.assertEqual([ord(c) for c in text[-2:]], [0x4E2D, 0x1F98A])
        self.assertIn('&amp;', O.HTML)
        self.assertIn(chr(0x1F98A), O.HTML)

    def test_original_blocks_opaque_and_styles_are_independent(self):
        for mutate in (
            lambda b: b[0]['elements'][0]['elements'][0].pop('style'),
            lambda b: b[1].pop('opaque'),
            lambda b: b[1]['elements'][0]['elements'][0].pop('fixture_opaque'),
            lambda b: b[0]['elements'][0]['elements'][1].update(url='https://wrong.test'),
        ):
            self.refuse(lambda r: mutate(r['stages'][5]['reply']['journal']['drafts'][0]['blocks']))

    def test_original_native_and_fresh_reopened_node(self):
        self.refuse(lambda r: r['stages'][2]['layout']['native'].update(sameNode=False))
        self.refuse(lambda r: r['stages'][5]['layout']['native'].update(sameNode=True))
        self.refuse(lambda r: r['stages'][5]['layout']['native'].update(sameMount=True))
        self.refuse(lambda r: r['departure']['native']['native'].update(running=False))

    def test_refresh_does_not_fabricate_same_node_requirement(self):
        r = sample()
        r['stages'][3]['layout']['native'].update(sameNode=False, sameMount=False)
        O.validate(r)

    def test_original_durable_identity_cannot_be_replaced(self):
        self.refuse(lambda r: r['stages'][4]['reply']['journal']['drafts'][0].update(id='other'))
        self.refuse(lambda r: r['departure']['reply']['journal']['drafts'][0].update(id='other'))

    def test_exact_disk_capture_revision_and_plain_fallback_preserved(self):
        self.refuse(lambda r: r['stages'][5]['reply']['journal']['drafts'][0].update(capture='other'))
        self.refuse(lambda r: r['stages'][5]['reply']['journal']['drafts'][0].update(revision=3))
        self.refuse(lambda r: r['stages'][4]['reply']['runtime'].update(text='flattened'))
        self.refuse(lambda r: r['stages'][1]['reply']['journal']['drafts'][0].update(revision=1))

    def test_departure_does_not_reuse_current_destination_editor(self):
        self.refuse(lambda r: r['stages'][4]['reply']['runtime'].update(nativePresent=True))
        self.refuse(lambda r: r['stages'][4]['layout'].update(native={'unexpected': True}))

    def test_no_post_or_attempt_during_navigation(self):
        self.refuse(lambda r: r['stages'][4]['reply']['journal'].update(attempt={'sent': True}))
        self.refuse(lambda r: r['stages'][4]['reply']['journal']['history'].append({'sent': True}))

    def test_held_delivery_is_real_and_stale_does_not_route(self):
        self.refuse(lambda r: r['stages'][3]['proof']['resolver']['releases'][0].update(delivered=False))
        self.refuse(lambda r: r['stages'][2]['proof']['resolver']['requests'].append({}))
        self.refuse(lambda r: r['stages'][3].update(targetReads=reader(O.CANVAS, True)))

    def test_request_scope_not_title_url_authority(self):
        self.refuse(lambda r: r['stages'][4]['proof']['resolver']['requests'][0].update(fields=[['file', O.LIST]]))
        self.refuse(lambda r: r['stages'][4]['proof']['resolver']['responses'][0]['file'].update(id=O.LIST))
        self.refuse(lambda r: r['stages'][4]['proof']['resolver']['requests'][0].update(url='https://other.test/api/files.info'))

    def test_original_root_raw_and_first_occurrence_remain(self):
        self.refuse(lambda r: r['stages'][3]['proof']['retained']['root']['message']['files'].reverse())
        self.refuse(lambda r: r['stages'][3]['proof']['responses'][2]['messages'][0].update(blocks=[]))
        self.refuse(lambda r: r['stages'][3]['proof']['retained']['catalog']['origins'][0].update(firstId=O.LIST))

    def test_destination_cannot_overwrite_display_source(self):
        self.refuse(lambda r: r['stages'][4]['proof']['retained']['catalog']['rows'][0].update(source=O.target_file(O.CANVAS)))
        self.refuse(lambda r: r['stages'][4]['proof']['media'][0].update(registered=1))

    def test_canvas_pair_requires_actual_raw_protobuf_and_html(self):
        self.refuse(lambda r: r['stages'][4]['targetReads']['canvas']['requests'][2]['response'].update(base64=''))
        self.refuse(lambda r: r['stages'][4]['targetReads']['canvas']['requests'][4]['response'].update(html='<p>fake</p>'))
        self.refuse(lambda r: r['stages'][4]['targetReads']['canvas']['settlements'][0].update(status='retired'))
        self.refuse(lambda r: r['stages'][4]['targetReads']['canvas']['requests'].pop())

    def test_list_actual_schema_records_permission_and_factory_shapes(self):
        self.refuse(lambda r: r['stages'][4]['targetReads']['list']['responses'][0]['responses'][0]['file']['list_metadata'].update(schema=[]), O.LIST)
        self.refuse(lambda r: r['stages'][4]['targetReads']['list']['responses'][0]['responses'][1]['records'][0].update(id='wrong'), O.LIST)
        self.refuse(lambda r: r['stages'][4]['targetReads']['list']['requests'][0]['requests'][1]['fields'].append(['cursor', 'foreign']), O.LIST)
        self.refuse(lambda r: r['stages'][4]['targetReads']['list']['settlements'][0].update(current=False), O.LIST)

    def test_list_read_only_guidance_has_one_visible_owner(self):
        self.refuse(lambda r: r['stages'][4]['layout']['guidance']['visibleReasons'].append(
            copy.deepcopy(r['stages'][4]['layout']['guidance']['visibleReasons'][0])), O.LIST)
        self.refuse(lambda r: r['stages'][4]['layout']['guidance'].update(visibleReasons=[]), O.LIST)
        self.refuse(lambda r: r['stages'][4]['layout'].pop('guidance'), O.LIST)
        self.refuse(lambda r: r['stages'][4]['layout']['guidance'].update(reason='This List is locked.'), O.LIST)

    def test_list_single_notice_must_be_readable_and_hit_testable(self):
        for field, value in [('text', ''), ('inView', False), ('hit', False)]:
            self.refuse(lambda r: r['stages'][4]['layout']['guidance']['visibleReasons'][0].update({field: value}), O.LIST)
        self.refuse(lambda r: r['stages'][4]['layout']['guidance']['visibleReasons'][0].update(rect=rect(y=900)), O.LIST)

    def test_list_add_disabled_title_and_help_remain_exact(self):
        for field, value in [('disabled', False), ('title', ''), ('title', 'Add a new item'),
                             ('text', 'Other'), ('describedBy', ''), ('describedBy', None)]:
            self.refuse(lambda r: r['stages'][4]['layout']['guidance']['add'].update({field: value}), O.LIST)
        self.refuse(lambda r: r['stages'][4]['layout']['guidance']['add'].pop('descriptions'), O.LIST)
        self.refuse(lambda r: r['stages'][4]['layout']['guidance']['add'].update(descriptions=[]), O.LIST)

    def test_list_wrong_duplicate_or_missing_description_association(self):
        for value in ('foreign-help', 'slack-list-create-help slack-list-create-help',
                      'slack-list-create-help other-help'):
            self.refuse(lambda r: r['stages'][4]['layout']['guidance']['add'].update(describedBy=value), O.LIST)
        for field, value in [('id', 'foreign-help'), ('text', ''), ('text', 'Add a new item'),
                             ('matches', 0), ('matches', 2), ('matches', True), ('visible', None), ('visible', True)]:
            self.refuse(lambda r: r['stages'][4]['layout']['guidance']['add']['descriptions'][0].update({field: value}), O.LIST)
        self.refuse(lambda r: r['stages'][4]['layout']['guidance']['add']['descriptions'].append(
            copy.deepcopy(r['stages'][4]['layout']['guidance']['add']['descriptions'][0])), O.LIST)

    def test_list_sr_only_help_can_be_hidden_but_present(self):
        value = sample(O.LIST)
        add = value['stages'][4]['layout']['guidance']['add']
        self.assertFalse(add['descriptions'][0]['visible'])
        # Existing narrow header can scroll the control offscreen; never fake
        # a painted Add hit test to prove its separate accessible association.
        add['rect']['x'] = 1000
        O.validate(value)
        add['rect']['width'] = 0
        with self.assertRaises(RuntimeError): O.validate(value)

    def test_canvas_does_not_gain_list_help_requirements(self):
        value = sample(O.CANVAS)
        value['stages'][4]['layout'].pop('guidance')
        O.validate(value)
        self.refuse(lambda r: r['stages'][4]['layout'].update(guidance=guidance()))

    def test_wrong_reader_or_network_refused(self):
        self.refuse(lambda r: r['stages'][4]['targetReads']['list'].update(requests=[{}]))
        self.refuse(lambda r: r['stages'][4]['targetReads']['canvas'].update(httpCapability=True))
        self.refuse(lambda r: r['stages'][4]['targetReads']['canvas'].update(networkCalls=1))

    def test_actual_title_body_identity_and_schema(self):
        self.refuse(lambda r: r['stages'][4]['layout']['title'].update(text='EMBEDDED first FCANON002'))
        self.refuse(lambda r: r['stages'][4]['layout']['body'].update(text=''))
        self.refuse(lambda r: r['stages'][4]['layout'].update(id=O.LIST))
        self.refuse(lambda r: r['stages'][4]['layout'].update(rowId='wrong'), O.LIST)

    def test_full_painted_geometry_and_hit_required(self):
        for key in ('title', 'body', 'close'):
            self.refuse(lambda r: r['stages'][4]['layout'][key].update(hit=False))
            self.refuse(lambda r: r['stages'][4]['layout'][key].update(rect=rect(y=900)))
        self.refuse(lambda r: r['stages'][4]['layout']['body']['rect'].update(height=float('nan')))
        self.refuse(lambda r: r['stages'][4]['layout'].update(scroll=rect(y=200, height=30)))

    def test_stage_terminal_prefix_and_counts_closed(self):
        self.refuse(lambda r: r['stages'].pop())
        self.refuse(lambda r: r['stages'][2].update(ordinal=True))
        self.refuse(lambda r: r.update(proof={}))
        self.refuse(lambda r: r['stages'][5]['proof']['requests'].append({}))

    def test_runner_preserves_named_four_case_boundary(self):
        path = Path(__file__).with_name('check-canvas-canonical-native-destinations.py')
        spec = importlib.util.spec_from_file_location('destination_runner_test', path)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        self.assertEqual(len(O.FIXTURES), 4)
        self.assertEqual(mod.SPEC.stages, O.STAGES)
        self.assertEqual(mod.SPEC.counts, {'requestCount': 5, 'resolverCount': 2})
        r = sample()
        r.update(ok=True, noSession=True, noTransport=True, draftPreserved=True,
                 freshTarget=True, staleRefused=True, noDuplicateDispatch=True,
                 requestCount=5, resolverCount=2)
        mod.validate(mod.SPEC, 'canvas-canonical-native-canvas-narrow', r)
        with self.assertRaises(RuntimeError):
            mod.validate(mod.SPEC, 'canvas-canonical-native-list-narrow', r)


if __name__ == '__main__':
    unittest.main()
