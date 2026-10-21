"""Portable adversarial storage acceptance receipts; no native execution."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import canvas_canonical_native_storage as O
from test_canvas_canonical_native_destinations import sample as destination_sample, reader
from test_canvas_canonical_file_roots import read, retained, visible, rect

IDS = ['11111111-1111-4111-8111-111111111111', '22222222-2222-4222-8222-222222222222',
       '33333333-3333-4333-8333-333333333333']
DRAFT = '44444444-4444-4444-8444-444444444444'


def fixture(id, narrow=True):
    return 'canvas-canonical-native-' + ('canvas' if id == O.D.CANVAS else 'list') + ('-narrow' if narrow else '')


def acknowledgments(saved_a, name):
    hold = {'version': 1, 'nonce': 'ab' * 16, 'pid': 123, 'fixture': name, 'sequence': 1,
            'action': 'hold', 'ok': True, 'account': O.ACCOUNT, 'scope': copy.deepcopy(O.SCOPE),
            'root': O.D.D.ROOT, 'journal': {'drafts': [copy.deepcopy(saved_a)], 'attempt': None, 'history': []},
            'revision': 2, 'captureIds': IDS[:2], 'contentSha256': 'ab' * 32}
    release = copy.deepcopy(hold)
    release.update(sequence=2, action='release', unchanged=True)
    return hold, release


def sample(id=O.D.CANVAS, viewport=(668, 380)):
    old = destination_sample(id, viewport)
    stages = []
    name = fixture(id, viewport == (668, 380))
    hold = release = None
    for i, (stage_name, count, resolves) in enumerate(zip(O.STAGES, O.READS, O.RESOLVES)):
        stage = copy.deepcopy(old['stages'][0 if i == 0 else 1])
        stage.update(stage=stage_name, ordinal=i)
        rows = [O.D.roots(id)[0], O.D.D.REPLY]
        value = retained(rows, True, count)
        raw = rows[0]['files'][-1]
        value.update(pendingFile=i == 4, catalog={'serial': count,
            'rows': [{'id': id, 'source': raw, 'effective': raw, 'current': True}],
            'origins': [{'ts': O.D.D.ROOT, 'firstId': id, 'blockId': 'original-' + O.D.D.ROOT}]})
        stage['proof'] = {'requests': [read('replies', O.D.D.ROOT) if j % 2 else read() for j in range(count)],
            'responses': [O.D.response(id, thread=j % 2 == 1) for j in range(count)],
            'originals': copy.deepcopy(rows), 'retained': value,
            'media': copy.deepcopy(old['stages'][0]['proof']['media']),
            'resolver': {'requests': [{'method': 'POST', 'url': 'https://fixture.slack.com/api/files.info',
                                      'fields': [['file', id]]} for _ in range(resolves)],
                         'responses': [{'ok': True, 'file': O.D.target_file(id)} for _ in range(resolves)],
                         'settlements': [{'serial': j, 'delivered': True} for j in range(1, resolves + 1)],
                         'held': None, 'releases': []}}
        stage['targetReads'] = reader(id, i >= 4)
        runtime = stage['reply']['runtime']
        runtime.update(blocks=O.blocks(i), text=O.text(i), capture=IDS[min(i, 2)],
                       error=O.ERROR if i == 2 else None, nativePresent=i != 4)
        if i == 2:
            stage['reply']['journal'] = None
        else:
            item = stage['reply']['journal']['drafts'][0]
            item.update(id=DRAFT, revision=1 if i == 0 else 2 if i == 1 else 3,
                        capture=IDS[min(i, 2)], blocks=O.blocks(i), text=O.text(i))
            if i == 1:
                hold, release = acknowledgments(item, name)
        stage['storage'] = {'hold': copy.deepcopy(hold) if i >= 2 else None,
                            'release': copy.deepcopy(release) if i >= 3 else None}
        if i == 4:
            stage['layout'] = copy.deepcopy(old['stages'][4]['layout'])
        else:
            stage['layout']['native'].update(blocks=O.blocks(i), sameNode=i == 1,
                                             sameMount=i == 1, sameBlocks=i == 1)
            stage['layout']['targetKind'] = 'retry' if i == 2 else 'editor-line'
            stage['layout']['alerts'] = ['', O.ERROR] if i == 2 else ['']
            stage['layout']['panelAlerts'] = [
                {'text': '', 'visible': False, 'owner': 'composer'}]
            if i == 2:
                stage['layout']['panelAlerts'].append(
                    {'text': O.ERROR, 'visible': True, 'owner': 'composer'})
            stage['layout']['controls'] = {'ok': True,
                'send': dict(visible('Send reply'), disabled=i == 2),
                'retry': dict(visible('Retry local save'), disabled=False) if i == 2 else None,
                'alerts': [visible(O.ERROR)] if i == 2 else []}
        stages.append(stage)
    return {'destinationId': id, 'stages': stages, 'proof': copy.deepcopy(stages[-1]['proof']),
            'layout': copy.deepcopy(stages[-1]['layout']), 'storage': copy.deepcopy(stages[-1]['storage']),
            'lockedEdit': {'ok': True, 'blocks': O.blocks(2), 'text': O.TAIL},
            'refusedClick': {'ok': True, 'prevented': [True]},
            'departure': {'native': copy.deepcopy(stages[3]['layout']),
                          'reply': copy.deepcopy(stages[3]['reply'])}}


class StorageOracle(unittest.TestCase):
    def refuse(self, fn, id=O.D.CANVAS):
        r = sample(id)
        fn(r)
        with self.assertRaises((RuntimeError, KeyError, TypeError, ValueError)):
            O.validate(r, fixture(id))

    def test_both_targets_both_widths(self):
        for id in (O.D.CANVAS, O.D.LIST):
            for viewport in ((668, 380), (1088, 728)):
                O.validate(sample(id, viewport), fixture(id, viewport == (668, 380)))

    def test_independent_unicode_and_distinct_B(self):
        self.assertEqual([ord(c) for c in O.TAIL[-2:]], [0x4e2d, 0x1f98a])
        self.assertEqual(O.TAIL[:13], ' Locked tail ')
        self.assertNotEqual(O.blocks(1), O.blocks(2))
        self.assertEqual(O.blocks(2)[0], O.blocks(1)[0])
        self.assertEqual(O.blocks(2)[1]['opaque'], O.blocks(1)[1]['opaque'])

    def test_exclusive_hold_cannot_claim_fresh_journal(self):
        self.refuse(lambda r: r['stages'][2]['reply'].update(journal=r['stages'][1]['reply']['journal']))
        self.refuse(lambda r: r['stages'][2]['storage'].update(release=r['stages'][3]['storage']['release']))
        self.refuse(lambda r: r['stages'][1]['storage'].update(hold=r['stages'][2]['storage']['hold']))

    def test_durable_A_baseline_is_exact_previous_journal(self):
        self.refuse(lambda r: r['stages'][2]['storage']['hold']['journal']['drafts'][0].update(blocks=O.blocks(2)))
        self.refuse(lambda r: r['stages'][2]['storage']['hold']['journal']['drafts'][0].update(capture=IDS[2]))
        self.refuse(lambda r: r['stages'][2]['storage']['hold'].update(revision=1))
        self.refuse(lambda r: r['stages'][2]['storage']['hold'].update(captureIds=[IDS[0]]))

    def test_lock_owner_protocol_scope_and_root_exact(self):
        for key, value in [('account', IDS[0]), ('root', '1719800009.000100'), ('pid', True),
                           ('nonce', 'x'), ('version', True), ('sequence', 2), ('action', 'release'),
                           ('fixture', fixture(O.D.LIST)), ('ok', False)]:
            self.refuse(lambda r: r['stages'][2]['storage']['hold'].update({key: value}))
        self.refuse(lambda r: r['stages'][2]['storage']['hold']['scope'].update(file=O.D.CANVAS))
        self.refuse(lambda r: r['stages'][2]['storage']['hold'].update(extra='foreign'))

    def test_release_sql_baseline_and_clock_capture_order_unchanged(self):
        for key, value in [('unchanged', False), ('contentSha256', 'f' * 64), ('revision', 5),
                           ('captureIds', [IDS[1], IDS[0]])]:
            self.refuse(lambda r: r['stages'][3]['storage']['release'].update({key: value}))
        self.refuse(lambda r: r['stages'][3]['storage']['release']['journal']['drafts'][0].update(revision=8))
        self.refuse(lambda r: r['stages'][4]['storage']['hold'].update(contentSha256='0' * 64))
        self.refuse(lambda r: r.update(storage={}))

    def test_unsaved_B_runtime_native_exact_and_no_target(self):
        self.refuse(lambda r: r['stages'][2]['reply']['runtime'].update(blocks=O.blocks(1)))
        self.refuse(lambda r: r['stages'][2]['layout']['native'].update(blocks=O.blocks(1)))
        self.refuse(lambda r: r['stages'][2]['reply']['runtime'].update(capture=IDS[1]))
        self.refuse(lambda r: r['stages'][2].update(targetReads=reader(O.D.CANVAS, True)))
        self.refuse(lambda r: r['stages'][2]['proof']['retained'].update(pendingFile=True))

    def test_failure_not_cleared_by_generic_file_notice(self):
        self.refuse(lambda r: r['stages'][2]['reply']['runtime'].update(error=None))
        self.refuse(lambda r: r['stages'][2]['reply']['runtime'].update(saving=True))
        self.refuse(lambda r: r['stages'][2]['layout']['controls']['alerts'][0].update(text='This file could not be opened.'))
        self.refuse(lambda r: r['stages'][2]['layout'].update(alerts=[]))

    def test_retry_error_visible_send_disabled(self):
        self.refuse(lambda r: r['stages'][2]['layout']['controls']['send'].update(disabled=False))
        self.refuse(lambda r: r['stages'][2]['layout']['controls']['retry'].update(disabled=True))
        for key in ('retry',):
            self.refuse(lambda r: r['stages'][2]['layout']['controls'][key].update(hit=False))
        self.refuse(lambda r: r['stages'][2]['layout']['controls']['alerts'][0].update(rect=rect(y=999)))
        self.refuse(lambda r: r['stages'][3]['layout']['controls'].update(retry=visible('Retry local save')))

    def test_specific_save_error_remains_single_visible_composer_alert(self):
        for change in ({'visible': False}, {'owner': 'panel'},
                       {'text': 'This file could not be opened. Your reply draft is retained here.'}):
            self.refuse(lambda r: r['stages'][2]['layout']['panelAlerts'][1].update(change))
        def duplicate(r):
            layout = r['stages'][2]['layout']
            layout['panelAlerts'].append({'text': O.ERROR, 'visible': True, 'owner': 'panel'})
            layout['alerts'].append(O.ERROR)
            layout['controls']['alerts'].append(visible(O.ERROR))
        self.refuse(duplicate)
        self.refuse(lambda r: r['stages'][2]['layout']['controls'].update(alerts=[]))

    def test_hidden_internal_alert_and_raw_error_lifecycle_preserved(self):
        self.refuse(lambda r: r['stages'][0]['layout'].pop('panelAlerts'))
        self.refuse(lambda r: r['stages'][0]['layout']['panelAlerts'][0].update(visible=True))
        self.refuse(lambda r: r['stages'][0]['layout']['panelAlerts'][0].update(owner='panel'))
        self.refuse(lambda r: r['stages'][2]['layout']['panelAlerts'][1].update(visible=1))
        self.refuse(lambda r: r['stages'][2]['layout']['alerts'].pop())
        self.refuse(lambda r: r['stages'][3]['layout']['panelAlerts'].append(
            {'text': O.ERROR, 'visible': True, 'owner': 'composer'}))

    def test_retry_persists_same_B_capture_revision_scope(self):
        for key, value in [('capture', IDS[0]), ('revision', 2), ('id', IDS[0]), ('text', O.text(1)),
                           ('blocks', O.blocks(1))]:
            self.refuse(lambda r: r['stages'][3]['reply']['journal']['drafts'][0].update({key: value}))
        self.refuse(lambda r: r['stages'][3]['reply']['runtime'].update(error=O.ERROR))
        self.refuse(lambda r: r['stages'][4]['reply']['journal']['drafts'][0].update(revision=4))

    def test_reopen_exact_B_opaque_styles_and_link(self):
        for fn in (lambda b: b[0]['elements'][0]['elements'][0].pop('style'),
                   lambda b: b[0]['elements'][0]['elements'][1].update(url='https://other.test'),
                   lambda b: b[1].pop('opaque'),
                   lambda b: b[1]['elements'][0]['elements'][0].pop('fixture_opaque')):
            self.refuse(lambda r: fn(r['stages'][5]['reply']['journal']['drafts'][0]['blocks']))
        self.refuse(lambda r: r['stages'][5]['reply']['journal']['drafts'][0].update(capture=IDS[1]))

    def test_original_native_remount_and_retirement(self):
        self.refuse(lambda r: r['stages'][1]['layout']['native'].update(sameNode=False))
        for i in (2, 3, 5):
            self.refuse(lambda r: r['stages'][i]['layout']['native'].update(sameMount=True))
            self.refuse(lambda r: r['stages'][i]['layout']['native'].update(sameBlocks=True))
        self.refuse(lambda r: r['stages'][4]['reply']['runtime'].update(nativePresent=True))

    def test_real_file_click_and_fresh_departure_not_autoroute(self):
        self.refuse(lambda r: r['refusedClick'].update(prevented=[False]))
        self.refuse(lambda r: r['lockedEdit'].update(text=' Locked tail ???'))
        self.refuse(lambda r: r['lockedEdit'].update(blocks=O.blocks(1)))
        self.refuse(lambda r: r['departure']['reply']['journal']['drafts'][0].update(capture=IDS[1]))
        self.refuse(lambda r: r['stages'][3]['proof']['resolver']['requests'].append({}))

    def test_no_send_history_or_network(self):
        self.refuse(lambda r: r['stages'][3]['reply']['journal'].update(attempt={'sent': True}))
        self.refuse(lambda r: r['stages'][3]['reply']['journal']['history'].append({'sent': True}))
        self.refuse(lambda r: r['stages'][4]['targetReads']['canvas'].update(networkCalls=1))
        self.refuse(lambda r: r['stages'][4]['targetReads']['canvas'].update(httpCapability=True))

    def test_target_raw_reader_and_source_authority(self):
        self.refuse(lambda r: r['stages'][4]['targetReads']['canvas']['requests'][4]['response'].update(html='fake'))
        self.refuse(lambda r: r['stages'][4]['targetReads']['list']['responses'][0]['responses'][1]['records'][0].update(id='wrong'), O.D.LIST)
        self.refuse(lambda r: r['stages'][4]['proof']['resolver']['responses'][0]['file'].update(id=O.D.LIST))
        self.refuse(lambda r: r['stages'][4]['proof']['retained']['catalog']['origins'][0].update(firstId='FCANON001'))
        self.refuse(lambda r: r['stages'][4]['proof']['media'][0].update(registered=1))

    def test_stage_terminal_geometry_closed(self):
        self.refuse(lambda r: r['stages'].pop())
        self.refuse(lambda r: r['stages'][2].update(ordinal=True))
        self.refuse(lambda r: r.update(proof={}))
        self.refuse(lambda r: r['stages'][4]['layout']['body'].update(text='wrong target'))
        self.refuse(lambda r: r['stages'][4]['layout']['close'].update(hit=False))

    def test_runner_scope_counts_flags_and_controller_hook(self):
        p = Path(__file__).with_name('check-canvas-canonical-native-storage.py')
        spec = importlib.util.spec_from_file_location('storage_runner_test', p)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        self.assertEqual(mod.SPEC.stages, O.STAGES)
        self.assertEqual(mod.SPEC.counts, {'requestCount': 4, 'resolverCount': 2, 'targetReadCount': 1})
        with tempfile.TemporaryDirectory() as d:
            flags = mod.private_checks(mod.SPEC, Path(d))
            self.assertEqual(flags['SUPER_PLATINUM_CANVAS_DESTINATION_STORAGE_CHECK'], '1')
            self.assertEqual(flags['SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK'], '1')
            with self.assertRaises(RuntimeError):
                mod.private_checks(copy.copy(mod.SPEC), Path(d))
        r = sample()
        r.update(ok=True, noSession=True, noTransport=True, localSaveRefused=True,
                 localSaveRecovered=True, draftPreserved=True, freshTarget=True, nativeClose=True,
                 requestCount=4, resolverCount=2, targetReadCount=1)
        mod.validate(mod.SPEC, fixture(O.D.CANVAS), r)
        with self.assertRaises(RuntimeError):
            mod.validate(mod.SPEC, fixture(O.D.LIST), r)
        with patch.object(mod.canvas_native, 'run') as run, \
             patch.object(mod.canvas_native, 'validate'), \
             patch.object(mod.canvas_native, 'validate_closed'), \
             patch.object(mod.canvas_native, 'private_checks'), \
             patch('sys.argv', [str(p), '--fixture', fixture(O.D.CANVAS)]):
            mod.main()
            self.assertEqual(run.call_count, 1)
            self.assertIs(run.call_args.kwargs['storage_factory'], mod.StorageControl)


if __name__ == '__main__':
    unittest.main()
