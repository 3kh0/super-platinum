#!/usr/bin/env python3
"""Actual rfd Cancel / private overwrite decline+accept / full new-file save.

Every native mutation waits for a root-published exact observed-control review.
Never builds, chooses a selector, replays an action, or uses a real Slack session.
"""
import argparse
import json
import os
from pathlib import Path
import secrets
import sys
import time
from native_fixture import ROOT, NativeFixture, DEFAULT_BINARY
from save_workflow_contracts import (ALIAS, STAGES, CAPTURES, ACTIONS, DestinationAudit,
    owner, native_owner, require, identical, read_json, digest, save, validate_probe,
    validate_review, validate_stage, validate_native_logs, unlinked)
from save_workflow_server import WorkflowOriginal
from save_workflow_capture import Capture
from save_workflow_helpers import Helpers


def records(log, kind):
    path = Path(log)
    if not path.exists():
        return []
    require(path.stat().st_size <= 4 * 1024 * 1024, 'bounded native log')
    prefix = f'workflow-{kind}: '
    return [json.loads(line.split(prefix, 1)[1]) for line in path.read_text(encoding='utf-8').splitlines(keepends=True)
            if prefix in line and line.endswith('\n')]


def logs(native):
    return {kind: records(native.log, kind) for kind in ('waiting', 'requested', 'returned', 'outcome', 'stage', 'check')}


def wait_record(native, kind, index, verify, seconds=30):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        verify()
        rows = records(native.log, kind)
        require(len(rows) <= index + 1, 'unexpected duplicate/future native record: ' + kind)
        if len(rows) == index + 1:
            return rows[index]
        terminal = records(native.log, 'check')
        require(not terminal, 'native terminated before expected stage')
        time.sleep(.05)
    raise RuntimeError('native stage deadline: ' + kind)


def ack(directory, expected, stage):
    require(stage in ('ready', *STAGES), 'allowed ACK')
    temporary = directory / f'ack-{stage}.staged'
    save(temporary, dict(native_owner(expected), stage=stage))
    try:
        os.link(temporary, directory / f'ack-{stage}.json')
    finally:
        temporary.unlink()


class Workflow:
    def __init__(self, native, directory, expected, audit, server, capture, helpers, verify, evidence, review_seconds):
        self.native, self.directory, self.expected = native, directory, expected
        self.audit, self.server, self.capture, self.helpers = audit, server, capture, helpers
        self.verify, self.evidence, self.review_seconds = verify, evidence, review_seconds
        self.app_hwnd = None
        self.stages = []
        self.width = None
        self.reviewed = []
        self.before_click = []
        self.before_save = []

    def advance(self, name):
        self.before_click = self.capture.visible_windows('before-app-click-' + name)
        ack(self.directory, self.expected, name)

    def files(self, phase, *, saved=False, busy=False):
        value = self.audit.snapshot(phase, saved=saved, busy=busy)
        self.evidence['files'].append(value)
        if phase not in ('busy', 'saved', 'closed'):
            require(self.server.rows == [] and self.server.connections == 0, 'network before final accepted new destination')
        return value

    def stage(self, name):
        index = STAGES.index(name)
        require(len(self.stages) == index, 'stage consumed once/in order')
        value = wait_record(self.native, 'stage', index, self.verify)
        self.width = validate_stage(value, index, self.expected, self.width)
        self.stages.append(value)
        if name == 'busy':
            deadline = time.monotonic() + 10
            while True:
                self.verify()
                files = self.audit.snapshot('busy', busy=True, pending_ok=True)
                rows = self.server.rows
                require(len(rows) <= 1 and self.server.connections <= 1, 'unexpected extra HTTP connection/request while settling busy')
                ready = False
                if rows:
                    row = rows[0]
                    require(row.get('method') == 'GET' and row.get('path') == '/original/' + self.expected['nonce']
                            and row.get('cookieAbsent') is True and row.get('authorizationAbsent') is True
                            and row.get('encoding') == 'identity' and not row.get('error')
                            and row.get('closed') is False and type(row.get('bytesSent')) is int
                            and 0 <= row['bytesSent'] <= 65536, 'exact held original request')
                    ready = row['bytesSent'] == 65536
                if ready and files is not None:
                    self.evidence['files'].append(files)
                    break
                require(time.monotonic() < deadline, 'busy staging/first chunk deadline')
                time.sleep(.05)
        else:
            self.files(name, saved=name == 'saved')
        self.capture.capture(name, self.app_hwnd)
        return value

    def requested(self, attempt):
        value = wait_record(self.native, 'requested', attempt - 1, self.verify)
        require(identical(value, dict(version=1, nonce=self.expected['nonce'], pid=self.expected['pid'], attempt=attempt, actualRfd=True)), 'exact actual-rfd request')
        window = self.capture.wait_other(self.app_hwnd)
        current = self.capture.visible_windows('requested-' + str(attempt))
        require(set(current) - set(self.before_click) == {window['hwnd']}, 'one newly visible same-process Save HWND')
        return window['hwnd']

    def review(self, action, hwnd, *, kind='save', save_hwnd=None, captured=None):
        require(ACTIONS[len(self.reviewed)] == action, 'finite review action order')
        self.capture.observe(hwnd)
        request = dict(self.expected, mode='probe', kind=kind, hwnd=hwnd)
        if kind == 'overwrite':
            request['saveHwnd'] = save_hwnd
        rows = self.helpers.call(request, action + '-probe')
        require(len(rows) == 1, 'one bounded control inventory')
        probe = rows[0]
        validate_probe(probe, self.expected, kind, hwnd, save_hwnd)
        picture = captured or self.capture.capture('review-' + action, hwnd, final=False)
        window = self.capture.observe(hwnd)
        require(identical(probe['window']['rect'], window['window_rect_physical']) and picture['rect'] == probe['window']['rect'], 'review inventory/capture geometry agrees')
        observed = dict(version=1, stage=action, probe=probe, image=picture['path'], imageSha256=picture['sha256'],
                        allowedPaths=[str(p) for p in self.audit.paths], saveHwnd=save_hwnd)
        path = self.directory / f'{action}-observed.json'
        save(path, observed)
        observed['_sha256'] = digest(path)
        review_path = self.directory / f'{action}-review.json'
        print(json.dumps(dict(stage='awaiting-root-review', action=action, observed=str(path), image=picture['path'], reviewFile=str(review_path))), flush=True)
        before = {key: records(self.native.log, key) for key in ('requested', 'returned', 'outcome', 'stage', 'check')}
        deadline = time.monotonic() + self.review_seconds
        while not review_path.exists():
            self.capture.observe(hwnd, stage='await-review-' + action)
            require(time.monotonic() < deadline, 'root review expired; no automatic action')
            require(all(identical(records(self.native.log, key), value) for key, value in before.items()), 'native changed during review')
            time.sleep(.25)
        require(all(identical(records(self.native.log, key), value) for key, value in before.items()), 'native changed before reviewed action')
        review = read_json(review_path)
        request = validate_review(review, observed)
        self.capture.observe(hwnd)
        self.files('before-' + action)
        self.reviewed.append(action)
        self.evidence['reviews'].append(dict(stage=action, path=str(review_path), sha256=digest(review_path), observedSha256=observed['_sha256']))
        return request

    def action(self, action, hwnd, **options):
        request = self.review(action, hwnd, **options)
        if action.startswith('save-'):
            self.before_save = self.capture.visible_windows('before-' + action)
        job = self.helpers.start(request, action)
        self.helpers.before(job)
        return job

    def execute(self):
        waiting = wait_record(self.native, 'waiting', 0, self.verify)
        require(identical(waiting, native_owner(self.expected)), 'exact native waiting before sentinel')
        self.evidence['sentinel'] = self.audit.create_sentinel()
        self.files('ready')
        self.native.focus()
        self.app_hwnd = self.capture.observe()['hwnd']
        ack(self.directory, self.expected, 'ready')
        self.stage('initial')
        self.advance('initial')
        first = self.requested(1)
        picture = self.capture.capture('cancel-dialog', first)
        self.helpers.finish(self.action('cancel', first, captured=picture))
        self.stage('canceled')
        self.advance('canceled')
        second = self.requested(2)
        self.helpers.finish(self.action('set-existing', second))
        picture = self.capture.capture('existing-dialog', second)
        save_job = self.action('save-existing-1', second, captured=picture)
        prompt = self.capture.wait_other(second)['hwnd']
        require(prompt != self.app_hwnd, 'expected overwrite popup, not app')
        require(set(self.capture.visible_windows('overwrite-decline')) - set(self.before_save) == {prompt}, 'one new overwrite window')
        picture = self.capture.capture('overwrite-decline', prompt)
        self.helpers.finish(self.action('decline', prompt, kind='overwrite', save_hwnd=second, captured=picture))
        self.helpers.finish(save_job)
        self.capture.observe(second)
        require(len(records(self.native.log, 'returned')) == 1 and records(self.native.log, 'outcome') == [], 'decline retains dialog without rfd return/transfer')
        self.files('after-decline')
        picture = self.capture.capture('after-decline', second)
        save_job = self.action('save-existing-2', second, captured=picture)
        prompt = self.capture.wait_other(second)['hwnd']
        require(prompt != self.app_hwnd, 'expected second overwrite popup')
        require(set(self.capture.visible_windows('overwrite-accept')) - set(self.before_save) == {prompt}, 'one new second overwrite window')
        picture = self.capture.capture('overwrite-accept', prompt)
        self.helpers.finish(self.action('accept', prompt, kind='overwrite', save_hwnd=second, captured=picture))
        self.helpers.finish(save_job)
        self.stage('preserved')
        self.advance('preserved')
        third = self.requested(3)
        self.helpers.finish(self.action('set-new', third))
        picture = self.capture.capture('new-dialog', third)
        self.helpers.finish(self.action('save-new', third, captured=picture))
        self.stage('busy')
        ack(self.directory, self.expected, 'busy')
        self.server.release()
        self.stage('saved')
        ack(self.directory, self.expected, 'saved')
        wait_record(self.native, 'check', 0, self.verify)
        validate_native_logs(logs(self.native), self.expected, self.stages)
        require(tuple(self.reviewed) == ACTIONS, 'exact eight reviewed native actions')
        require(tuple(row['stage'] for row in self.evidence['captures']) == CAPTURES, 'eleven exact final captures')
        self.evidence['nativeLogs'] = logs(self.native)


def run(binary, review_seconds):
    require(sys.platform == 'win32', 'Windows only')
    require(15 <= review_seconds <= 120, 'bounded review window')
    from windows_process_metrics import WindowsAPI
    from visual_pointer import WindowsPointerIO
    nonce = secrets.token_hex(16)
    directory = (ROOT / 'tmp' / f'original-save-workflow-{nonce}').resolve()
    require(directory.parent == (ROOT / 'tmp').resolve(), 'private tmp parent')
    directory.mkdir(exist_ok=False)
    (directory / 'downloads').mkdir()
    audit = DestinationAudit(directory, nonce)
    evidence = dict(ok=False, directory=str(directory), nonce=nonce, binary=str(binary), binarySha256=digest(binary),
                    pointerMoveAttempted=False, captures=[], reviewCaptures=[], captureSamples=[], observations=[],
                    helpers=[], reviews=[], files=[], windows=[], limitations=[
                        'Actual Windows rfd and UIA synthetic actions; not trusted physical input or human overwrite consent.',
                        'Disconnected fixture and one loopback original; no Slack authentication or cached overwrite coverage.',
                        'Sampled ownership and pixel stability; no continuous occlusion proof.'])
    api, pointer = WindowsAPI(), WindowsPointerIO()
    pointer_start = pointer.position()
    evidence['pointerOriginal'] = pointer_start
    handle = native = server = expected = helpers = workflow = None
    primary = None
    started = time.monotonic()
    try:
        server = WorkflowOriginal(nonce)
        env = {'SUPER_PLATINUM_ORIGINAL_DOWNLOAD_CHECK': '1', 'SUPER_PLATINUM_SAVE_WORKFLOW_CHECK': '1',
               'SUPER_PLATINUM_ORIGINAL_DOWNLOAD_DIR': str(directory), 'SUPER_PLATINUM_ORIGINAL_DOWNLOAD_NONCE': nonce,
               'SUPER_PLATINUM_ORIGINAL_DOWNLOAD_PORT': str(server.port)}
        native = NativeFixture(ALIAS, directory / 'native.log', binary, env)
        with native:
            handle = api.open(native.process.pid)
            expected = owner(nonce, native.process.pid, api.times(handle)['creation_filetime'])
            evidence['owner'] = expected
            def verify():
                native.require_running()
                times = api.times(handle)
                require(times['creation_filetime'] == expected['creation'] and times['exit_filetime'] == 0, 'owned native process changed')
                require(pointer.position() == pointer_start, 'pointer moved during workflow')
                require(time.monotonic() - started < 1800, 'finite workflow total deadline')
            capture = Capture(directory, expected, evidence, verify)
            helpers = Helpers(directory, api, verify, evidence)
            workflow = Workflow(native, directory, expected, audit, server, capture, helpers, verify, evidence, review_seconds)
            try:
                workflow.execute()
            finally:
                helpers.close()
        validate_native_logs(logs(native), expected, workflow.stages)
        evidence['closedLogValidated'] = True
    except BaseException as error:
        primary = error
        evidence['error'] = f'{type(error).__name__}: {error}'
    finally:
        errors = []
        def checked(label, callback):
            try:
                callback()
            except BaseException as error:
                errors.append(dict(stage=label, error=f'{type(error).__name__}: {error}'))
        if helpers is not None:
            checked('helpers-close', helpers.close)
        if server is not None:
            checked('server-close', server.close)
            evidence['requests'] = list(server.rows)
            evidence['connections'] = server.connections
            if primary is None:
                checked('one-full-GET', server.validate)
        if handle is not None:
            def closed_identity():
                value = api.times(handle)
                evidence['exitIdentity'] = value
                require(value['creation_filetime'] == expected['creation'] and value['exit_filetime'] >= expected['creation'], 'native closed identity')
            checked('native-exit', closed_identity)
            checked('native-handle-close', lambda: require(api.kernel.CloseHandle(handle), 'native handle close'))
        evidence['processExited'] = native is not None and native.process is not None and native.process.poll() is not None
        evidence['exitCode'] = None if native is None or native.process is None else native.process.poll()
        evidence['cleanupErrors'] = [] if native is None else native.cleanup_errors
        checked('process-exit', lambda: require(evidence['processExited'], 'native process exit missing'))
        checked('cleanup', lambda: require(not evidence['cleanupErrors'], 'native cleanup failed'))
        def closed_files():
            evidence['closedInventory'] = audit.raw_snapshot('closed')
            require(not evidence['closedInventory']['errors'], 'unsafe or incomplete raw closed inventory')
            value = audit.snapshot('closed', saved=primary is None)
            evidence['files'].append(value)
            if primary is None:
                require(value['files'] == next(row['files'] for row in evidence['files'] if row['phase'] == 'saved'), 'saved bytes/identities changed after exit')
        if audit.sentinel is not None:
            checked('closed-files', closed_files)
        def same_binary():
            evidence['binarySha256After'] = digest(binary)
            require(evidence['binarySha256After'] == evidence['binarySha256'], 'binary changed')
        def same_pointer():
            evidence['pointerFinal'] = pointer.position()
            require(tuple(evidence['pointerFinal']) == tuple(pointer_start), 'pointer moved')
        checked('binary', same_binary)
        checked('pointer', same_pointer)
        evidence['auditErrors'] = errors
        evidence['ok'] = primary is None and not errors
        save(directory / 'receipt.json', evidence)
        print(json.dumps(dict(receipt=str(directory / 'receipt.json'), ok=evidence['ok'])), flush=True)
    if primary is not None:
        raise primary
    require(evidence['ok'], 'final workflow audit failed')
    return evidence


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=DEFAULT_BINARY)
    parser.add_argument('--review-seconds', type=int, default=120)
    args = parser.parse_args()
    run(args.binary.resolve(), args.review_seconds)
