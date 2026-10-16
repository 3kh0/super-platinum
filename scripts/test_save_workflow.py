"""Pure negative oracles and loopback bytes; never launches native/UI helpers."""
import copy
import hashlib
import http.client
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, Mock
from save_workflow_contracts import (ALIAS, STAGES, CAPTURES, RETURNS, OUTCOMES, PRESERVED,
    owner, native_owner, validate_stage, validate_native_logs, validate_probe,
    validate_review, DestinationAudit, require, digest, save, identical, read_json)
from save_workflow_helpers import validate_actions, Helpers
from save_workflow_server import WorkflowOriginal
from original_download_native import BYTES, CHUNK, expected_digest

from test_toast_geometry import sample_toast

EXPECTED = owner('a' * 32, 123, 456)


def stage(index):
    name = STAGES[index]
    return dict(ok=True, stage=name, owner=native_owner(EXPECTED),
                dom=dict(toast=sample_toast(PRESERVED if name == 'preserved' else 'Saved original-fixture.bin') if name in ('preserved','saved') else None,ok=True, stage=name, label='Cancel' if name == 'busy' else 'Download', width=96, viewport=[1280, 780]),
                issued=(0, 1, 2, 3, 3)[index], returns=copy.deepcopy(RETURNS[:(0, 1, 2, 3, 3)[index]]),
                outcomes=copy.deepcopy(OUTCOMES[:(0, 0, 1, 1, 2)[index]]),
                noSession=True, noTransport=True, noAccount=True,
                toast=(None, None, PRESERVED, None, 'Saved original-fixture.bin')[index])


def native_logs():
    stages = [stage(i) for i in range(5)]
    common = dict(version=1, nonce=EXPECTED['nonce'], pid=EXPECTED['pid'])
    return dict(waiting=[native_owner(EXPECTED)],
                requested=[dict(common, attempt=i, actualRfd=True) for i in (1, 2, 3)],
                returned=[dict(common, actualRfd=True, transferAllowed=i != 1, **RETURNS[i-1]) for i in (1, 2, 3)],
                outcome=[dict(common, **row) for row in OUTCOMES], stage=stages,
                check=[dict(ok=True, owner=native_owner(EXPECTED), stages=stages, actualRfd=True,
                            noSession=True, noTransport=True, noAccount=True)])


def control(index=1, edit=False):
    return dict(runtimeId=[42, index], processId=123, nativeHwnd=0, automationId=str(index),
                name='File name:' if edit else 'Save', controlType='ControlType.Edit' if edit else 'ControlType.Button',
                className='Edit' if edit else 'Button', enabled=True, offscreen=False,
                invoke=not edit, value=edit, readOnly=False if edit else None, rect=[1, 2, 3, 4])


def observed(action='save-new', kind='save'):
    probe = dict(EXPECTED, ok=True, mode='probe', kind=kind,
                 window=dict(hwnd=77, title='Save original file - choose a new filename' if kind == 'save' else 'Confirm Save As',
                             className='#32770', rect=[0, 0, 400, 300], ownerChain=[] if kind == 'save' else [66]),
                 controls=[control(), control(2, edit=True)])
    value = dict(version=1, stage=action, probe=probe, image='image.png', imageSha256='b'*64,
                 allowedPaths=['preserve.bin', 'saved.bin'], saveHwnd=66, _sha256='c'*64)
    if not action.startswith('set-'):
        probe['controls'][0]['name'] = {'cancel': 'Cancel', 'decline': 'No', 'accept': 'Yes'}.get(action, 'Save')
    request = dict(EXPECTED, kind=kind, mode='set-path' if action.startswith('set-') else 'invoke', hwnd=77,
                   window=probe['window'], target=probe['controls'][1 if action.startswith('set-') else 0])
    if not action.startswith('set-'):
        request['role'] = 'save' if action.startswith('save-') else action
    if action.startswith('save-') or action.startswith('set-'):
        request['allowedPath'] = value['allowedPaths'][1 if action.endswith('new') else 0]
    if action.startswith('save-'):
        request['filenameTarget'] = probe['controls'][1]
    if kind == 'overwrite':
        request['saveHwnd'] = 66
    review = dict(version=1, stage=action, observedSha256='c'*64, imageSha256='b'*64, request=request)
    return value, review


class Contracts(unittest.TestCase):
    def test_toast_text_without_visible_dom_proof_refuses(self):
        for index in (2,4):
            for value in (None,dict(stage(index)['dom']['toast'],visible=False)):
                row=stage(index);row['dom']['toast']=value
                with self.assertRaises(RuntimeError):validate_stage(row,index,EXPECTED)

    def test_exact_native_ledger(self):
        data = native_logs()
        validate_native_logs(data, EXPECTED, data['stage'])
        self.assertEqual(len(CAPTURES), 11)
        for key in ('requested', 'returned', 'outcome', 'stage', 'check'):
            bad = copy.deepcopy(data)
            bad[key].append(copy.deepcopy(bad[key][-1]))
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                validate_native_logs(bad, EXPECTED, data['stage'])

    def test_none_is_accepted_but_never_transferred(self):
        data = native_logs()
        for key, value in [('transferAllowed', True), ('accepted', False), ('kind', 'new'), ('actualRfd', False)]:
            bad = copy.deepcopy(data)
            bad['returned'][0][key] = value
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                validate_native_logs(bad, EXPECTED, data['stage'])

    def test_wrong_owner_bool_pid_and_missing_disconnect_fail(self):
        for mutation in (lambda r: r['owner'].update(pid=True), lambda r: r.update(noAccount=False),
                         lambda r: r.update(issued=True), lambda r: r.update(toast='Saved unexpected')):
            row = stage(1)
            mutation(row)
            with self.assertRaises(RuntimeError):
                validate_stage(row, 1, EXPECTED)

    def test_stage_dom_width_viewport_and_outcomes(self):
        for mutation in (lambda r: r['dom'].update(width=95), lambda r: r['dom'].update(width=float('nan')),
                         lambda r: r['dom'].update(label='Download'), lambda r: r['dom'].update(viewport=[780, 1280]),
                         lambda r: r['outcomes'].append(dict(attempt=3, kind='saved'))):
            row = stage(3)
            mutation(row)
            with self.assertRaises(RuntimeError):
                validate_stage(row, 3, EXPECTED, 96)

    def test_all_reviews_bind_exact_control_and_kind(self):
        for action in ('cancel', 'set-existing', 'save-existing-1', 'decline', 'save-existing-2', 'accept', 'set-new', 'save-new'):
            value, review = observed(action, 'overwrite' if action in ('decline', 'accept') else 'save')
            validate_probe(value['probe'], EXPECTED, value['probe']['kind'], 77, 66)
            validate_review(review, value)

    def test_stale_review_control_window_hash_path_and_extra_fields_fail(self):
        mutations = [lambda r: r.update(stage='save-existing-2'), lambda r: r.update(imageSha256='d'*64),
                     lambda r: r.update(observedSha256='d'*64), lambda r: r['request'].update(pid=124),
                     lambda r: r['request'].update(hwmd=77), lambda r: r['request'].update(allowedPath='elsewhere'),
                     lambda r: r['request']['target'].update(runtimeId=[42, 99]),
                     lambda r: r['request']['filenameTarget'].update(offscreen=True),
                     lambda r: r['request']['window'].update(hwnd=99)]
        for mutation in mutations:
            value, review = observed()
            review = copy.deepcopy(review)
            mutation(review)
            with self.assertRaises(RuntimeError):
                validate_review(review, value)

    def test_overwrite_requires_measured_save_relationship(self):
        value, _ = observed('decline', 'overwrite')
        for chain in ([], [77], [77, 55], [66, 77], [True, 66], [66, 66]):
            probe = copy.deepcopy(value['probe'])
            probe['window']['ownerChain'] = chain
            with self.assertRaises(RuntimeError):
                validate_probe(probe, EXPECTED, 'overwrite', 77, 66)

    def test_ambiguous_and_disabled_controls_fail(self):
        value, review = observed()
        value['probe']['controls'].append(copy.deepcopy(value['probe']['controls'][0]))
        with self.assertRaises(RuntimeError):
            validate_review(review, value)
        with self.assertRaises(RuntimeError):
            validate_probe(value['probe'], EXPECTED, 'save', 77)
        value, review = observed()
        review['request']['target']['enabled'] = False
        with self.assertRaises(RuntimeError):
            validate_review(review, value)

    def test_action_missing_after_or_replayed_marker_fails(self):
        _, review = observed('cancel')
        request = review['request']
        common = {key: request[key] for key in ('nonce', 'pid', 'creation', 'hwnd')}
        rows = [dict(common, kind='before-invoke', dialogKind='save', role='cancel', target=request['target'], count=1),
                dict(common, kind='invoke-returned', dialogKind='save', role='cancel', count=1)]
        validate_actions(rows, request)
        for bad in (rows[:1], rows + rows, list(reversed(rows))):
            with self.assertRaises(RuntimeError):
                validate_actions(bad, request)

    def test_set_requires_exact_readback(self):
        _, review = observed('set-new')
        request = review['request']
        common = {key: request[key] for key in ('nonce', 'pid', 'creation', 'hwnd')}
        rows = [dict(common, kind='before-set', target=request['target'], allowedPath='saved.bin', count=1),
                dict(common, kind='after-set', allowedPath='saved.bin', valueEqualsAllowedPath=True, count=1)]
        validate_actions(rows, request)
        rows[1]['valueEqualsAllowedPath'] = False
        with self.assertRaises(RuntimeError):
            validate_actions(rows, request)

    def test_duplicate_json_key_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'review.json'
            path.write_text('{"pid":1,"pid":2}', encoding='utf-8')
            with self.assertRaises(RuntimeError):
                read_json(path)


class Files(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name)
        (self.path / 'downloads').mkdir()
        self.audit = DestinationAudit(self.path, 'a' * 32)

    def test_empty_first_and_exclusive_sentinel(self):
        self.assertIsNone(self.audit.sentinel)
        pin = self.audit.create_sentinel()
        self.assertEqual(self.audit.snapshot('ready')['files'], [pin])
        with self.assertRaises(RuntimeError):
            self.audit.create_sentinel()

    def test_changed_sentinel_bytes_and_identity_fail(self):
        self.audit.create_sentinel()
        self.audit.paths[0].write_bytes(b'changed')
        with self.assertRaises(RuntimeError):
            self.audit.snapshot('preserved')

    def test_extra_file_parts_and_wrong_saved_body_fail(self):
        self.audit.create_sentinel()
        extra = self.path / 'downloads/unexpected.bin'
        extra.write_bytes(b'x')
        with self.assertRaises(RuntimeError):
            self.audit.snapshot('canceled')
        extra.unlink()
        part = self.path / 'downloads/.super-platinum-download-00000000-0000-0000-0000-000000000000.part'
        part.write_bytes(CHUNK)
        self.audit.snapshot('busy', busy=True)
        with self.assertRaises(RuntimeError):
            self.audit.snapshot('preserved')
        part.unlink()
        self.audit.paths[1].write_bytes(b'wrong')
        with self.assertRaises(RuntimeError):
            self.audit.snapshot('saved', saved=True)

    def test_directory_identity_changed_fails(self):
        self.audit.create_sentinel()
        with patch('save_workflow_contracts.identity', return_value=[-1, -1]):
            with self.assertRaises(RuntimeError):
                self.audit.snapshot('canceled')

    def test_busy_waits_only_for_missing_or_short_part(self):
        self.audit.create_sentinel()
        self.assertIsNone(self.audit.snapshot('busy', busy=True, pending_ok=True))
        part = self.path / 'downloads/.super-platinum-download-00000000-0000-0000-0000-000000000000.part'
        part.write_bytes(b'first short chunk')
        self.assertIsNone(self.audit.snapshot('busy', busy=True, pending_ok=True))
        part.write_bytes(CHUNK)
        self.assertIsNotNone(self.audit.snapshot('busy', busy=True, pending_ok=True))
        (self.path / 'downloads/unexpected.bin').write_bytes(b'bad')
        with self.assertRaises(RuntimeError):
            self.audit.snapshot('busy', busy=True, pending_ok=True)

    def test_busy_pending_never_masks_sentinel_corruption(self):
        self.audit.create_sentinel()
        self.audit.paths[0].write_bytes(b'corrupt')
        with self.assertRaises(RuntimeError):
            self.audit.snapshot('busy', busy=True, pending_ok=True)

    def test_raw_closed_inventory_preserves_unexpected_file_evidence(self):
        self.audit.create_sentinel()
        path = self.audit.paths[1]
        path.write_bytes(b'incomplete saved data')
        raw = self.audit.raw_snapshot('closed')
        self.assertEqual(raw['errors'], [])
        row = next(row for row in raw['files'] if row['name'] == path.name)
        self.assertEqual(row['sha256'], hashlib.sha256(b'incomplete saved data').hexdigest())
        with self.assertRaises(RuntimeError):
            self.audit.snapshot('closed')

    def test_raw_closed_inventory_bounds_entries_and_unsafe_ancestor(self):
        self.audit.create_sentinel()
        for index in range(5):
            (self.audit.downloads / f'extra-{index}').write_bytes(b'x')
        raw = self.audit.raw_snapshot('closed')
        self.assertEqual(len(raw['files']), 4)
        self.assertTrue(raw['errors'])
        with patch('save_workflow_contracts.identity', return_value=[-1, -1]):
            raw = self.audit.raw_snapshot('closed')
        self.assertEqual(raw['files'], [])
        self.assertTrue(raw['errors'])


class Server(unittest.TestCase):
    def test_one_full_body_held_then_released_and_closed(self):
        server = WorkflowOriginal('a' * 32)
        try:
            connection = http.client.HTTPConnection('127.0.0.1', server.port, timeout=10)
            connection.request('GET', '/original/' + 'a'*32, headers={'Accept-Encoding': 'identity'})
            response = connection.getresponse()
            self.assertEqual(response.status, 200)
            checksum = hashlib.sha256()
            first = response.read(len(CHUNK))
            self.assertEqual(first, CHUNK)
            checksum.update(first)
            server.release()
            count = len(first)
            while block := response.read(65536):
                count += len(block)
                checksum.update(block)
            connection.close()
            self.assertEqual(count, BYTES)
            self.assertEqual(checksum.hexdigest(), expected_digest())
        finally:
            server.close()
        server.validate()
        self.assertTrue(all(not thread.is_alive() for thread in server.handlers))

    def test_request_oracle_rejects_auth_extra_truncation_method(self):
        server = WorkflowOriginal('a' * 32)
        server.close()
        good = dict(index=1, method='GET', path='/original/' + 'a'*32, cookieAbsent=True,
                    authorizationAbsent=True, encoding='identity', bytesSent=BYTES, closed=True)
        server.rows = [good]
        server.connections = 1
        server.validate()
        for patch_value in ({'cookieAbsent': False}, {'authorizationAbsent': False}, {'bytesSent': 65536},
                            {'method': 'POST'}, {'closed': False}, {'error': 'timeout'}):
            server.rows = [dict(good, **patch_value)]
            with self.assertRaises(RuntimeError):
                server.validate()
        server.rows = [good, good]
        with self.assertRaises(RuntimeError):
            server.validate()
        server.rows = [good]
        server.connections = 2
        with self.assertRaises(RuntimeError):
            server.validate()


class HelperLifecycle(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.api = Mock()
        self.api.open.return_value = 42
        self.api.times.return_value = dict(creation_filetime=456, exit_filetime=500)
        self.api.kernel.CloseHandle.return_value = True
        self.evidence = dict(helpers=[])
        self.helpers = Helpers(self.directory, self.api, lambda: None, self.evidence)
        self.addCleanup(self.helpers.close)

    def fake_process(self, pid=100, exited=False):
        process = Mock(pid=pid)
        process.poll.return_value = 0 if exited else None
        process.terminate.side_effect = lambda: setattr(process.poll, 'return_value', 1)
        return process

    def test_at_most_two_helpers_and_no_replay(self):
        processes = [self.fake_process(100), self.fake_process(101)]
        request = dict(EXPECTED, mode='probe', kind='save', hwnd=77)
        with patch('save_workflow_helpers.subprocess.Popen', side_effect=processes) as launch:
            first = self.helpers.start(request, 'first')
            self.helpers.start(request, 'second')
            with self.assertRaises(RuntimeError):
                self.helpers.start(request, 'third')
            self.helpers.stop(first)
            with self.assertRaises(RuntimeError):
                self.helpers.start(request, 'first')
            self.assertEqual(launch.call_count, 2)
        self.helpers.close()
        self.assertTrue(all(row['exited'] and row['handleClosed'] for row in self.evidence['helpers']))

    def test_wrong_closed_creation_and_missing_exit_fail(self):
        request = dict(EXPECTED, mode='probe', kind='save', hwnd=77)
        with patch('save_workflow_helpers.subprocess.Popen', return_value=self.fake_process()):
            job = self.helpers.start(request, 'identity')
        self.api.times.return_value = dict(creation_filetime=457, exit_filetime=500)
        with self.assertRaises(RuntimeError):
            self.helpers.stop(job)
        self.assertTrue(job['output'].closed)
        with patch('save_workflow_helpers.subprocess.Popen', return_value=self.fake_process(101)):
            job = self.helpers.start(request, 'exit')
        self.api.times.return_value = dict(creation_filetime=457, exit_filetime=0)
        with self.assertRaises(RuntimeError):
            self.helpers.stop(job)

    def test_handle_cleanup_failure_still_closes_output(self):
        request = dict(EXPECTED, mode='probe', kind='save', hwnd=77)
        with patch('save_workflow_helpers.subprocess.Popen', return_value=self.fake_process()):
            job = self.helpers.start(request, 'handle')
        self.api.kernel.CloseHandle.return_value = False
        with self.assertRaises(RuntimeError):
            self.helpers.stop(job)
        self.assertTrue(job['output'].closed)
        self.assertFalse(job['record']['handleClosed'])

    def test_missing_action_return_remains_failure(self):
        _, review = observed('cancel')
        request = review['request']
        with patch('save_workflow_helpers.subprocess.Popen', return_value=self.fake_process(exited=True)):
            job = self.helpers.start(request, 'partial')
        common = {key: request[key] for key in ('nonce', 'pid', 'creation', 'hwnd')}
        job['output'].write('save-workflow-provider: {"ok":true}\n' + json.dumps(dict(common, kind='before-invoke', dialogKind='save', role='cancel', target=request['target'], count=1)) + '\n')
        job['output'].flush()
        with self.assertRaises(RuntimeError):
            self.helpers.finish(job)
        self.assertTrue(job['finished'])


if __name__ == '__main__':
    unittest.main()
