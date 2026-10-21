"""Closed DB-only transfer for explicitly paired own-edit/own-delete fixtures."""
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import sqlite3
import stat

H = runpy.run_path(str(Path(__file__).with_name('check-slack-list-hydration-native.py')))
CASES = tuple('own-edit-restart-' + mode + '-' + phase
              for mode in ('unsent', 'unknown') for phase in ('exercise', 'restore'))
DELETE_CASES = tuple('own-delete-restart-' + mode + '-' + phase
    for mode in ('pending', 'unknown') for phase in ('exercise', 'restore'))
CASES += DELETE_CASES
DELETE_TABLES = {'delete_state': ('owner','body'), 'delete_history': ('id','owner','body')}
TABLES = {'metadata': ('account',), 'drafts': ('owner', 'body'), 'history': ('id', 'owner', 'body'),
          'edit_drafts': ('owner', 'body'), 'edit_history': ('id', 'owner', 'body')}
MAX_BYTES = 128 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def plain(path, *, directory=False):
    path = Path(path)
    require(path.is_absolute(), 'Restart artifact must be absolute')
    for parent in (path, *path.parents):
        info = parent.lstat()
        require(not stat.S_ISLNK(info.st_mode) and not (getattr(info, 'st_file_attributes', 0) & 0x400),
                'Linked/reparsed restart artifact refused')
        require(stat.S_ISDIR(info.st_mode) if parent != path or directory else stat.S_ISREG(info.st_mode),
                'Restart artifact type changed')
    return path.stat()


def sha(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def identity(info):
    return dict(device=info.st_dev, inode=info.st_ino, size=info.st_size, mtimeNs=info.st_mtime_ns)


def json_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'Duplicate key in persisted restart evidence')
        result[key] = value
    return result


def decoded(text):
    return json.loads(text, object_pairs_hook=json_object,
                      parse_constant=lambda value: (_ for _ in ()).throw(RuntimeError('Nonfinite persisted JSON')))


def database(path, *, operation='edit'):
    """Read-only independent SQL observation; never creates, checkpoints or repairs."""
    require(operation in ('edit','delete'), 'Unknown persisted recovery operation')
    path = Path(path)
    before = plain(path)
    require(16 <= before.st_size <= MAX_BYTES, 'Restart database size is invalid')
    for suffix in ('-wal', '-shm', '-journal'):
        sidecar = Path(str(path) + suffix)
        if sidecar.exists() or sidecar.is_symlink():
            require(plain(sidecar).st_size == 0, 'Restart database has unclosed sidecar content')
    digest = sha(path)
    with path.open('rb') as source:
        require(source.read(16) == b'SQLite format 3\x00', 'Wrong restart database format')
    connection = sqlite3.connect(path.as_uri() + '?mode=ro', uri=True, timeout=1)
    try:
        connection.execute('PRAGMA query_only=ON')
        require(connection.execute('PRAGMA quick_check').fetchall() == [('ok',)], 'Persisted restart database is corrupt')
        require(connection.execute('PRAGMA user_version').fetchone() == (1,), 'Wrong comment journal version')
        require(connection.execute('PRAGMA journal_mode').fetchone() == ('delete',), 'Restart requires closed DELETE-mode SQLite')
        names = {r[0] for r in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")}
        current_tables = dict(TABLES, **DELETE_TABLES)
        require(names == set(current_tables) or (operation == 'edit' and names == set(TABLES)), 'Unexpected persisted restart tables')
        tables, inventories = {}, {}
        for name, columns in current_tables.items():
            if name not in names: continue
            require(tuple(r[1] for r in connection.execute(f'PRAGMA table_info({name})')) == columns, 'Restart table shape changed')
            rows = connection.execute(f'SELECT {",".join(columns)} FROM {name} ORDER BY {columns[0]} LIMIT 258').fetchall()
            require(len(rows) <= (1 if name == 'metadata' else 256), 'Restart inventory exceeded bounds')
            require(all(all(isinstance(v, str) and len(v.encode('utf-8')) <= 1024 * 1024 for v in row) for row in rows),
                    'Malformed persisted restart row')
            tables[name] = [dict(zip(columns, row)) for row in rows]
            if name != 'metadata':
                inventories[name] = [decoded(row[-1]) for row in rows]
        require(len(tables['metadata']) == 1, 'Missing isolated journal account')
        if operation == 'edit':
            require(all(tables.get(name,[]) == [] for name in DELETE_TABLES), 'Own-edit transfer contains deletion authority')
        else:
            require(tables['edit_drafts'] == [] and tables['edit_history'] == [], 'Own-delete transfer contains edit authority')
    finally:
        connection.close()
    require(identity(path.stat()) == identity(before) and sha(path) == digest, 'Read-only database inspection changed or raced persisted bytes')
    return dict(path=str(path), sha256=digest, identity=identity(before), account=tables['metadata'][0]['account'],
                tables=tables, inventories=inventories)


def closed_wrapper(value, fixture, case, binary):
    require(case in CASES and value.get('case') == case and value.get('fixture') == fixture
        and fixture in ('slack-list', 'slack-list-narrow') and value.get('ok') is True
        and value.get('processExited') is True and value.get('closedLogRevalidated') is True
        and value.get('cleanupErrors') == [], 'First restart process lacks successful final closure')
    require(Path(value.get('binary', '')).resolve() == Path(binary).resolve()
        and value.get('binarySha256') == sha(binary), 'Restart binary changed')
    nonce, pid = value.get('nonce'), value.get('pid')
    require(isinstance(nonce, str) and re.fullmatch('[0-9a-f]{32}', nonce)
        and type(pid) is int and pid > 0, 'Invalid first process ownership')
    expected = dict(version=1, nonce=nonce, pid=pid, fixture=fixture)
    H['validate_ready'](value.get('readiness'), expected)
    H['validate_ready'](value.get('native', {}).get('readiness'), expected)
    require(value['native'].get('ok') is True and value['native'].get('case') == case, 'Missing original terminal restart receipt')
    if os.name == 'nt' or 'processCreationFiletime' in value:
        creation = value.get('processCreationFiletime')
        H['validate_exit'](creation, value.get('exitIdentity', {}))
        H['validate_exit'](creation, value.get('cleanupProcessIdentity', {}))
        H['validate_pointer_cleanup'](value.get('pointer', {}))
    directory = Path(value.get('readyDirectory', ''))
    plain(directory, directory=True)
    require(directory.name == 'slack-list-comments-compose-' + nonce, 'Wrong original private readiness directory')
    for name, expected_value in (('owner.json', dict(version=1, nonce=nonce, fixture=fixture)), ('ready.json', expected)):
        marker = directory / name
        require(plain(marker).st_size <= 1024 and decoded(marker.read_text(encoding='utf-8')) == expected_value,
                'Original closed readiness marker changed')
    return directory


def transfer(receipt_path, destination, *, fixture, case, binary):
    require(case in CASES and case.endswith('-restore'), 'DB transfer is limited to exact restart restore cases')
    receipt_path, destination = Path(receipt_path), Path(destination)
    require(plain(receipt_path).st_size <= 64 * 1024 * 1024, 'Restart wrapper exceeds bounds')
    original_text = receipt_path.read_bytes()
    previous = decoded(original_text.decode('utf-8'))
    source_dir = closed_wrapper(previous, fixture, case.removesuffix('-restore') + '-exercise', binary)
    log = receipt_path.parent / f'{fixture}.log'
    require(plain(log).st_size <= 64 * 1024 * 1024, 'Original closed log exceeds bounds')
    require(H['closed_receipts'](log, 'slack-list-comments-compose-waiting: ') == [previous['readiness']]
        and H['closed_receipts'](log, 'slack-list-comments-compose-check: ') == [previous['native']],
        'Original closed log no longer matches validated exercise receipt')
    stages = previous['native'].get('stages', [])
    require(stages and H['closed_receipts'](log, 'slack-list-comments-compose-stage: ')
        == [dict(stage=row['stage'], edge=edge) for row in stages for edge in ('before', 'after')],
        'Original closed capture lifecycle changed')
    for row in stages:
        path = source_dir / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture', '')) == path, 'Original restart screenshot changed directory')
        H['png_signature'](path, previous.get('expectedPngDimensions'))
    source = source_dir / 'comments.sqlite'
    plain(destination.parent, directory=True)
    require(destination.name == 'comments.sqlite' and not destination.exists() and not destination.is_symlink()
        and source != destination and source.parent != destination.parent, 'Restart needs a fresh isolated database destination')
    operation = 'delete' if case in DELETE_CASES else 'edit'
    before = database(source, operation=operation)
    if operation == 'delete':
        oracle=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_delete_restart.py')))
        oracle['validator'](previous['case'])(fixture,previous['native'],source_dir,dimensions=previous.get('expectedPngDimensions'))
        oracle['sql_custody'](before, stages[-1])
    with source.open('rb') as incoming, destination.open('xb') as outgoing:
        require(identity(os.fstat(incoming.fileno())) == before['identity'], 'Original closed DB replaced before copy')
        shutil.copyfileobj(incoming, outgoing, length=1024 * 1024)
        outgoing.flush()
        os.fsync(outgoing.fileno())
    copied = database(destination, operation=operation)
    require(copied['sha256'] == before['sha256'] and copied['tables'] == before['tables']
        and copied['account'] == before['account'] and identity(source.stat()) == before['identity']
        and sha(source) == before['sha256'] and receipt_path.read_bytes() == original_text, 'Original closed DB bytes changed during transfer')
    return dict(kind='closed-database-byte-copy', sourceReceipt=str(receipt_path),
        sourceReceiptSha256=hashlib.sha256(original_text).hexdigest(), source=before, destination=copied,
        sourceLog=str(log), sourceLogSha256=sha(log),
        originalPid=previous['pid'], originalNonce=previous['nonce'], originalBinarySha256=previous['binarySha256'])
