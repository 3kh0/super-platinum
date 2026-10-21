"""Adversarial portable retirement receipts; no app or Slack actions."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
import canvas_canonical_native_retirement as O
from test_canvas_canonical_native_destinations import sample as destination_sample, reader
from test_canvas_canonical_file_roots import rect, read, retained, visible


def target_reads(id, index):
    value = reader(id, False)
    count = (0, 0, 1, 1, 1, 2, 2)[index]
    if not count:
        return value
    kind = 'canvas' if id == O.D.CANVAS else 'list'
    v = value[kind]
    v['held'] = 1 if index < 4 else None
    v['releases'] = [] if index < 4 else [{'serial': 1, 'delivered': kind == 'list'}]
    if kind == 'canvas':
        for j in range(count):
            for raw in O.D.canvas_requests():
                raw.update(pair=j + 1, serial=len(v['requests']) + 1)
                v['requests'].append(raw)
            v['pairs'].append({'serial': j + 1, 'held': j == 0})
        if index >= 3:
            v['settlements'].append({'serial': 1, 'status': 'cancelled'})
        if index >= 5:
            v['settlements'].append({'serial': 2, 'status': 'ready'})
    else:
        for j in range(count):
            batch = copy.deepcopy(reader(id, True)['list'])
            batch['requests'][0].update(serial=j + 1, generation=20 + j * 2, read=30 + j * 2)
            batch['responses'][0]['serial'] = j + 1
            v['requests'] += batch['requests']
            v['responses'] += batch['responses']
        if index >= 4:
            v['settlements'].append({'serial': 1, 'delivered': True, 'current': False,
                                     'parsed': False, 'refreshAgain': False})
        if index >= 5:
            v['settlements'].append({'serial': 2, 'delivered': True, 'current': True,
                                     'parsed': True, 'refreshAgain': False})
    return value


def route(id, index):
    target = index in (2, 5)
    generation = (1, 1, 2, 3, 3, 4, 5)[index]
    result = {'account': '147b5480-9f1a-4443-89da-836472cbb209', 'epoch': 1,
              'team': 'T1', 'user': 'U0', 'shortcutGeneration': generation,
              'overlay': 'list' if target and id == O.D.LIST else 'canvas',
              'canvas': {'id': id if target and id == O.D.CANVAS else 'Ffixture',
                         'title': O.D.TITLE[id] if target and id == O.D.CANVAS else 'Project handbook'},
              'canvasRead': None, 'list': None}
    if target and id == O.D.LIST:
        result['list'] = {'id': id, 'generation': 20 if index == 2 else 22,
                          'loading': index == 2, 'stale': index == 2}
    elif target:
        result['canvasRead'] = {'visit': generation, 'read': generation,
                               'loading': index == 2, 'stale': index == 2,
                               'error': None, 'file': None if index == 2 else id}
    return result


def sample(id=O.D.CANVAS, viewport=(668, 380)):
    old = destination_sample(id, viewport)
    stages = []
    requests = [read('replies', O.D.D.ROOT) if i % 2 else read() for i in range(6)]
    raws = [O.D.response(id, thread=i % 2 == 1) for i in range(6)]
    for i, (name, count, resolves) in enumerate(zip(O.STAGES, O.READS, O.RESOLVES)):
        stage = copy.deepcopy(old['stages'][0 if i == 0 else 1])
        stage.update(stage=name, ordinal=i)
        rows = [O.D.roots(id)[0], O.D.D.REPLY]
        value = retained(rows, True, count)
        raw = rows[0]['files'][-1]
        value.update(pendingFile=i in (2, 5), catalog={
            'serial': count, 'rows': [{'id': id, 'source': raw, 'effective': raw, 'current': True}],
            'origins': [{'ts': O.D.D.ROOT, 'firstId': id, 'blockId': 'original-' + O.D.D.ROOT}]})
        stage['proof'] = {'requests': copy.deepcopy(requests[:count]),
                          'responses': copy.deepcopy(raws[:count]), 'originals': copy.deepcopy(rows),
                          'retained': value, 'media': copy.deepcopy(old['stages'][0]['proof']['media']),
                          'resolver': {'requests': [{'method': 'POST',
                              'url': 'https://fixture.slack.com/api/files.info', 'fields': [['file', id]]}
                              for _ in range(resolves)],
                             'responses': [{'ok': True, 'file': O.D.target_file(id)} for _ in range(resolves)],
                             'settlements': [{'serial': n, 'delivered': True} for n in range(1, resolves + 1)],
                             'held': None, 'releases': []}}
        stage['targetReads'] = target_reads(id, i)
        stage['route'] = route(id, i)
        stage['reply']['runtime']['nativePresent'] = i not in (2, 5)
        if i in (2, 5):
            stage['layout'] = copy.deepcopy(old['stages'][4]['layout'])
            stage['layout']['generation'] = str(stage['route']['list']['generation'] if id == O.D.LIST
                                                else stage['route']['shortcutGeneration'])
            if i == 2:
                stage['layout'].pop('ready')
                stage['layout'].pop('body')
                stage['layout'].update(pending=True, busy=True, contentPresent=False,
                    busySource='scroll-aria-busy' if id == O.D.CANVAS else 'initial-list-loading-status',
                    scrollMounted=id == O.D.CANVAS,
                    scroll=rect(0, 0, 600, 300) if id == O.D.CANVAS else None,
                    skeletonPresent=id == O.D.CANVAS, dialog=rect(0, 0, 620, 320),
                    loading=visible('Loading\u2026' if id == O.D.CANVAS else 'Loading list\u2026'))
                stage['layout']['close']['disabled'] = False
        else:
            stage['layout']['targetKind'] = 'editor-line'
            stage['layout']['native'].update(sameNode=i == 1, sameMount=i == 1,
                                             sameBlocks=i != 0)
        stages.append(stage)
    r = {'destinationId': id, 'stages': stages, 'layout': copy.deepcopy(stages[-1]['layout']),
         'proof': copy.deepcopy(stages[-1]['proof']), 'departure': {
             'native': copy.deepcopy(stages[1]['layout']), 'reply': copy.deepcopy(stages[1]['reply']),
             'route': copy.deepcopy(stages[1]['route'])}}
    before = {k: copy.deepcopy(stages[3][k]) for k in ('route', 'layout', 'proof', 'reply')}
    r['late'] = {'before': before, 'after': copy.deepcopy(before),
                 'release': {'serial': 1, 'delivered': id == O.D.LIST}}
    return r


class RetirementOracle(unittest.TestCase):
    def refuse(self, fn, id=O.D.CANVAS):
        r = sample(id)
        fn(r)
        with self.assertRaises((RuntimeError, KeyError, TypeError, ValueError)):
            O.validate(r)

    def test_both_readers_both_widths(self):
        for id in (O.D.CANVAS, O.D.LIST):
            for viewport in ((668, 380), (1088, 728)):
                O.validate(sample(id, viewport))

    def test_canvas_cancellation_precedes_release(self):
        self.refuse(lambda r: r['stages'][3]['targetReads']['canvas']['settlements'].clear())
        self.refuse(lambda r: r['stages'][3]['targetReads']['canvas']['settlements'][0].update(status='failed'))
        self.refuse(lambda r: r['stages'][4]['targetReads']['canvas']['releases'][0].update(delivered=True))
        self.refuse(lambda r: r['late']['release'].update(delivered=True))

    def test_list_requires_delivery_but_no_parse_or_adoption(self):
        for key in ('current', 'parsed', 'refreshAgain'):
            self.refuse(lambda r: r['stages'][4]['targetReads']['list']['settlements'][0].update({key: True}), O.D.LIST)
        self.refuse(lambda r: r['stages'][4]['targetReads']['list']['settlements'][0].update(delivered=False), O.D.LIST)
        self.refuse(lambda r: r['stages'][3]['targetReads']['list']['settlements'].append({'serial': 1}), O.D.LIST)
        self.refuse(lambda r: r['late']['release'].update(delivered=False), O.D.LIST)

    def test_pending_not_fake_ready_content(self):
        for key in ('pending', 'busy', 'skeletonPresent'):
            self.refuse(lambda r: r['stages'][2]['layout'].update({key: False}))
        self.refuse(lambda r: r['stages'][2]['layout'].update(contentPresent=True))
        self.refuse(lambda r: r['stages'][2]['layout'].update(native={'old': True}))
        self.refuse(lambda r: r['stages'][2]['layout']['loading'].update(text='Ready'))
        self.refuse(lambda r: r['stages'][2]['layout']['close'].update(disabled=True))

    def test_pending_painted_title_status_and_close(self):
        for key in ('title', 'loading', 'close'):
            self.refuse(lambda r: r['stages'][2]['layout'][key].update(hit=False))
            self.refuse(lambda r: r['stages'][2]['layout'][key].update(rect=rect(y=900)))
        self.refuse(lambda r: r['stages'][2]['layout']['title'].update(text='EMBEDDED title'))

    def test_canvas_loading_status_is_outside_body_scroll(self):
        r = sample()
        r['stages'][2]['layout']['scroll'] = rect(y=150, height=100)
        O.validate(r)

    def test_cold_list_loading_has_no_document_scroll(self):
        r = sample(O.D.LIST)
        self.assertIsNone(r['stages'][2]['layout']['scroll'])
        self.assertFalse(r['stages'][2]['layout']['scrollMounted'])
        O.validate(r)
        self.refuse(lambda r: r['stages'][2]['layout'].update(scroll=rect()), O.D.LIST)
        self.refuse(lambda r: r['stages'][2]['layout'].update(scrollMounted=True), O.D.LIST)
        self.refuse(lambda r: r['stages'][2]['layout'].update(busySource='scroll-aria-busy'), O.D.LIST)
        self.refuse(lambda r: r['stages'][2]['layout'].update(scroll=None))
        self.refuse(lambda r: r['stages'][2]['layout'].update(scrollMounted=False))
        self.refuse(lambda r: r['stages'][2]['layout'].update(busySource='initial-list-loading-status'))

    def test_exact_raw_response_and_factory_each_batch(self):
        for stage in (2, 5):
            self.refuse(lambda r: r['stages'][stage]['targetReads']['canvas']['requests'][2]['response'].update(base64=''))
            self.refuse(lambda r: r['stages'][stage]['targetReads']['list']['responses'][0]['responses'][1]['records'][0].update(id='OTHER'), O.D.LIST)
        self.refuse(lambda r: r['stages'][5]['targetReads']['canvas']['requests'][9]['response'].update(html='<p>fabricated</p>'))
        self.refuse(lambda r: r['stages'][5]['targetReads']['list']['requests'][1]['requests'][0].update(fields=[['file', O.D.CANVAS]]), O.D.LIST)

    def test_fresh_batch_only_second_ready(self):
        self.refuse(lambda r: r['stages'][5]['targetReads']['canvas']['settlements'][0].update(status='ready'))
        self.refuse(lambda r: r['stages'][5]['targetReads']['canvas']['pairs'][1].update(held=True))
        self.refuse(lambda r: r['stages'][5]['targetReads']['list']['requests'][1].update(generation=20), O.D.LIST)
        self.refuse(lambda r: r['stages'][5]['targetReads']['list']['settlements'][1].update(current=False), O.D.LIST)
        self.refuse(lambda r: r['stages'][5]['layout'].update(generation=r['stages'][2]['layout']['generation']))

    def test_late_equality_not_detached_from_captured_source(self):
        for key in ('route', 'layout', 'proof', 'reply'):
            self.refuse(lambda r: r['late']['after'][key].update(corrupt=True))
            def both(r):
                r['late']['before'][key] = {'fake': True}
                r['late']['after'][key] = {'fake': True}
            self.refuse(both)
        self.refuse(lambda r: r['stages'][4]['route'].update(shortcutGeneration=999))

    def test_original_identity_owner_and_visit(self):
        for key, value in [('team', 'T2'), ('user', 'UOTHER'), ('account', None), ('epoch', 2)]:
            self.refuse(lambda r: r['stages'][3]['route'].update({key: value}))
        self.refuse(lambda r: r['stages'][2]['route']['canvas'].update(id='Ffixture'))
        self.refuse(lambda r: r['stages'][2]['route']['list'].update(loading=False), O.D.LIST)
        self.refuse(lambda r: r['stages'][5]['route']['canvasRead'].update(file='Ffixture'))

    def test_original_held_batch_cannot_change_while_still_well_formed(self):
        self.refuse(lambda r: r['stages'][3]['targetReads']['list']['requests'][0].update(read=999), O.D.LIST)
        self.refuse(lambda r: r['stages'][3]['targetReads']['list']['requests'][0].update(revision=9), O.D.LIST)
        self.refuse(lambda r: r['stages'][2]['targetReads']['list']['requests'][0].update(generation=9), O.D.LIST)

    def test_no_resolver_hold_or_source_rename(self):
        self.refuse(lambda r: r['stages'][2]['proof']['resolver'].update(held=1))
        self.refuse(lambda r: r['stages'][3]['proof']['responses'][3]['messages'][0].update(text='renamed'))
        self.refuse(lambda r: r['stages'][4]['proof']['requests'].append({}))
        self.refuse(lambda r: r['stages'][2]['proof']['retained'].update(pendingFile=False))

    def test_original_blocks_style_opaque_and_journal_unchanged(self):
        self.refuse(lambda r: r['stages'][6]['reply']['journal']['drafts'][0]['blocks'][0]['elements'][0]['elements'][0].pop('style'))
        self.refuse(lambda r: r['stages'][6]['reply']['journal']['drafts'][0]['blocks'][1].pop('opaque'))
        for key, value in [('id', 'other'), ('capture', 'other'), ('revision', 99), ('text', 'flattened')]:
            self.refuse(lambda r: r['stages'][6]['reply']['journal']['drafts'][0].update({key: value}))
        self.refuse(lambda r: r['stages'][2]['reply']['journal'].update(attempt={'sent': True}))

    def test_node_custody_departure_and_reopen(self):
        self.refuse(lambda r: r['stages'][1]['layout']['native'].update(sameNode=False))
        for i in (3, 4, 6):
            self.refuse(lambda r: r['stages'][i]['layout']['native'].update(sameMount=True))
        self.refuse(lambda r: r['departure']['native']['native'].update(running=False))
        self.refuse(lambda r: r['stages'][2]['reply']['runtime'].update(nativePresent=True))

    def test_no_wrong_reader_network_media_or_target_source_adoption(self):
        self.refuse(lambda r: r['stages'][5]['targetReads']['list'].update(requests=[{}]))
        self.refuse(lambda r: r['stages'][5]['targetReads']['canvas'].update(httpCapability=True))
        self.refuse(lambda r: r['stages'][5]['proof']['media'][0].update(registered=1))
        self.refuse(lambda r: r['stages'][5]['proof']['retained']['catalog']['rows'][0].update(source=O.D.target_file(O.D.CANVAS)))

    def test_closed_stage_shape_terminal_and_ordinal(self):
        self.refuse(lambda r: r['stages'].pop())
        self.refuse(lambda r: r['stages'][2].update(ordinal=True))
        self.refuse(lambda r: r.update(proof={}))
        self.refuse(lambda r: r['stages'][5]['layout']['body'].update(text='wrong destination'))

    def test_unicode_codepoints_independent_of_constants(self):
        self.assertEqual([ord(c) for c in O.D.C.edited()[1]['elements'][0]['elements'][0]['text'][-2:]], [0x4e2d, 0x1f98a])

    def test_runner_named_flags_and_existing_oracle_unchanged(self):
        path = Path(__file__).with_name('check-canvas-canonical-native-retirement.py')
        spec = importlib.util.spec_from_file_location('retirement_runner_test', path)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        self.assertEqual(mod.SPEC.stages, O.STAGES)
        self.assertEqual(mod.SPEC.counts, {'requestCount': 6, 'resolverCount': 2, 'targetReadCount': 2})
        with tempfile.TemporaryDirectory() as d:
            flags = mod.private_checks(mod.SPEC, Path(d))
            self.assertEqual(flags['SUPER_PLATINUM_CANVAS_DESTINATION_RETIREMENT_CHECK'], '1')
            self.assertEqual(flags['SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK'], '1')
            with self.assertRaises(RuntimeError):
                mod.private_checks(copy.copy(mod.SPEC), Path(d))
        self.assertEqual(len(O.D.STAGES), 6)
        r = sample()
        r.update(ok=True, noSession=True, noTransport=True, draftPreserved=True, freshTarget=True,
                 lateRefused=True, nativeClose=True, requestCount=6, resolverCount=2, targetReadCount=2)
        mod.validate(mod.SPEC, 'canvas-canonical-native-canvas-narrow', r)
        with self.assertRaises(RuntimeError):
            mod.validate(mod.SPEC, 'canvas-canonical-native-list-narrow', r)


if __name__ == '__main__':
    unittest.main()
