"""Independent synthetic receipts and adversaries; never launches the app."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import canvas_cold_file_roots as O
from test_canvas_canonical_file_roots import rect, read, retained, visible


def acquisition(index):
    count = O.ACQUISITIONS[index]
    settlements = [{'serial': j, 'cancelled': j == 3} for j in range(1, count + 1)
                   if not (index == 2 and j == 3)]
    return {'requests': [{'serial': i + 1, 'id': O.ID, 'method': 'files.info',
                         'path': '/api/files.info', 'args': copy.deepcopy(O.ARGS),
                         'mode': mode, 'reason': 'files-store-unknown-fetch'}
                        for i, mode in enumerate(O.MODES[:count])],
            'responses': [{'serial': i + 1, 'id': O.ID, 'disposition': mode,
                           'raw': O.raw_response(mode)} for i, mode in enumerate(O.MODES[:count])],
            'settlements': settlements, 'completed': len(settlements),
            'cancelled': int(index >= 3),
            'held': [{'serial': 3, 'id': O.ID, 'receiverClosed': False}] if index == 2 else [],
            'releases': [{'serial': 3, 'id': O.ID, 'delivered': False,
                          'cancelledBeforeReply': True}] if index >= 3 else [],
            'networkCalls': 0, 'httpCapability': False}


def sample(viewport=(668, 380)):
    stages = []
    for i, (name, count) in enumerate(zip(O.STAGES, O.READS)):
        rows = [O.roots()[0], O.D.REPLY]
        value = retained(rows, True, count)
        value.update(pendingFile=i == 9, catalog={
            'serial': count, 'origins': [{'ts': O.D.ROOT, 'firstId': O.ID,
                                         'blockId': 'original-' + O.D.ROOT}],
            'rows': [{'id': O.ID, 'source': O.positive()['file'], 'effective': O.positive()['file'],
                      'current': True, 'link': {'name': O.NAME, 'url': O.D.file(O.ID)['permalink']}}]
                    if i >= 8 else []})
        value['coldAcquisition'] = {'cooling': i in (5, 6), 'running': i == 2, 'rows': [] if i == 10 else [
            {'id': O.ID, 'rootTs': O.D.ROOT, 'phase': 'ready' if i >= 8 else O.PHASES[i],
             'current': i != 3, 'retryable': i in (0, 1, 4, 7), 'notice': O.STORED_NOTICES[i],
             'receipt': O.positive() if i >= 8 else {'ok': True, 'file': False} if i == 4 else None}]}
        target = O.D.file(O.ID)
        target.update(title='Fresh hosted preview', mimetype='image/svg+xml', url_private=O.C.PREVIEW)
        target.pop('thumb_360')
        resolver = {'requests': [{'method': 'POST', 'url': 'https://fixture.slack.com/api/files.info',
                                 'fields': [['file', O.ID]]}] if i >= 9 else [],
                    'responses': [{'ok': True, 'file': target}] if i >= 9 else [],
                    'settlements': [{'serial': 1, 'delivered': True}] if i >= 9 else [],
                    'held': None, 'releases': []}
        proof = {'requests': [read('replies', O.D.ROOT) if j in (1, 2, 4, 6) else read()
                              for j in range(count)], 'responses': O.responses()[:count],
                 'retained': value, 'originals': rows, 'resolver': resolver,
                 'media': [{'url': url, 'registered': 0} for url in O.D.URLS], 'cold': acquisition(i)}
        blocks = O.C.blocks() if not i else O.C.edited()
        text = ('Keep exact \u4e2d\U0001f98a & draft link e\u0301.\nOpaque attributes retained.'
                + (' Canonical tail \u4e2d\U0001f98a' if i else ''))
        reply = {'runtime': {'blocks': blocks, 'loading': False, 'saving': False, 'sending': False,
                             'error': None, 'text': text, 'capture': 'capture-b' if i else 'capture-a',
                             'serial': 1, 'nativePresent': i != 9},
                 'journal': {'drafts': [{'blocks': blocks, 'id': 'draft-a', 'text': text,
                                        'revision': 2 if i else 1,
                                        'capture': 'capture-b' if i else 'capture-a'}],
                             'attempt': None, 'history': []}}
        native = {'blocks': blocks, 'composing': False, 'running': True, 'scope': 'original-scope',
                  'mount': 'mount-1' if i < 4 else 'mount-2' if i < 6 else 'mount-3' if i < 10 else 'mount-4',
                  'generation': 1 if i < 4 else 2 if i < 6 else 3 if i < 10 else 4,
                  'sameNode': i in (1, 2, 3), 'sameMount': i in (1, 2, 3), 'sameBlocks': i > 0}
        layout = {'ok': True, 'viewport': list(viewport), 'inScroll': True,
                  'scroll': rect(0, 0, 600, 300), 'target': visible('actual target'),
                  'targetKind': 'viewer-title' if i == 9 else 'editor-line' if i in (1, 10)
                                else 'link' if i == 8 else 'metadata-status',
                  'native': None if i == 9 else native, 'images': [],
                  'body': O.NAME if i >= 8 else rows[0]['text'],
                  'status': {'text': O.NOTICES[i], 'phase': O.PHASES[i]} if O.PHASES[i] else None,
                  'retry': dict(visible('Retry file details'), disabled=i in (5, 6),
                                style={'opacity': .45 if i in (5, 6) else 1,
                                       'cursor': 'default' if i in (5, 6) else 'pointer'})
                           if i in (0, 1, 4, 5, 6, 7) else None,
                  'link': {'id': O.ID, 'name': O.NAME, 'url': O.D.file(O.ID)['permalink'],
                           'root': O.D.ROOT} if i >= 8 else None,
                  'viewer': {'title': 'Fresh hosted preview', 'ready': True,
                             'close': visible('Close')} if i == 9 else None}
        stages.append({'ok': True, 'noSession': True, 'noTransport': True,
                       'stage': name, 'ordinal': i, 'proof': proof, 'reply': reply,
                       'layout': layout, 'acquisition': acquisition(i)})
    return copy.deepcopy({'rateElapsedMs': 15001, 'rateRetained': True, 'explicitRetry': True, 'stages': stages, **{k: stages[-1][k] for k in
                                               ('proof', 'layout', 'acquisition')}})


class ColdOracle(unittest.TestCase):
    def refuse(self, change):
        receipt = sample()
        change(receipt)
        with self.assertRaises((RuntimeError, KeyError, TypeError, ValueError)):
            O.validate(receipt)

    def test_complete_normal_and_narrow(self):
        O.validate(sample())
        O.validate(sample((1088, 728)))

    def test_loading_derived_ui_notice_is_not_stored_failure(self):
        receipt = sample()
        self.assertIsNone(receipt['stages'][2]['proof']['retained']['coldAcquisition']['rows'][0]['notice'])
        self.assertEqual(receipt['stages'][2]['layout']['status']['text'], 'Loading file details...')
        O.validate(receipt)
        self.refuse(lambda r: r['stages'][2]['proof']['retained']['coldAcquisition']['rows'][0].update(notice=O.LOADING))
        self.refuse(lambda r: r['stages'][2]['layout']['status'].update(text=None))

    def test_unchanged_first_native_receipt_and_closed_log_regression(self):
        directory = Path(__file__).resolve().parents[1] / 'tmp/canvas-cold-file-roots-20261020/integration/cold-normal-a'
        path = directory / 'canvas-canonical-file-custody.json'
        if not path.exists():
            self.skipTest('Retained first native failure evidence is not available')
        evidence = json.loads(path.read_text(encoding='utf-8'))
        self.assertFalse(evidence['ok'])
        self.assertIn('Actual owner-bound acquisition state/raw receipt differs', evidence['error'])
        self.assertEqual(evidence['closedTerminalReceipts'], [evidence['native']])
        # Historical acceptance remains replayable under its original contract.
        prior = Path(__file__).resolve().parents[1] / 'tmp/canvas-file-retry-guidance-20261020/prior-oracle/canvas_cold_file_roots.py'
        spec = importlib.util.spec_from_file_location('cold_prior_guidance_oracle', prior)
        old = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(old)
        old.validate(evidence['native'])
        with self.assertRaises(RuntimeError):
            O.validate(evidence['native'])
        from canvas_evidence import closed_receipts
        log = directory / 'canvas-canonical-file-custody.log'
        self.assertEqual(closed_receipts(log, O.PREFIX + '-check: '), [evidence['native']])
        stages = [{k: v for k, v in row.items() if k != 'screenshot'} for row in evidence['stages']]
        self.assertEqual(closed_receipts(log, O.PREFIX + '-stage: '), stages)
        self.assertEqual(stages, evidence['native']['stages'])

    def test_unicode_fixture_not_matching_corrupted_oracle(self):
        self.assertEqual([ord(c) for c in O.NAME[-2:]], [0x4e2d, 0x1f98a])
        self.assertEqual(O.roots()[0]['files'], [{'id': 'FCANON001', 'file_access': 'check_file_info'}])
        self.assertNotEqual(O.positive()['file']['title'], O.NAME)

    def test_original_read_prefix_and_raw_body_not_replaced_by_metadata(self):
        self.refuse(lambda r: r['stages'][8]['proof']['responses'][2]['messages'][0].update(files=[O.positive()['file']]))
        self.refuse(lambda r: r['stages'][8]['proof']['originals'][0]['blocks'].clear())
        self.refuse(lambda r: r['stages'][8]['proof']['retained']['root']['message'].update(text='canonical'))
        self.refuse(lambda r: r['stages'][3]['proof']['requests'].append(read()))

    def test_singleton_wire_not_generic_navigation(self):
        for field in O.ARGS:
            self.refuse(lambda r: r['stages'][2]['acquisition']['requests'][2]['args'].pop(field))
        self.refuse(lambda r: r['stages'][2]['acquisition']['requests'][2]['args'].update(file='FCANON002'))
        self.refuse(lambda r: r['stages'][2]['acquisition']['requests'][2].update(path='/api/files.list'))
        self.refuse(lambda r: r['stages'][2]['acquisition']['requests'][2].update(reason='native-guessed'))
        self.refuse(lambda r: r['stages'][2]['acquisition']['requests'][2]['args'].update(extra='1'))

    def test_raw_mode_envelopes_retained_before_outcome(self):
        self.refuse(lambda r: r['stages'][0]['acquisition']['responses'][0].update(raw=O.positive()))
        self.refuse(lambda r: r['stages'][2]['acquisition']['responses'][2]['raw']['file'].update(id='FOREIGN'))
        self.refuse(lambda r: r['stages'][4]['acquisition']['responses'][3].update(raw=O.positive()))
        self.refuse(lambda r: r['stages'][8]['acquisition']['responses'][5]['raw']['file'].pop('fixture_opaque'))

    def test_real_retry_after_bound_and_terminal_claims(self):
        for elapsed in (None, True, 14999, 15, -1, '15000'):
            self.refuse(lambda r: r.update(rateElapsedMs=elapsed))
        self.refuse(lambda r: r.update(rateRetained=False))
        self.refuse(lambda r: r.update(explicitRetry=False))
        r = sample()
        r['rateElapsedMs'] = 15000
        O.validate(r)

    def test_cooldown_survives_actual_close_reopen_without_read(self):
        self.assertEqual(O.READS, (2, 2, 2, 2, 3, 3, 5, 5, 5, 5, 7))
        self.assertEqual(O.ACQUISITIONS, (2, 2, 3, 3, 4, 5, 5, 5, 6, 6, 7))
        for stage in (5, 6):
            self.refuse(lambda r: r['stages'][stage]['layout']['retry'].update(disabled=False))
            self.refuse(lambda r: r['stages'][stage]['proof']['retained']['coldAcquisition'].update(cooling=False))
            self.refuse(lambda r: r['stages'][stage]['proof']['retained']['coldAcquisition']['rows'][0].update(retryable=True))
        self.refuse(lambda r: r['stages'][6]['layout']['native'].update(sameNode=True))
        self.refuse(lambda r: r['stages'][6]['proof']['requests'].pop())
        self.refuse(lambda r: r['stages'][6]['acquisition']['requests'].append({}))

    def test_expiry_only_enables_explicit_retry_without_dispatch(self):
        self.refuse(lambda r: r['stages'][7]['layout']['retry'].update(disabled=True))
        self.refuse(lambda r: r['stages'][7]['layout']['status'].update(text=O.FAILED))
        self.refuse(lambda r: r['stages'][7]['proof']['retained']['coldAcquisition'].update(cooling=True))
        self.refuse(lambda r: r['stages'][7]['acquisition']['requests'].append({}))
        self.refuse(lambda r: r['stages'][7]['proof']['retained']['catalog']['rows'].append(
            copy.deepcopy(r['stages'][8]['proof']['retained']['catalog']['rows'][0])))
        self.refuse(lambda r: r['stages'][8]['layout']['native'].update(mount='new-on-success'))

    def test_expired_guidance_differs_from_retained_original_rate_notice(self):
        r = sample()
        self.assertEqual(r['stages'][7]['layout']['status']['text'],
                         'You can retry loading file details now.')
        self.assertEqual(r['stages'][7]['proof']['retained']['coldAcquisition']['rows'][0]['notice'],
                         'Slack asked us to wait before retrying file details.')
        O.validate(r)
        self.refuse(lambda r: r['stages'][7]['layout']['status'].update(text=O.LIMITED))
        self.refuse(lambda r: r['stages'][7]['proof']['retained']['coldAcquisition']['rows'][0].update(notice=O.AVAILABLE))
        self.refuse(lambda r: r['stages'][6]['layout']['status'].update(text=O.AVAILABLE))

    def test_actual_disabled_and_enabled_retry_appearance(self):
        for stage in (5, 6):
            self.refuse(lambda r: r['stages'][stage]['layout']['retry']['style'].update(opacity=1))
            self.refuse(lambda r: r['stages'][stage]['layout']['retry']['style'].update(cursor='pointer'))
        for stage in (0, 1, 4, 7):
            self.refuse(lambda r: r['stages'][stage]['layout']['retry']['style'].update(opacity=.45))
            self.refuse(lambda r: r['stages'][stage]['layout']['retry']['style'].update(cursor='default'))
        for opacity in (True, '1', None, float('nan')):
            self.refuse(lambda r: r['stages'][7]['layout']['retry']['style'].update(opacity=opacity))
        self.refuse(lambda r: r['stages'][7]['layout']['retry'].pop('style'))
        for key in ('width', 'height'):
            self.refuse(lambda r: r['stages'][7]['layout']['retry']['rect'].update({key: 50}))
        r = sample()
        r['stages'][7]['layout']['retry']['rect'].update(x=20, y=40)
        O.validate(r)  # Status wrapping may move the button without resizing it.

    def test_rate_limit_is_original_completed_failure_not_raw_success_or_cancel(self):
        self.refuse(lambda r: r['stages'][5]['acquisition']['responses'][4].update(raw=O.positive()))
        self.refuse(lambda r: r['stages'][5]['acquisition']['requests'][4].update(mode='offline'))
        self.refuse(lambda r: r['stages'][5]['acquisition']['settlements'][4].update(cancelled=True))
        self.refuse(lambda r: r['stages'][5]['proof']['retained']['coldAcquisition']['rows'][0].update(receipt=O.positive()))

    def test_held_cancel_before_release_not_delivered_stale(self):
        self.refuse(lambda r: r['stages'][2]['acquisition']['settlements'].append({'serial': 3, 'cancelled': True}))
        self.refuse(lambda r: r['stages'][3]['acquisition']['settlements'][2].update(cancelled=False))
        self.refuse(lambda r: r['stages'][3]['acquisition']['releases'][0].update(delivered=True))
        self.refuse(lambda r: r['stages'][3]['acquisition']['releases'][0].update(cancelledBeforeReply=False))
        self.refuse(lambda r: r['stages'][2]['acquisition']['held'][0].update(receiverClosed=True))

    def test_absent_until_positive_and_source_not_fresh_target(self):
        self.refuse(lambda r: r['stages'][3]['proof']['retained']['catalog']['rows'].append(
            copy.deepcopy(r['stages'][8]['proof']['retained']['catalog']['rows'][0])))
        self.refuse(lambda r: r['stages'][8]['proof']['retained']['catalog']['rows'][0]['source'].update(name='EMBEDDED'))
        self.refuse(lambda r: r['stages'][8]['proof']['retained']['catalog']['rows'][0].update(current=False))
        self.refuse(lambda r: r['stages'][8]['proof']['retained']['catalog']['origins'][0].update(firstId='FCANON002'))
        self.refuse(lambda r: r['stages'][9]['proof']['retained']['catalog']['rows'][0]['source'].update(title='Fresh hosted preview'))

    def test_no_automatic_navigation_or_extra_read(self):
        self.refuse(lambda r: r['stages'][8]['proof']['resolver']['requests'].append({}))
        self.refuse(lambda r: r['stages'][8]['acquisition']['requests'].append({}))
        self.refuse(lambda r: r['stages'][8]['proof']['retained'].update(pendingFile=True))
        self.refuse(lambda r: r['stages'][8]['layout'].update(viewer={'title': 'auto-open'}))

    def test_native_status_retry_not_missing_disabled_or_wrong(self):
        self.refuse(lambda r: r['stages'][0]['layout'].update(retry=None))
        self.refuse(lambda r: r['stages'][4]['layout']['retry'].update(disabled=True))
        self.refuse(lambda r: r['stages'][4]['layout']['retry'].update(hit=False))
        self.refuse(lambda r: r['stages'][3]['layout'].update(retry=visible('Retry')))
        self.refuse(lambda r: r['stages'][2]['layout']['status'].update(phase='unavailable'))
        self.refuse(lambda r: r['stages'][4]['layout']['status'].update(text=O.OFFLINE))

    def test_metadata_is_not_editor_remount(self):
        self.refuse(lambda r: r['stages'][2]['layout']['native'].update(sameNode=False))
        self.refuse(lambda r: r['stages'][8]['layout']['native'].update(mount='replacement'))
        self.refuse(lambda r: r['stages'][8]['proof']['retained'].update(serial=100))
        self.refuse(lambda r: r['stages'][8]['reply']['runtime'].update(capture='other'))
        self.refuse(lambda r: r['stages'][8]['reply']['runtime'].update(serial=99))

    def test_actual_cold_owner_state_and_original_ledger_bound(self):
        self.refuse(lambda r: r['stages'][2]['proof']['cold'].update(completed=3))
        self.refuse(lambda r: r['stages'][3]['proof']['retained']['coldAcquisition']['rows'][0].update(current=True))
        self.refuse(lambda r: r['stages'][3]['proof']['retained']['coldAcquisition']['rows'][0].update(retryable=True))
        self.refuse(lambda r: r['stages'][8]['proof']['retained']['coldAcquisition']['rows'][0].update(receipt={'ok': True}))
        self.refuse(lambda r: r['stages'][10]['proof']['retained']['coldAcquisition']['rows'].append({}))
        self.refuse(lambda r: r['stages'][4]['proof']['retained']['coldAcquisition'].update(running=True))

    def test_durable_rich_styles_opaque_capture_remain_exact(self):
        self.refuse(lambda r: r['stages'][10]['reply']['journal']['drafts'][0]['blocks'][0]['elements'][0]['elements'][0].pop('style'))
        self.refuse(lambda r: r['stages'][10]['reply']['journal']['drafts'][0]['blocks'][1].pop('opaque'))
        for key, value in [('id', 'other'), ('capture', 'other'), ('revision', 9), ('text', 'flattened')]:
            self.refuse(lambda r: r['stages'][10]['reply']['journal']['drafts'][0].update({key: value}))
        self.refuse(lambda r: r['stages'][10]['reply']['journal'].update(attempt={'sent': True}))

    def test_actual_viewer_and_fresh_reopened_editor(self):
        self.refuse(lambda r: r['stages'][9]['layout']['viewer'].update(ready=False))
        self.refuse(lambda r: r['stages'][9]['layout']['viewer'].update(title=O.NAME))
        self.refuse(lambda r: r['stages'][9]['layout']['viewer']['close'].update(hit=False))
        self.refuse(lambda r: r['stages'][10]['layout']['native'].update(sameNode=True))
        self.refuse(lambda r: r['stages'][9]['proof']['resolver']['responses'][0]['file'].update(id='FCANON002'))

    def test_painted_name_not_title_and_no_discarded_media(self):
        self.refuse(lambda r: r['stages'][8]['layout']['link'].update(name=O.positive()['file']['title']))
        self.refuse(lambda r: r['stages'][8]['layout'].update(body='EMBEDDED'))
        self.refuse(lambda r: r['stages'][8]['layout']['target'].update(hit=False))
        self.refuse(lambda r: r['stages'][4]['layout']['target'].update(rect=rect(y=900)))
        self.refuse(lambda r: r['stages'][8]['proof']['media'][0].update(registered=1))
        self.refuse(lambda r: r['stages'][8]['layout']['images'].append('discarded'))

    def test_no_network_extra_stages_or_terminal_substitution(self):
        self.refuse(lambda r: r['stages'][8]['acquisition'].update(httpCapability=True))
        self.refuse(lambda r: r['stages'][8]['acquisition'].update(networkCalls=1))
        self.refuse(lambda r: r['stages'][8].update(noSession=False))
        self.refuse(lambda r: r['stages'].pop())
        self.refuse(lambda r: r.update(acquisition={}))

    def test_runner_flags_scope_and_counts(self):
        path = Path(__file__).with_name('check-canvas-cold-file-roots.py')
        spec = importlib.util.spec_from_file_location('cold_runner_test', path)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        self.assertEqual(mod.SPEC.stages, O.STAGES)
        self.assertEqual(mod.SPEC.counts, {'requestCount': 7, 'acquisitionCount': 7, 'resolverCount': 1})
        with tempfile.TemporaryDirectory() as directory:
            flags = mod.private_checks(mod.SPEC, Path(directory))
            self.assertEqual(flags['SUPER_PLATINUM_CANVAS_COLD_FILE_CHECK'], '1')
            self.assertEqual(flags['SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK'], '1')
            with self.assertRaises(RuntimeError):
                mod.private_checks(copy.copy(mod.SPEC), Path(directory))
        r = sample()
        r.update(ok=True, noSession=True, noTransport=True, draftPreserved=True,
                 freshTarget=True, staleRefused=True, requestCount=7, acquisitionCount=7, resolverCount=1)
        mod.validate(mod.SPEC, O.FIXTURES[1], r)
        with self.assertRaises(RuntimeError):
            mod.validate(mod.SPEC, 'canvas-canonical-native-list-narrow', r)


if __name__ == '__main__':
    unittest.main()
