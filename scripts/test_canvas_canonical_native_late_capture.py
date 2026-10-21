"""Portable adversarial late-capture receipts; never launches native processes."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import canvas_canonical_native_late_capture as O
from test_canvas_canonical_native_destinations import sample as destination_sample, reader
from test_canvas_canonical_native_storage import IDS, DRAFT, fixture
from test_canvas_canonical_file_roots import read, retained, visible

MOUNT = '55555555-5555-4555-8555-555555555555'
NEXT = '66666666-6666-4666-8666-666666666666'
REOPENED = '77777777-7777-4777-8777-777777777777'
REQUEST = '88888888-8888-4888-8888-888888888888'
OWNER = {'scope': DRAFT + ':1:2', 'mount': MOUNT, 'generation': 2}


def bridge_sample(index):
    value = {**OWNER, 'phase': 'idle', 'closed': False, 'heldPayload': None,
             'sends': 2, 'finalSends': 0, 'releaseCount': 0, 'failure': None,
             'finishRequests': [], 'heldAt': None, 'releasedAt': None}
    if index >= 2:
        value.update(phase='held', heldAt=1000, heldPayload={
            **OWNER, 'kind': 'capture', 'request': REQUEST, 'sequence': 3,
            'revision': 8, 'blocks': O.D.C.edited(), 'composing': False,
            'detached': False, 'finalCapture': True},
            finishRequests=[{'kind': 'finish', 'scope': OWNER['scope'],
                             'mount': MOUNT, 'request': REQUEST}])
    if index >= 4:
        value.update(phase='delivered', closed=True, releasedAt=8000,
                     sends=3, finalSends=1, releaseCount=1)
    return value


def sample(id=O.D.CANVAS, viewport=(668, 380)):
    old = destination_sample(id, viewport)
    stages = []
    for i, (name, count, resolves) in enumerate(zip(O.STAGES, O.READS, O.RESOLVES)):
        stage = copy.deepcopy(old['stages'][0 if i == 0 else 1])
        stage.update(stage=name, ordinal=i)
        rows = [O.D.roots(id)[0], O.D.D.REPLY]
        value = retained(rows, True, count)
        serial = 4 if i == 7 else 3 if i >= 4 else 2
        value['serial'] = serial
        raw = rows[0]['files'][-1]
        value.update(pendingFile=i == 6, navigationError=O.ERROR if i in (2, 3, 4, 5) else None,
                     catalog={'serial': serial,
            'rows': [{'id': id, 'source': raw, 'effective': raw, 'current': True}],
            'origins': [{'ts': O.D.D.ROOT, 'firstId': id, 'blockId': 'original-' + O.D.D.ROOT}]})
        stage['proof'] = {
            'requests': [read('replies', O.D.D.ROOT) if j % 2 else read() for j in range(count)],
            'responses': [O.D.response(id, thread=j % 2 == 1) for j in range(count)],
            'originals': copy.deepcopy(rows), 'retained': value,
            'media': copy.deepcopy(old['stages'][0]['proof']['media']),
            'resolver': {'requests': [{'method': 'POST',
                                      'url': 'https://fixture.slack.com/api/files.info',
                                      'fields': [['file', id]]} for _ in range(resolves)],
                         'responses': [{'ok': True, 'file': O.D.target_file(id)} for _ in range(resolves)],
                         'settlements': [{'serial': j, 'delivered': True} for j in range(1, resolves + 1)],
                         'held': None, 'releases': []}}
        stage['targetReads'] = reader(id, i >= 6)
        runtime = stage['reply']['runtime']
        runtime.update(capture=IDS[min(i, 1)], serial=1 if i == 0 else 2,
                       error=O.ERROR if i in (2, 3, 4) else None, nativePresent=i != 6)
        item = stage['reply']['journal']['drafts'][0]
        item.update(id=DRAFT, capture=IDS[min(i, 1)], revision=1 if i == 0 else 2)
        native = {**OWNER, 'sequence': 1 if i == 0 else 2,
                  'failed': False, 'originalCompletion': i <= 3}
        if i >= 4:
            native.update(mount=REOPENED if i == 7 else NEXT,
                          generation=4 if i == 7 else 3, sequence=1,
                          scope=DRAFT + (':2:4' if i == 7 else ':1:3'))
        stage['lateRuntime'] = {**copy.deepcopy(runtime), 'nativeBusy': i in (2, 3),
                                'owner': None if i == 6 else native}
        stage['bridge'] = bridge_sample(i)
        if i == 6:
            stage['layout'] = copy.deepcopy(old['stages'][4]['layout'])
            stage['layout'].update(editor=None, controls={'retry': None, 'send': None, 'alerts': []})
        else:
            stage['layout']['native'].update(
                **{k: native[k] for k in ('scope', 'mount', 'generation')},
                sameNode=i in (1, 2, 3), sameMount=i in (1, 2, 3), sameBlocks=i != 0)
            stage['layout']['targetKind'] = 'retry' if i in (2, 3, 4) else 'editor-line'
            stage['layout']['alerts'] = [''] + (
                [O.ERROR, O.ERROR] if i in (2, 3, 4) else [O.ERROR] if i == 5 else [])
            stage['layout']['panelAlerts'] = [
                {'text': '', 'visible': False, 'owner': 'composer'}]
            if i in (2, 3, 4):
                stage['layout']['panelAlerts'].extend([
                    {'text': O.ERROR, 'visible': True, 'owner': 'composer'},
                    {'text': O.ERROR, 'visible': False, 'owner': 'panel'}])
            elif i == 5:
                stage['layout']['panelAlerts'].append(
                    {'text': O.ERROR, 'visible': True, 'owner': 'panel'})
            stage['layout']['retry'] = dict(visible('Retry local save'),
                                            disabled=i in (2, 3)) if i in (2, 3, 4) else None
            stage['layout']['controls'] = {
                'send': dict(visible('Send reply'), disabled=i in (2, 3, 4), inScroll=True),
                'retry': dict(visible('Retry local save'), disabled=i in (2, 3),
                              inScroll=True) if i in (2, 3, 4) else None,
                'alerts': [dict(visible(O.ERROR), disabled=None, inScroll=True)] if i in (2, 3, 4, 5) else []}
            stage['layout']['editor'] = {'connected': True, 'running': True,
                'contentEditable': 'false' if i in (2, 3) else 'true', 'finishing': i in (2, 3)}
        stages.append(stage)
    arm = bridge_sample(1)
    arm['phase'] = 'armed'
    release = bridge_sample(4)
    release.update(phase='released', closed=False, sends=2, finalSends=0)
    return {'destinationId': id, 'stages': stages, 'proof': copy.deepcopy(stages[-1]['proof']),
            'layout': copy.deepcopy(stages[-1]['layout']),
            'late': {'arm': {'ok': True, 'bridge': arm}, 'timeoutElapsedMs': 6100,
                     'firstClick': {'ok': True, 'prevented': [True]},
                     'duplicateClick': {'ok': True, 'prevented': [True]},
                     'beforeRelease': {'ok': True, 'bridge': bridge_sample(3),
                        'controls': copy.deepcopy(stages[3]['layout']['controls']),
                        'editor': copy.deepcopy(stages[3]['layout']['editor'])},
                     'release': {'ok': True, 'bridge': release},
                     'afterRelease': {'ok': True, 'bridge': bridge_sample(4),
                        'controls': copy.deepcopy(stages[4]['layout']['controls']),
                        'editor': copy.deepcopy(stages[4]['layout']['editor'])}},
            'departure': {'native': copy.deepcopy(stages[5]['layout']),
                          'reply': copy.deepcopy(stages[5]['reply'])}}


class LateCaptureOracle(unittest.TestCase):
    def refuse(self, fn, id=O.D.CANVAS):
        value = sample(id)
        fn(value)
        with self.assertRaises((RuntimeError, KeyError, TypeError, ValueError)):
            O.validate(value, fixture(id))

    def test_both_targets_both_widths(self):
        for id in (O.D.CANVAS, O.D.LIST):
            for viewport in ((668, 380), (1088, 728)):
                O.validate(sample(id, viewport), fixture(id, viewport == (668, 380)))

    def test_exact_edited_unicode_and_opaque_content(self):
        self.assertEqual([ord(c) for c in O.D.C.edited()[1]['elements'][0]['elements'][0]['text'][-2:]],
                         [0x4e2d, 0x1f98a])
        for mutate in (
            lambda b: b[0]['elements'][0]['elements'][0].pop('style'),
            lambda b: b[0]['elements'][0]['elements'][1].update(url='https://other.test'),
            lambda b: b[1].pop('opaque'),
            lambda b: b[1]['elements'][0]['elements'][0].pop('fixture_opaque')):
            self.refuse(lambda r: mutate(r['stages'][7]['reply']['journal']['drafts'][0]['blocks']))

    def test_held_owner_mount_scope_and_request_bound(self):
        for key, value in [('mount', NEXT), ('scope', 'foreign'), ('generation', 9),
                           ('request', ''), ('sequence', True), ('revision', -1)]:
            self.refuse(lambda r: r['stages'][2]['bridge']['heldPayload'].update({key: value}))
        self.refuse(lambda r: r['stages'][2]['bridge'].update(mount=NEXT))
        self.refuse(lambda r: r['stages'][2]['lateRuntime']['owner'].update(originalCompletion=False))
        self.refuse(lambda r: (r['stages'][4]['proof']['retained'].update(serial=99),
                               r['stages'][4]['proof']['retained']['catalog'].update(serial=99)))

    def test_held_must_be_real_noncomposing_attached_final_capture(self):
        for key, value in [('kind', 'capture-result'), ('finalCapture', False),
                           ('detached', True), ('composing', True), ('blocks', O.D.C.blocks())]:
            self.refuse(lambda r: r['stages'][2]['bridge']['heldPayload'].update({key: value}))

    def test_timeout_retains_native_charge_and_same_node(self):
        for i in (2, 3):
            self.refuse(lambda r: r['stages'][i]['lateRuntime'].update(nativeBusy=False))
            self.refuse(lambda r: r['stages'][i]['layout']['native'].update(sameNode=False))
            self.refuse(lambda r: r['stages'][i]['reply']['runtime'].update(nativePresent=False))
            self.refuse(lambda r: r['stages'][i]['bridge'].update(finalSends=1))

    def test_duplicate_click_may_resolve_but_cannot_finish(self):
        self.refuse(lambda r: r['stages'][3]['proof']['resolver']['requests'].pop())
        self.refuse(lambda r: r['stages'][3]['bridge']['finishRequests'].append(
            {'kind': 'finish', 'scope': OWNER['scope'], 'mount': MOUNT, 'request': NEXT}))
        self.refuse(lambda r: r['stages'][3]['bridge'].update(sends=3))
        self.refuse(lambda r: r['stages'][3].update(targetReads=reader(O.D.CANVAS, True)))

    def test_late_final_original_delivered_exactly_once(self):
        for key, value in [('finalSends', 0), ('finalSends', 2), ('releaseCount', 2),
                           ('closed', False), ('phase', 'released'), ('sends', 4)]:
            self.refuse(lambda r: r['stages'][4]['bridge'].update({key: value}))
        self.refuse(lambda r: r['stages'][4]['bridge']['heldPayload'].update(sequence=4))
        self.refuse(lambda r: r['stages'][4]['bridge'].update(heldAt=1001))
        self.refuse(lambda r: r['stages'][4]['bridge'].update(releasedAt=6999))

    def test_late_completion_remounts_without_auto_destination(self):
        self.refuse(lambda r: r['stages'][4]['lateRuntime'].update(nativeBusy=True))
        self.refuse(lambda r: r['stages'][4]['lateRuntime']['owner'].update(mount=MOUNT))
        self.refuse(lambda r: r['stages'][4]['lateRuntime']['owner'].update(originalCompletion=True))
        self.refuse(lambda r: r['stages'][4]['layout']['native'].update(sameMount=True))
        self.refuse(lambda r: r['stages'][4].update(targetReads=reader(O.D.CANVAS, True)))

    def test_retry_disabled_until_terminal_then_actual_recovery(self):
        self.refuse(lambda r: r['stages'][2]['layout']['retry'].update(disabled=False))
        self.refuse(lambda r: r['stages'][4]['layout']['retry'].update(disabled=True))
        self.refuse(lambda r: r['stages'][4]['layout']['retry'].update(hit=False))
        self.refuse(lambda r: r['stages'][4]['layout'].update(alerts=[]))
        self.refuse(lambda r: r['stages'][5]['reply']['runtime'].update(error=O.ERROR))
        self.refuse(lambda r: r['stages'][2]['layout']['controls']['send'].update(disabled=False))
        self.refuse(lambda r: r['stages'][2]['layout']['editor'].update(contentEditable='true'))
        self.refuse(lambda r: r['stages'][4]['layout']['editor'].update(finishing=True))
        self.refuse(lambda r: r['stages'][2]['layout']['controls']['alerts'][0].update(hit=False))

    def test_actual_timing_original_release_and_fresh_departure(self):
        for value in (5999, True, float('nan'), 60000):
            self.refuse(lambda r: r['late'].update(timeoutElapsedMs=value))
        self.refuse(lambda r: r['late']['arm']['bridge'].update(mount=NEXT))
        self.refuse(lambda r: r['late']['release']['bridge'].update(finalSends=1))
        self.refuse(lambda r: r['late']['release']['bridge']['heldPayload'].update(request=NEXT))
        self.refuse(lambda r: r['late']['beforeRelease']['editor'].update(contentEditable='true'))
        self.refuse(lambda r: r['late']['afterRelease']['bridge'].update(closed=False))
        self.refuse(lambda r: r['late']['duplicateClick'].update(prevented=[False]))
        self.refuse(lambda r: r['departure']['reply']['journal']['drafts'][0].update(capture=IDS[2]))

    def test_explicit_retry_does_not_clear_unrelated_navigation_alert(self):
        self.refuse(lambda r: r['stages'][5]['proof']['retained'].update(navigationError=None))
        self.refuse(lambda r: r['stages'][6]['proof']['retained'].update(navigationError=O.ERROR))
        self.refuse(lambda r: r['stages'][5]['layout']['panelAlerts'][1].update(visible=False))
        self.refuse(lambda r: r['stages'][5]['layout']['panelAlerts'][1].update(owner='composer'))
        # Layout-visible duplicates remain duplicates even when scrolled offscreen.
        self.refuse(lambda r: r['stages'][2]['layout']['controls']['alerts'].append(
            dict(visible(O.ERROR), inScroll=False, inView=False, hit=False)))

    def test_only_matching_panel_duplicate_hidden_while_composer_wins(self):
        for i in (2, 3, 4):
            self.refuse(lambda r: r['stages'][i]['layout']['panelAlerts'][2].update(visible=True))
            self.refuse(lambda r: r['stages'][i]['layout']['panelAlerts'][1].update(visible=False))
            self.refuse(lambda r: r['stages'][i]['layout']['panelAlerts'][1].update(owner='panel'))
            self.refuse(lambda r: r['stages'][i]['layout']['panelAlerts'][2].update(owner='composer'))
            self.refuse(lambda r: r['stages'][i]['layout']['panelAlerts'][2].update(text='Different error'))
            self.refuse(lambda r: r['stages'][i]['layout']['panelAlerts'].pop())

    def test_raw_duplicates_retained_and_hidden_internal_alert_not_promoted(self):
        self.refuse(lambda r: r['stages'][2]['layout']['alerts'].pop())
        self.refuse(lambda r: r['stages'][4]['layout']['panelAlerts'][0].update(visible=True))
        self.refuse(lambda r: r['stages'][2]['layout']['panelAlerts'][2].update(visible=0))
        self.refuse(lambda r: r['stages'][5]['layout']['controls'].update(alerts=[]))
        self.refuse(lambda r: r['stages'][7]['layout']['panelAlerts'].append(
            {'text': O.ERROR, 'visible': True, 'owner': 'panel'}))

    def test_original_journal_id_revision_capture_stable(self):
        for key, value in [('id', NEXT), ('revision', 3), ('capture', IDS[2])]:
            self.refuse(lambda r: r['stages'][4]['reply']['journal']['drafts'][0].update({key: value}))
        self.refuse(lambda r: r['stages'][3]['reply']['journal'].update(attempt={'sent': True}))
        self.refuse(lambda r: r['stages'][3]['reply']['journal']['history'].append({'sent': True}))
        self.refuse(lambda r: r['stages'][3]['lateRuntime'].update(text='different'))

    def test_closed_original_bridge_stays_inert(self):
        self.refuse(lambda r: r['stages'][5]['bridge'].update(sends=4))
        self.refuse(lambda r: r['stages'][6]['bridge'].update(mount=NEXT))
        self.refuse(lambda r: r['stages'][7]['bridge'].update(failure='late failure'))

    def test_fresh_target_raw_factories_and_no_network(self):
        self.refuse(lambda r: r['stages'][6]['targetReads']['canvas']['requests'][4]['response'].update(html='fake'))
        self.refuse(lambda r: r['stages'][6]['targetReads']['list']['responses'][0]['responses'][1]['records'][0].update(id='wrong'), O.D.LIST)
        self.refuse(lambda r: r['stages'][6]['targetReads']['canvas'].update(networkCalls=1))
        self.refuse(lambda r: r['stages'][6]['proof']['resolver']['responses'][0]['file'].update(id=O.D.LIST))

    def test_original_source_not_resolver_authority(self):
        self.refuse(lambda r: r['stages'][4]['proof']['retained']['catalog']['origins'][0].update(firstId=O.D.LIST))
        self.refuse(lambda r: r['stages'][4]['proof']['media'][0].update(registered=1))
        self.refuse(lambda r: r['stages'][4]['proof']['responses'][0]['messages'][0].update(text='different'))

    def test_stages_terminal_and_visible_destination(self):
        self.refuse(lambda r: r['stages'].pop())
        self.refuse(lambda r: r['stages'][3].update(ordinal=True))
        self.refuse(lambda r: r.update(proof={}))
        self.refuse(lambda r: r['stages'][6]['layout']['body'].update(text='wrong destination'))
        self.refuse(lambda r: r['stages'][6]['layout']['close'].update(hit=False))

    def test_runner_flags_private_db_counts_and_no_storage_lock(self):
        path = Path(__file__).with_name('check-canvas-canonical-native-late-capture.py')
        spec = importlib.util.spec_from_file_location('late_capture_runner_test', path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.assertEqual(module.SPEC.stages, O.STAGES)
        self.assertEqual(module.SPEC.counts, {'requestCount': 4, 'resolverCount': 3, 'targetReadCount': 1})
        with tempfile.TemporaryDirectory() as directory:
            checks = module.private_checks(module.SPEC, Path(directory))
            self.assertEqual(checks['SUPER_PLATINUM_CANVAS_DESTINATION_LATE_CAPTURE_CHECK'], '1')
            self.assertEqual(checks['SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK'], '1')
            self.assertIn('SUPER_PLATINUM_CANVAS_REPLY_DB', checks)
            with self.assertRaises(RuntimeError):
                module.private_checks(copy.copy(module.SPEC), Path(directory))
        r = sample()
        r.update(ok=True, noSession=True, noTransport=True, timedOut=True,
                 competingFinishRefused=True, lateCaptureRecovered=True, draftPreserved=True,
                 freshTarget=True, nativeClose=True, requestCount=4, resolverCount=3, targetReadCount=1)
        module.validate(module.SPEC, fixture(O.D.CANVAS), r)
        with self.assertRaises(RuntimeError):
            module.validate(module.SPEC, fixture(O.D.LIST), r)
        with patch.object(module.canvas_native, 'run') as run, \
             patch.object(module.canvas_native, 'validate'), \
             patch.object(module.canvas_native, 'validate_closed'), \
             patch.object(module.canvas_native, 'private_checks'), \
             patch('sys.argv', [str(path), '--fixture', fixture(O.D.CANVAS)]):
            module.main()
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.kwargs, {})


if __name__ == '__main__':
    unittest.main()
