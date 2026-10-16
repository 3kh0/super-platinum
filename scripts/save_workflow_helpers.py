"""Owned asynchronous UIA helpers. At most two; never replay an action."""
import json
import os
import subprocess
import time
from pathlib import Path
from native_fixture import ROOT
from save_workflow_contracts import require, save, identical, digest


def read_lines(path):
    require(path.stat().st_size <= 512 * 1024, 'bounded helper output')
    return [line for line in path.read_text(encoding='utf-8-sig').splitlines(keepends=True) if line.endswith('\n')]


def validate_actions(rows, request):
    common = {key: request[key] for key in ('nonce', 'pid', 'creation', 'hwnd')}
    if request['mode'] == 'set-path':
        expected = [dict(common, kind='before-set', target=request['target'], allowedPath=request['allowedPath'], count=1),
                    dict(common, kind='after-set', allowedPath=request['allowedPath'], valueEqualsAllowedPath=True, count=1)]
    else:
        expected = [dict(common, kind='before-invoke', dialogKind=request['kind'], role=request['role'], target=request['target'], count=1),
                    dict(common, kind='invoke-returned', dialogKind=request['kind'], role=request['role'], count=1)]
    require(identical(rows, expected), 'exact one-action helper record sequence')


class Helpers:
    def __init__(self, directory, api, verify, evidence):
        self.directory, self.api, self.verify, self.evidence = directory, api, verify, evidence
        self.jobs = []

    def start(self, request, label):
        self.verify()
        require(sum(not job['finished'] for job in self.jobs) < 2, 'at most two owned helpers')
        require(not any(job['label'] == label for job in self.jobs), 'helper action never replayed')
        path = self.directory / f'uia-{label}.log'
        env = os.environ.copy()
        env['SP_SAVE_WORKFLOW_JSON'] = json.dumps(request, separators=(',', ':'))
        script = Path(__file__).with_name('windows_save_workflow.ps1')
        command = [str(Path(os.environ['SystemRoot']) / 'System32/WindowsPowerShell/v1.0/powershell.exe'),
                   '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', str(script)]
        output = path.open('x', encoding='utf-8')
        job = dict(label=label, request=request, path=path, output=output, finished=False,
                   handle=None, process=None, started=time.monotonic())
        self.jobs.append(job)
        record = dict(label=label, request=request, timedOut=False, log=str(path))
        job['record'] = record
        self.evidence['helpers'].append(record)
        try:
            process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=output, stderr=output, creationflags=subprocess.CREATE_NO_WINDOW)
            job['process'] = process
            record['pid'] = process.pid
            job['handle'] = self.api.open(process.pid)
            job['creation'] = self.api.times(job['handle'])['creation_filetime']
            record['creation'] = job['creation']
            return job
        except BaseException:
            self.stop(job)
            raise

    def rows(self, job):
        return [json.loads(line) for line in read_lines(job['path']) if line.startswith('{')]

    def before(self, job, seconds=12):
        deadline = time.monotonic() + seconds
        prefix = 'before-set' if job['request']['mode'] == 'set-path' else 'before-invoke'
        while time.monotonic() < deadline:
            self.verify()
            rows = self.rows(job)
            if rows:
                require(rows[0].get('kind') == prefix, 'expected pre-action marker')
                return
            require(job['process'].poll() is None, 'helper exited before marker')
            time.sleep(.05)
        job['record']['timedOut'] = True
        raise RuntimeError('pre-action helper deadline; never replay')

    def finish(self, job, seconds=12):
        deadline = time.monotonic() + seconds
        try:
            while job['process'].poll() is None:
                self.verify()
                require(time.monotonic() < deadline and time.monotonic() - job['started'] < 180, 'helper deadline; never replay')
                time.sleep(.05)
            code = job['process'].poll()
            rows = self.rows(job)
            job['record']['rows'] = rows
            require(code == 0, 'helper failure: ' + job['label'])
            providers = [json.loads(line.split(': ', 1)[1]) for line in read_lines(job['path']) if line.startswith('save-workflow-provider: ')]
            require(len(providers) == 1 and providers[0].get('ok') is True, 'one successful built-in UIA provider bootstrap')
            if job['request']['mode'] != 'probe':
                validate_actions(rows, job['request'])
            return rows
        finally:
            self.stop(job)

    def stop(self, job):
        if job['finished']:
            return
        process, record = job['process'], job['record']
        errors = []
        def audit(label, fn):
            try:
                fn()
            except BaseException as error:
                errors.append(dict(stage=label, error=f'{type(error).__name__}: {error}'))
        def terminate():
            if process is not None and process.poll() is None:
                # Popen retains the kernel process handle, so terminate cannot target a reused PID.
                record['terminated'] = True
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        audit('terminate', terminate)
        def process_record():
            record['exitCode'] = None if process is None else process.poll()
            record['exited'] = process is not None and process.poll() is not None
            require(record['exited'], 'helper process did not exit')
        audit('process-exit', process_record)
        def closed_identity():
            if job['handle'] is not None:
                times = self.api.times(job['handle'])
                record['exitIdentity'] = times
                require(times['creation_filetime'] == job['creation'] and times['exit_filetime'] >= job['creation'], 'helper closed identity')
        audit('closed-identity', closed_identity)
        def close_handle():
            if job['handle'] is not None:
                closed = self.api.kernel.CloseHandle(job['handle'])
                job['handle'] = None
                record['handleClosed'] = bool(closed)
                require(closed, 'helper handle close')
        audit('handle-close', close_handle)
        audit('output-close', job['output'].close)
        def retain_output():
            record['logSha256'] = digest(job['path'])
            record['logBytes'] = job['path'].stat().st_size
            require(record['logBytes'] <= 512 * 1024, 'bounded closed helper output')
            source = job['path'].read_text(encoding='utf-8-sig')
            record['partialLine'] = bool(source) and not source.endswith('\n')
            record['rows'] = []
            record['parseErrors'] = []
            for line in source.splitlines(keepends=True):
                if line.startswith('{') and line.endswith('\n'):
                    try:
                        record['rows'].append(json.loads(line))
                    except ValueError as error:
                        record['parseErrors'].append(str(error))
        audit('retain-output', retain_output)
        job['finished'] = True
        record['cleanupErrors'] = errors
        save(self.directory / f'uia-{job["label"]}-lifecycle.json', record)
        require(not errors, 'helper cleanup: ' + repr(errors))

    def call(self, request, label):
        return self.finish(self.start(request, label))

    def close(self):
        errors = []
        for job in self.jobs:
            try:
                self.stop(job)
            except BaseException as error:
                errors.append(f'{job["label"]}: {type(error).__name__}: {error}')
        require(not errors, 'helper cleanup errors: ' + repr(errors))
