"""Closed fixture SQLite lock protocol. Never creates or modifies journal data."""
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import stat
import time
import uuid

PREFIX = 'slack-list-comments-compose-storage: '
MAX_BODY = 1024 * 1024
MAX_SCOPES = 128
MAX_HISTORY = 128


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def strict_json(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, 'Duplicate JSON key')
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=pairs,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError('Nonfinite JSON')))


def identity(path, directory=False):
    info = path.lstat()
    require(not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0) & 0x400,
            'Symlink/reparse storage path refused')
    require(stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode), 'Wrong storage path type')
    if not directory:
        require(info.st_nlink == 1, 'Multiply linked storage file refused')
    return info.st_dev, info.st_ino


class StorageControl:
    """One nonce-owned database, exact named command count, rollback on failure."""
    def __init__(self, directory, expected, *, case='storage-recovery'):
        require(case in ('storage-recovery','file-navigation-custody','own-edit-storage','own-delete-storage'), 'Unsupported storage case')
        self.case = case
        self.command_count = {'file-navigation-custody':4, 'storage-recovery':6, 'own-edit-storage':8, 'own-delete-storage':4}[case]
        self.directory = Path(directory)
        self.expected = dict(expected)
        self.connection = None
        self._requests = []
        self._before = None
        self._db_identity = None
        self._entered = False
        self._hold_started = None
        self.evidence = dict(protocol=1, requests=[], acknowledgments=[], cleanupErrors=[],
                             held=False, finished=False, mutations=False)

    def __enter__(self):
        e = self.expected
        require(set(e) == {'version', 'nonce', 'pid', 'fixture'} and type(e['version']) is int and e['version'] == 1
                and type(e['pid']) is int and e['pid'] > 0 and e['fixture'] in ('slack-list', 'slack-list-narrow')
                and isinstance(e['nonce'], str) and re.fullmatch('[0-9a-f]{32}', e['nonce']), 'Invalid storage readiness')
        require(self.directory.is_absolute() and self.directory.name == 'slack-list-comments-compose-' + e['nonce'],
                'Storage must use exact isolated nonce directory')
        for ancestor in reversed((self.directory, *self.directory.parents)):
            identity(ancestor, directory=True)
        require(self.directory.resolve(strict=True) == self.directory, 'Redirected storage directory')
        self._directory_identity = identity(self.directory, directory=True)
        self.owner_path = self.directory / 'owner.json'
        self._owner_identity = identity(self.owner_path)
        require(self.owner_path.stat().st_size <= 4096, 'Oversized storage owner')
        self._owner = self.owner_path.read_bytes()
        require(strict_json(self._owner) == {key: e[key] for key in ('version', 'nonce', 'fixture')}, 'Foreign storage owner')
        self.path = self.directory / 'comments.sqlite'
        self._entered = True
        self.evidence['database'] = str(self.path)
        self.evidence['readiness'] = dict(e)
        return self

    def _paths(self, *, closed=False):
        require(self._entered or (closed and self.evidence['finished']), 'Storage controller not entered')
        for ancestor in reversed((self.directory, *self.directory.parents)):
            identity(ancestor, directory=True)
        require(identity(self.directory, directory=True) == self._directory_identity
                and identity(self.owner_path) == self._owner_identity and self.owner_path.read_bytes() == self._owner,
                'Storage directory owner changed')
        found = identity(self.path)
        require(self.path.stat().st_size <= 512 * MAX_BODY, 'Oversized fixture database')
        if self._db_identity is None:
            self._db_identity = found
        require(found == self._db_identity, 'Storage database replaced')
        for suffix in ('-journal', '-wal', '-shm'):
            sidecar = Path(str(self.path) + suffix)
            if sidecar.exists() or sidecar.is_symlink():
                identity(sidecar)

    def _snapshot(self):
        conn = self.connection
        require(conn.execute('PRAGMA user_version').fetchone() == (1,), 'Foreign journal version')
        require(conn.execute('PRAGMA journal_mode').fetchone() == ('delete',), 'Fixture journal is not DELETE mode')
        accounts = conn.execute('SELECT account FROM metadata LIMIT 2').fetchall()
        require(len(accounts) == 1 and str(uuid.UUID(accounts[0][0])) == accounts[0][0], 'Foreign journal account')
        drafts = conn.execute('SELECT owner,body FROM drafts ORDER BY owner LIMIT 129').fetchall()
        history = conn.execute('SELECT id,owner,body FROM history ORDER BY id LIMIT 129').fetchall()
        require(len(drafts) <= MAX_SCOPES and len(history) <= MAX_HISTORY, 'Unbounded fixture journal')
        inventories = []
        for owner, body in drafts:
            require(isinstance(body, str) and len(body.encode('utf-8')) <= MAX_BODY, 'Oversized journal inventory')
            value = strict_json(body)
            require(set(value) == {'scope', 'revision', 'capture', 'blocks', 'attempt'}
                    and strict_json(owner) == value['scope'], 'Mismatched journal inventory')
            inventories.append(value)
        for row in history:
            require(all(isinstance(value, str) and len(value.encode('utf-8')) <= MAX_BODY for value in row),
                    'Oversized journal history')
        rows = [accounts, drafts, history]
        extra = {}
        if self.case in ('own-edit-storage','own-delete-storage'):
            edit_rows = conn.execute('SELECT owner,body FROM edit_drafts ORDER BY owner LIMIT 129').fetchall()
            edit_history = conn.execute('SELECT id,owner,body FROM edit_history ORDER BY id LIMIT 257').fetchall()
            require(len(edit_rows) <= 128 and len(edit_history) <= 256, 'Unbounded edit recovery journal')
            edits, attempts = [], []
            for owner, body in edit_rows:
                require(all(isinstance(v, str) and len(v.encode('utf-8')) <= MAX_BODY for v in (owner, body)),
                        'Oversized edit inventory')
                value = strict_json(body)
                require(isinstance(value, dict) and set(value) == {'scope','ts','revision','capture','seed','blocks','attempt','retry_at'}
                        and strict_json(owner) == [value['scope'], value['ts']], 'Mismatched edit inventory owner')
                edits.append(value)
            for id, owner, body in edit_history:
                require(all(isinstance(v, str) and len(v.encode('utf-8')) <= MAX_BODY for v in (id, owner, body)),
                        'Oversized edit attempt history')
                value = strict_json(body)
                require(isinstance(value, dict) and set(value) == {'id','record','capture','phase','raw','readback','retry_at'}
                        and value['id'] == id and str(uuid.UUID(id)) == id, 'Mismatched edit attempt history')
                seed = value.get('record', {}).get('seed', {})
                target = seed.get('target', {})
                record = target.get('record', {})
                scope = dict(workspace=seed.get('workspace'), user=seed.get('user'),
                    list=target.get('document', {}).get('id'), record=record.get('id'),
                    created=record.get('date_created'), creator=record.get('created_by'), root=target.get('root'))
                require(strict_json(owner) == [scope, seed.get('ts')], 'Foreign edit history scope')
                attempts.append(value)
            for value in edits:
                if value['attempt'] is not None:
                    require(sum(a == value['attempt'] for a in attempts) == 1, 'Edit inventory/history attempt diverged')
            rows += [edit_rows, edit_history]
            extra = dict(editInventories=edits, editHistory=attempts)
        if self.case == 'own-delete-storage':
            delete_rows, delete_history, deletions, attempts = self._delete_snapshot()
            rows += [delete_rows, delete_history]
            extra.update(deleteInventories=deletions, deleteHistory=attempts)
        body = json.dumps(rows, ensure_ascii=True, separators=(',', ':')).encode()
        return dict(account=accounts[0][0], inventories=inventories, contentSha256=hashlib.sha256(body).hexdigest(), **extra)

    def _delete_snapshot(self):
        conn=self.connection
        for table,expected in (
            ('delete_state',[('owner','TEXT',0,None,1),('body','TEXT',1,None,0)]),
            ('delete_history',[('id','TEXT',0,None,1),('owner','TEXT',1,None,0),('body','TEXT',1,None,0)])):
            require([tuple(r[1:]) for r in conn.execute(f'PRAGMA table_info({table})')] == expected,
                    'Unexpected deletion table schema')
        bounded='CASE WHEN length(CAST(body AS BLOB))<=? THEN body ELSE NULL END'
        states=conn.execute(f'SELECT owner,{bounded} FROM delete_state ORDER BY owner LIMIT 129',(MAX_BODY,)).fetchall()
        history=conn.execute(f'SELECT id,owner,{bounded} FROM delete_history ORDER BY id LIMIT 257',(MAX_BODY,)).fetchall()
        require(len(states)<=128 and len(history)<=256,'Unbounded deletion recovery journal')
        require(sum(len(v.encode('utf-8')) for row in states+history for v in row if isinstance(v,str))<=32*MAX_BODY,
                'Unbounded deletion recovery bytes')
        attempts=[]
        for aid,owner,body in history:
            require(all(isinstance(v,str) and len(v.encode('utf-8'))<=MAX_BODY for v in (aid,owner,body)), 'Oversized deletion history')
            a=strict_json(body)
            require(isinstance(a,dict) and set(a)=={'id','record','phase','raw','readback','retry_at'}
                and a['id']==aid and str(uuid.UUID(aid))==aid
                and a['phase'] in ('Pending','Uncertain','NotSent','Rejected','RateLimited','Confirmed','Observed')
                and type(a['retry_at']) is int and 0<=a['retry_at']<=2**63-1, 'Malformed deletion attempt')
            r=a['record'];require(isinstance(r,dict) and set(r)=={'version','workspace','user','target','ts','http','message'},'Malformed deletion record')
            target=r.get('target',{});record=target.get('record',{})
            scope=dict(workspace=r.get('workspace'),user=r.get('user'),list=target.get('document',{}).get('id'),
                record=record.get('id'),created=record.get('date_created'),creator=record.get('created_by'),root=target.get('root'))
            require(strict_json(owner)==[scope,r.get('ts')],'Foreign deletion history scope')
            attempts.append(a)
        inventories=[]
        for owner,body in states:
            require(all(isinstance(v,str) and len(v.encode('utf-8'))<=MAX_BODY for v in (owner,body)), 'Oversized deletion inventory')
            value=strict_json(body)
            require(isinstance(value,dict) and set(value)=={'scope','ts','revision','attempt','retry_at'}
                and type(value['revision']) is int and 0<value['revision']<=2**63-1
                and type(value['retry_at']) is int and 0<=value['retry_at']<=2**63-1
                and strict_json(owner)==[value['scope'],value['ts']], 'Mismatched deletion inventory owner/revision')
            require(isinstance(value['attempt'],dict) and value['attempt'].get('retry_at')==value['retry_at']
                and sum(a==value['attempt'] and strict_json(h[1])==[value['scope'],value['ts']] for a,h in zip(attempts,history))==1,'Deletion inventory/history attempt diverged')
            inventories.append(value)
        require(all(any(strict_json(owner)==[v['scope'],v['ts']] for v in inventories) for _,owner,_ in history),
                'Orphaned deletion history without state')
        return states,history,inventories,attempts

    def _hold(self):
        require(self.connection is None, 'Duplicate storage hold')
        self._paths()
        conn = sqlite3.connect(self.path.as_uri() + '?mode=rw', uri=True, timeout=1.0, isolation_level=None)
        self.connection = conn
        try:
            # Prevent DDL/DML even if a future edit accidentally adds a write statement.
            conn.execute('BEGIN EXCLUSIVE')
            conn.execute('PRAGMA query_only=ON')
            self._paths()
            result = self._snapshot()
            self._before = result
            self._hold_started = time.monotonic()
            self.evidence['held'] = True
            return result
        except BaseException:
            self._cleanup()
            raise

    def _release(self):
        require(self.connection is not None, 'Storage release without hold')
        self._paths()
        result = self._snapshot()
        require(result == self._before, 'Journal changed while exclusive lock held')
        self.connection.rollback()
        self.connection.close()
        self.connection = None
        self._hold_started = None
        self.evidence['held'] = False
        return dict(result, unchanged=True)

    def _ack(self, value):
        self._paths()
        path = self.directory / f"storage-{value['sequence']}.json"
        temporary = self.directory / f"storage-{value['sequence']}.json.tmp"
        require(not path.exists() and not path.is_symlink() and not temporary.exists() and not temporary.is_symlink(),
                'Storage ACK path already exists')
        published = False
        try:
            encoded = (json.dumps(value, ensure_ascii=True) + '\n').encode('utf-8')
            require(len(encoded) <= MAX_BODY, 'Oversized storage ACK')
            with temporary.open('xb') as stream:
                stream.write(encoded)
                stream.flush()
                os.fsync(stream.fileno())
            # Atomic publication without replacing an existing file.
            os.link(temporary, path)
            published = True
        finally:
            if published:
                temporary.unlink()

    def service(self, native):
        require(self._entered and not self.evidence['finished'], 'Storage service outside active run')
        require(self._hold_started is None or time.monotonic() - self._hold_started <= 90,
                'Storage hold exceeded bounded fixture deadline')
        require(native.process.pid == self.expected['pid'] and native.fixture == self.expected['fixture'], 'Foreign native process')
        requests = native.receipts(PREFIX)
        require(requests[:len(self._requests)] == self._requests and len(requests) >= len(self._requests), 'Storage protocol history changed')
        for request in requests[len(self._requests):]:
            sequence = len(self._requests) + 1
            action = 'hold' if sequence % 2 else 'release'
            expected = dict(self.expected, sequence=sequence, action=action)
            require(sequence <= self.command_count and request == expected and all(type(request.get(k)) is int for k in ('sequence','version','pid')),
                    'Invalid storage command/sequence/owner')
            self.evidence['requests'].append(dict(request))
            data = self._hold() if action == 'hold' else self._release()
            ack = dict(expected, ok=True, **data)
            self._ack(ack)
            self._requests.append(dict(request))
            self.evidence['acknowledgments'].append(ack)

    def finish(self, native):
        self.service(native)
        require(len(self._requests) == self.command_count and self.connection is None and not self.evidence['held'], 'Storage terminal has outstanding/missing commands')
        self._paths()
        self.evidence['finished'] = True

    def validate_closed(self, native):
        require(self.evidence['finished'] and not self.evidence['held'] and self.connection is None
                and not self.evidence['cleanupErrors'] and len(self._requests) == self.command_count
                and native.process.pid == self.expected['pid'] and native.fixture == self.expected['fixture']
                and native.receipts(PREFIX) == self._requests, 'Closed storage protocol changed or incomplete')
        return self.evidence

    def closed_edit_snapshot(self, native):
        """Inspect retained edit history only after the owned process has exited."""
        require(self.case == 'own-edit-storage', 'Wrong closed edit case')
        return self._closed_snapshot(native)

    def closed_delete_snapshot(self, native):
        """Deletion evidence adds no permission to mutate or replay the database."""
        require(self.case == 'own-delete-storage', 'Wrong closed deletion case')
        return self._closed_snapshot(native)

    def _closed_snapshot(self, native):
        require(native.process.poll() is not None
                and not native.cleanup_errors and not self._entered, 'Edit history requires clean process exit')
        self.validate_closed(native)
        self._paths(closed=True)
        for suffix in ('-journal', '-wal', '-shm'):
            path = Path(str(self.path) + suffix)
            require(not path.exists() or path.stat().st_size == 0, 'Nonempty closed SQLite sidecar')
        before = hashlib.sha256(self.path.read_bytes()).hexdigest()
        conn = sqlite3.connect(self.path.as_uri() + '?mode=ro', uri=True, timeout=1.0, isolation_level=None)
        self.connection = conn
        try:
            conn.execute('PRAGMA query_only=ON')
            conn.execute('BEGIN')
            snapshot = self._snapshot()
            conn.rollback()
        finally:
            conn.close()
            self.connection = None
        self._paths(closed=True)
        require(hashlib.sha256(self.path.read_bytes()).hexdigest() == before, 'Closed database changed during read-only proof')
        result = dict(snapshot=snapshot, sha256=before, readOnly=True)
        self.evidence['closedDatabase'] = result
        return result

    def _cleanup(self):
        if self.connection is not None:
            conn, self.connection = self.connection, None
            for name, callback in (('rollback', conn.rollback), ('close', conn.close)):
                try:
                    callback()
                except BaseException as error:
                    self.evidence['cleanupErrors'].append(dict(stage=name, type=type(error).__name__, message=str(error)))
            self.evidence['held'] = False
            self._hold_started = None

    def __exit__(self, kind, error, traceback):
        unfinished = not self.evidence['finished']
        self._cleanup()
        self._entered = False
        if kind is None:
            require(not unfinished and not self.evidence['cleanupErrors'], 'Storage protocol did not finish cleanly')
        return False
