"""Pure contracts for the finite, explicitly reviewed native Save workflow."""
import hashlib
import math
import os
import re
from pathlib import Path
from save_dialog_discovery import require, read_json, owner, TITLE
from original_download_native import BYTES, expected_digest
from toast_geometry import validate_toast

ALIAS = 'original-download-save-workflow'
STAGES = ('initial', 'canceled', 'preserved', 'busy', 'saved')
CAPTURES = ('initial', 'cancel-dialog', 'canceled', 'existing-dialog',
            'overwrite-decline', 'after-decline', 'overwrite-accept',
            'preserved', 'new-dialog', 'busy', 'saved')
ACTIONS = ('cancel', 'set-existing', 'save-existing-1', 'decline',
           'save-existing-2', 'accept', 'set-new', 'save-new')
PRESERVED = 'The existing file was preserved. Download again and choose a new filename.'
RETURNS = [dict(attempt=1, kind='none', accepted=True),
           dict(attempt=2, kind='existing', accepted=True),
           dict(attempt=3, kind='new', accepted=True)]
OUTCOMES = [dict(attempt=2, kind='destination-exists'), dict(attempt=3, kind='saved')]


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def save(path, value):
    import json
    with Path(path).open('x', encoding='utf-8') as output:
        json.dump(value, output, indent=2, allow_nan=False)
        output.write('\n')


def identical(a, b):
    """Unlike Python equality, reject bool/int substitutions recursively."""
    if type(a) is not type(b):
        return False
    if isinstance(a, dict):
        return a.keys() == b.keys() and all(identical(a[k], b[k]) for k in a)
    if isinstance(a, list):
        return len(a) == len(b) and all(identical(x, y) for x, y in zip(a, b))
    return a == b


def native_owner(expected):
    return dict(version=1, nonce=expected['nonce'], pid=expected['pid'], fixture=ALIAS)


def validate_stage(record, index, expected, width=None):
    require(0 <= index < len(STAGES) and isinstance(record, dict), 'stage index/type')
    name = STAGES[index]
    require(record.get('ok') is True and record.get('stage') == name, 'exact native stage order')
    require(identical(record.get('owner'), native_owner(expected)), 'native stage owner')
    require(all(record.get(k) is True for k in ('noSession', 'noTransport', 'noAccount')), 'disconnected shell')
    require(type(record.get('issued')) is int and record['issued'] == (0, 1, 2, 3, 3)[index], 'issued attempts')
    require(identical(record.get('returns'), RETURNS[:(0, 1, 2, 3, 3)[index]]), 'exact return ledger')
    require(identical(record.get('outcomes'), OUTCOMES[:(0, 0, 1, 1, 2)[index]]), 'exact outcome ledger')
    dom = record.get('dom')
    require(isinstance(dom, dict) and dom.get('ok') is True and dom.get('stage') == name, 'DOM stage')
    require(identical(dom.get('viewport'), [1280, 780]), 'normal viewport')
    require(dom.get('label') == ('Cancel' if name == 'busy' else 'Download'), 'actual action label')
    value = dom.get('width')
    require(type(value) in (int, float) and math.isfinite(value) and value >= 96, 'stable action width')
    require(width is None or abs(value - width) <= 1, 'action width changed')
    toast = (None, None, PRESERVED, None, 'Saved original-fixture.bin')[index]
    require(record.get('toast') == toast, 'exact toast')
    if toast is not None:validate_toast(dom.get('toast'), dom['viewport'], toast)
    return value if width is None else width


def validate_native_logs(logs, expected, stages):
    require(identical(logs['waiting'], [native_owner(expected)]), 'one waiting record')
    requests = [dict(version=1, nonce=expected['nonce'], pid=expected['pid'], attempt=i, actualRfd=True) for i in (1, 2, 3)]
    returns = [dict(version=1, nonce=expected['nonce'], pid=expected['pid'], actualRfd=True,
                    transferAllowed=i != 1, **RETURNS[i-1]) for i in (1, 2, 3)]
    outcomes = [dict(version=1, nonce=expected['nonce'], pid=expected['pid'], **row) for row in OUTCOMES]
    require(identical(logs['requested'], requests), 'one request per real dialog')
    require(identical(logs['returned'], returns), 'actual rfd returns')
    require(identical(logs['outcome'], outcomes), 'typed transfer outcomes')
    require(identical(logs['stage'], stages), 'closed stage ledger changed')
    require(len(logs['check']) == 1, 'one terminal')
    terminal = logs['check'][0]
    require(terminal.get('ok') is True and identical(terminal.get('owner'), native_owner(expected)), 'terminal owner')
    require(identical(terminal.get('stages'), stages), 'full terminal stages')
    require(all(terminal.get(k) is True for k in ('actualRfd', 'noSession', 'noTransport', 'noAccount')), 'terminal scope')
    width = None
    require(len(stages) == 5, 'all five native stages')
    for index, stage in enumerate(stages):
        width = validate_stage(stage, index, expected, width)


def unlinked(path):
    path = Path(path)
    info = path.lstat()
    require(not path.is_symlink() and not (getattr(info, 'st_file_attributes', 0) & 0x400), 'linked/reparse path')
    return info


def identity(path):
    info = unlinked(path)
    return [info.st_dev, info.st_ino]


class DestinationAudit:
    """Owned directories and sentinel identity are frozen before readiness ACK."""
    def __init__(self, directory, nonce):
        self.directory = Path(directory)
        self.downloads = self.directory / 'downloads'
        self.nonce = nonce
        self.parents = [(p, identity(p)) for p in [self.downloads, self.directory, *self.directory.parents]]
        self.paths = [self.downloads / f'preserve-{nonce}.bin', self.downloads / f'saved-{nonce}.bin']
        require(list(self.downloads.iterdir()) == [], 'installer directory must initially be empty')
        self.sentinel = None

    def check_parents(self):
        for path, pinned in self.parents:
            require(identity(path) == pinned and path.is_dir(), 'owned directory changed')

    def create_sentinel(self):
        self.check_parents()
        require(self.sentinel is None and list(self.downloads.iterdir()) == [], 'sentinel one-use creation')
        with self.paths[0].open('xb') as output:
            output.write(b'Super Platinum disposable preserve sentinel\x00' + self.nonce.encode() + b'\xff\n')
            output.flush()
            os.fsync(output.fileno())
        self.sentinel = self.file(self.paths[0])
        return self.sentinel

    def file(self, path):
        info = unlinked(path)
        require(path.is_file(), 'regular destination')
        return dict(name=path.name, bytes=info.st_size, identity=identity(path), sha256=digest(path))

    def raw_snapshot(self, phase):
        """Retain safe bounded files even when the expected inventory fails."""
        result = dict(phase=phase, files=[], errors=[])
        try:
            self.check_parents()
            entries = []
            for path in self.downloads.iterdir():
                entries.append(path)
                if len(entries) == 5:
                    result['errors'].append('closed inventory exceeds four files')
                    break
            for path in sorted(entries[:4]):
                row = dict(name=path.name)
                result['files'].append(row)
                try:
                    info = unlinked(path)
                    row.update(bytes=info.st_size, identity=[info.st_dev, info.st_ino])
                    require(path.is_file() and info.st_size <= BYTES, 'bounded ordinary closed file')
                    row['sha256'] = digest(path)
                except (RuntimeError, OSError) as error:
                    row['error'] = f'{type(error).__name__}: {error}'
                    result['errors'].append(path.name + ': ' + row['error'])
        except (RuntimeError, OSError) as error:
            result['errors'].append(f'{type(error).__name__}: {error}')
        return result

    def snapshot(self, phase, *, saved=False, busy=False, pending_ok=False):
        self.check_parents()
        require(self.sentinel is not None, 'sentinel not yet created')
        require(self.file(self.paths[0]) == self.sentinel, 'sentinel bytes/identity changed')
        files = sorted(self.downloads.iterdir())
        wanted = [self.paths[0]] + ([self.paths[1]] if saved else [])
        if busy:
            parts = [p for p in files if re.fullmatch(r'\.super-platinum-download-[0-9a-f-]{36}\.part', p.name)]
            require(len(parts) <= 1, 'at most one actual staging file')
            if parts:
                part = parts[0]
                require(part.is_file() and 0 <= unlinked(part).st_size <= BYTES, 'bounded staging file')
            wanted += parts
        require(files == sorted(wanted), 'exact destination inventory/no residual parts')
        if busy and (not parts or parts[0].stat().st_size < 65536):
            require(pending_ok, 'staging write not settled')
            return None
        result = [self.file(p) for p in files]
        if saved:
            final = next(row for row in result if row['name'] == self.paths[1].name)
            require(final['bytes'] == BYTES and final['sha256'] == expected_digest(), 'full saved body')
        return dict(phase=phase, files=result)


def validate_probe(probe, expected, kind, hwnd, save_hwnd=None):
    require(isinstance(probe, dict) and probe.get('ok') is True and probe.get('mode') == 'probe', 'probe outcome')
    require(all(identical(probe.get(k), v) for k, v in expected.items()), 'probe owner')
    window = probe.get('window', {})
    require(probe.get('kind') == kind and type(window.get('hwnd')) is int and window['hwnd'] == hwnd, 'probe kind/HWND')
    if kind == 'save':
        require(window.get('title') == TITLE, 'Save title')
    else:
        chain = window.get('ownerChain')
        require(isinstance(chain, list) and chain and all(type(n) is int and n > 0 for n in chain), 'measured owner chain')
        require(hwnd not in chain and save_hwnd in chain and len(chain) <= 8 and len(set(chain)) == len(chain), 'overwrite rooted at observed Save')
    require(isinstance(probe.get('controls'), list) and 0 < len(probe['controls']) <= 80, 'bounded controls')
    require(len({tuple(c['runtimeId']) for c in probe['controls']}) == len(probe['controls']), 'unique control runtime IDs')


def validate_review(review, observed):
    require(set(review) == {'version', 'stage', 'observedSha256', 'imageSha256', 'request'}, 'exact review fields')
    require(identical(review['version'], 1) and review['stage'] == observed['stage'], 'stage-bound review')
    require(review['observedSha256'] == observed['_sha256'] and review['imageSha256'] == observed['imageSha256'], 'review evidence hashes')
    probe = observed['probe']; request = review['request']; action = observed['stage']
    keys = {'nonce', 'pid', 'creation', 'mode', 'kind', 'hwnd', 'window', 'target'}
    if action.startswith('set-') or action.startswith('save-'):
        keys.add('allowedPath')
    if not action.startswith('set-'):
        keys.add('role')
    if action.startswith('save-'):
        keys.add('filenameTarget')
    if probe['kind'] == 'overwrite':
        keys.add('saveHwnd')
    require(set(request) == keys, 'exact action request fields')
    require(all(identical(request[k], probe[k]) for k in ('nonce', 'pid', 'creation', 'kind')) and identical(request['window'], probe['window']) and identical(request['hwnd'], probe['window']['hwnd']), 'review owner/window')
    require(request['mode'] == ('set-path' if action.startswith('set-') else 'invoke'), 'review mode')
    target = request['target']
    require(sum(identical(target, c) for c in probe['controls']) == 1, 'exact observed target')
    require(target.get('enabled') is True and target.get('offscreen') is False, 'visible enabled target')
    if action.startswith('set-'):
        require(target.get('controlType') == 'ControlType.Edit' and target.get('value') is True and target.get('readOnly') is False, 'writable observed filename')
    else:
        require(target.get('controlType') == 'ControlType.Button' and target.get('invoke') is True, 'invokable observed button')
    if 'allowedPath' in keys:
        require(request['allowedPath'] == observed['allowedPaths'][1 if action.endswith('new') else 0], 'exact destination for stage')
    if 'role' in keys:
        require(request['role'] == ('save' if action.startswith('save-') else action), 'exact reviewed role')
        name = {'cancel': 'Cancel', 'save': 'Save', 'decline': 'No', 'accept': 'Yes'}[request['role']]
        require(target.get('name') in (name, '&' + name), 'reviewed button semantic name/locale')
    if 'filenameTarget' in keys:
        filename = request['filenameTarget']
        require(sum(identical(filename, c) for c in probe['controls']) == 1, 'exact observed filename control')
        require(filename.get('controlType') == 'ControlType.Edit' and filename.get('value') is True and filename.get('readOnly') is False, 'filename pattern')
    if probe['kind'] == 'overwrite':
        require(request['saveHwnd'] == observed['saveHwnd'], 'reviewed overwrite relationship')
    return request
