"""Real temporary SQLite locking and fail-closed fixture protocol tests."""
import copy
from contextlib import closing
import json
from pathlib import Path
import sqlite3
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from slack_list_comments_storage_lock import StorageControl, PREFIX


class LockTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.expected = dict(version=1, nonce='ab' * 16, pid=123, fixture='slack-list')
        self.directory = Path(self.temporary.name).resolve() / ('slack-list-comments-compose-' + self.expected['nonce'])
        self.directory.mkdir()
        (self.directory / 'owner.json').write_text(json.dumps({k:self.expected[k] for k in ('version','nonce','fixture')}))
        self.db = self.directory / 'comments.sqlite'
        self.account = '00000000-0000-4000-8000-000000000001'
        self.inventory = dict(scope={'list':'FCOMMENT01'},revision=1,capture=self.account,blocks=[],attempt=None)
        with closing(sqlite3.connect(self.db)) as conn:
            conn.executescript('PRAGMA user_version=1; CREATE TABLE metadata(account TEXT); CREATE TABLE drafts(owner TEXT PRIMARY KEY,body TEXT); CREATE TABLE history(id TEXT PRIMARY KEY,owner TEXT,body TEXT);')
            conn.execute('INSERT INTO metadata VALUES(?)',(self.account,))
            conn.execute('INSERT INTO drafts VALUES(?,?)',(json.dumps(self.inventory['scope']),json.dumps(self.inventory)))
            conn.commit()
        self.requests = []
        self.native = SimpleNamespace(process=SimpleNamespace(pid=123),fixture='slack-list',receipts=self.receipts)

    def receipts(self,prefix):
        self.assertEqual(prefix,PREFIX)
        return copy.deepcopy(self.requests)

    def command(self, control, action):
        self.requests.append(dict(self.expected,sequence=len(self.requests)+1,action=action))
        control.service(self.native)

    def complete(self,control):
        while len(self.requests)<6:self.command(control,'hold' if len(self.requests)%2==0 else 'release')
        control.finish(self.native)

    def read(self):
        with closing(sqlite3.connect(self.db,timeout=.02)) as conn:
            return conn.execute('SELECT body FROM drafts').fetchall()

    def test_real_exclusive_lock_blocks_second_connection_and_releases(self):
        before=self.read()
        with StorageControl(self.directory,self.expected) as control:
            self.command(control,'hold')
            with self.assertRaisesRegex(sqlite3.OperationalError,'locked'):self.read()
            with self.assertRaisesRegex(sqlite3.OperationalError,'readonly'):
                control.connection.execute('DELETE FROM drafts')
            self.command(control,'release')
            self.assertEqual(self.read(),before)
            self.complete(control)
            acks=control.evidence['acknowledgments']
            self.assertEqual(len(acks),6)
            for hold,release in zip(acks[::2],acks[1::2]):
                self.assertEqual(hold['inventories'],[self.inventory])
                self.assertEqual(hold['inventories'],release['inventories'])
                self.assertEqual(hold['contentSha256'],release['contentSha256'])
                self.assertTrue(release['unchanged'])
            for ack in acks:
                self.assertEqual(json.loads((self.directory/f"storage-{ack['sequence']}.json").read_text()),ack)
        self.assertEqual(self.read(),before)

    def test_navigation_exact_four_commands_and_closed_log(self):
        before=self.read()
        with StorageControl(self.directory,self.expected,case='file-navigation-custody') as control:
            for action in ('hold','release','hold','release'):self.command(control,action)
            control.finish(self.native)
            self.assertEqual(len(control.evidence['acknowledgments']),4)
        control.validate_closed(self.native)
        self.assertEqual(self.read(),before)
        self.requests.append(dict(self.expected,sequence=5,action='hold'))
        with self.assertRaises(RuntimeError):control.validate_closed(self.native)

    def test_navigation_fifth_command_refused(self):
        with self.assertRaises(RuntimeError):
            with StorageControl(self.directory,self.expected,case='file-navigation-custody') as control:
                for action in ('hold','release','hold','release','hold'):self.command(control,action)
        self.assertEqual(len(self.read()),1)

    def test_storage_unknown_case_refused(self):
        with self.assertRaises(RuntimeError):StorageControl(self.directory,self.expected,case='arbitrary')

    def test_finally_unlocks_without_masking_primary_failure(self):
        control=StorageControl(self.directory,self.expected)
        with self.assertRaisesRegex(ValueError,'primary'):
            with control:
                self.command(control,'hold')
                raise ValueError('primary')
        self.assertEqual(len(self.read()),1)
        self.assertFalse(control.evidence['held'])
        self.assertFalse(control.evidence['finished'])
        self.assertEqual(control.evidence['cleanupErrors'],[])

    def test_terminal_cannot_hide_missing_release(self):
        control=StorageControl(self.directory,self.expected)
        with self.assertRaisesRegex(RuntimeError,'outstanding'):
            with control:
                self.command(control,'hold')
                control.finish(self.native)
        self.read()

    def test_clean_exit_requires_finish(self):
        with self.assertRaisesRegex(RuntimeError,'did not finish'):
            with StorageControl(self.directory,self.expected):pass

    def test_wrong_sequence_owner_action_and_extra_keys(self):
        for patch_value in ({'sequence':2},{'sequence':True},{'pid':456},{'nonce':'cd'*16},{'action':'release'},{'path':str(self.db)}):
            with self.subTest(patch_value=patch_value):
                self.requests=[dict(self.expected,sequence=1,action='hold',**{})]
                self.requests[0].update(patch_value)
                with self.assertRaises(RuntimeError):
                    with StorageControl(self.directory,self.expected) as control:control.service(self.native)
                self.read()

    def test_changed_protocol_history_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError,'history changed'):
            with StorageControl(self.directory,self.expected) as control:
                self.command(control,'hold')
                self.requests[0]['pid']=5
                control.service(self.native)
        self.read()

    def test_foreign_native_process_refused(self):
        self.native.process.pid=999
        with self.assertRaisesRegex(RuntimeError,'Foreign native'):
            with StorageControl(self.directory,self.expected) as control:control.service(self.native)

    def test_does_not_create_missing_database(self):
        self.db.unlink()
        with self.assertRaises(FileNotFoundError):
            with StorageControl(self.directory,self.expected) as control:self.command(control,'hold')
        self.assertFalse(self.db.exists())

    def test_owner_path_and_nonce_refusal(self):
        bad=dict(self.expected,nonce='cd'*16)
        with self.assertRaises(RuntimeError):StorageControl(self.directory,bad).__enter__()
        with self.assertRaises(RuntimeError):StorageControl(Path('relative'),self.expected).__enter__()
        (self.directory/'owner.json').write_text(json.dumps(dict(version=1,fixture='slack-list',nonce='cd'*16)))
        with self.assertRaises(RuntimeError):StorageControl(self.directory,self.expected).__enter__()

    def test_database_replacement_refused(self):
        with self.assertRaisesRegex(RuntimeError,'replaced'):
            with StorageControl(self.directory,self.expected) as control:
                self.command(control,'hold');self.command(control,'release')
                original=self.db.with_suffix('.original')
                self.db.rename(original)
                self.db.write_bytes(original.read_bytes())
                self.command(control,'hold')

    def test_symlink_and_hardlink_database_refused(self):
        target=self.db.with_suffix('.original')
        self.db.rename(target)
        import os
        os.link(target,self.db)
        with self.assertRaisesRegex(RuntimeError,'linked'):
            with StorageControl(self.directory,self.expected) as control:self.command(control,'hold')
        self.db.unlink();self.requests=[]
        try:self.db.symlink_to(target)
        except OSError:self.skipTest('Host does not allow symlink creation')
        with self.assertRaisesRegex(RuntimeError,'Symlink'):
            with StorageControl(self.directory,self.expected) as control:self.command(control,'hold')

    def test_existing_ack_is_never_overwritten(self):
        marker=self.directory/'storage-1.json';marker.write_text('original')
        with self.assertRaisesRegex(RuntimeError,'already exists'):
            with StorageControl(self.directory,self.expected) as control:self.command(control,'hold')
        self.assertEqual(marker.read_text(),'original')
        self.read()

    def test_actual_acquisition_timeout_is_bounded(self):
        conn=sqlite3.connect(self.db,isolation_level=None)
        conn.execute('BEGIN EXCLUSIVE')
        try:
            import time
            start=time.monotonic()
            with self.assertRaisesRegex(sqlite3.OperationalError,'locked'):
                with StorageControl(self.directory,self.expected) as control:self.command(control,'hold')
            self.assertLess(time.monotonic()-start,3)
        finally:conn.rollback();conn.close()

    def test_wrong_schema_or_inventory_refused(self):
        with closing(sqlite3.connect(self.db)) as conn:
            conn.execute('UPDATE drafts SET body=?',('{"scope":{},"scope":{}}',))
            conn.commit()
        with self.assertRaisesRegex(RuntimeError,'Duplicate'):
            with StorageControl(self.directory,self.expected) as control:self.command(control,'hold')
        self.read()

    def test_mutation_attempt_was_prevented_by_query_only(self):
        before=self.db.read_bytes()
        with StorageControl(self.directory,self.expected) as control:self.complete(control)
        self.assertEqual(self.db.read_bytes(),before)

    def test_closed_log_cannot_append_a_late_command(self):
        with StorageControl(self.directory,self.expected) as control:self.complete(control)
        control.validate_closed(self.native)
        self.requests.append(dict(self.expected,sequence=7,action='hold'))
        with self.assertRaisesRegex(RuntimeError,'Closed storage'):control.validate_closed(self.native)

    def test_hold_deadline_releases_on_failure(self):
        with self.assertRaisesRegex(RuntimeError,'deadline'):
            with StorageControl(self.directory,self.expected) as control:
                self.command(control,'hold')
                control._hold_started-=91
                control.service(self.native)
        self.read()

    def test_failed_ack_publication_retains_complete_temp_evidence(self):
        with self.assertRaisesRegex(OSError,'publish failed'):
            with StorageControl(self.directory,self.expected) as control:
                with patch('slack_list_comments_storage_lock.os.link',side_effect=OSError('publish failed')):
                    self.command(control,'hold')
        self.assertFalse((self.directory/'storage-1.json').exists())
        self.assertTrue(json.loads((self.directory/'storage-1.json.tmp').read_text())['ok'])
        self.read()

    def test_cleanup_failure_preserves_primary_exception(self):
        control=StorageControl(self.directory,self.expected)
        class Failed:
            def rollback(self):raise OSError('rollback failed')
            def close(self):raise OSError('close failed')
        with self.assertRaisesRegex(ValueError,'primary'):
            with control:
                control.connection=Failed()
                raise ValueError('primary')
        self.assertEqual([e['stage'] for e in control.evidence['cleanupErrors']],['rollback','close'])

    def test_reparse_flag_is_refused_even_without_symlink_privilege(self):
        import slack_list_comments_storage_lock as module
        original=Path.lstat
        def marked(path,*args,**kwargs):
            info=original(path,*args,**kwargs)
            if path==self.db:
                return SimpleNamespace(st_mode=info.st_mode,st_file_attributes=0x400)
            return info
        with patch.object(Path,'lstat',marked):
            with self.assertRaisesRegex(RuntimeError,'reparse'):
                with StorageControl(self.directory,self.expected) as control:self.command(control,'hold')


class EditLockTests(unittest.TestCase):
    receipts = LockTests.receipts
    command = LockTests.command
    read = LockTests.read

    def setUp(self):
        LockTests.setUp(self)
        scope = dict(workspace='T1', user='ULIST0001', list='FCOMMENT01', record='R_LIST_000',
                     root='1781524800.000001', created=1, creator='ULIST0001')
        target = dict(document={'id': scope['list']}, record={'id': scope['record'], 'date_created': 1,
                      'created_by': scope['creator']}, channel='CCOMMENT01', root=scope['root'])
        seed = dict(workspace=scope['workspace'], user=scope['user'], ts='1781524801.000001', target=target)
        self.attempt = dict(id='00000000-0000-4000-8000-000000000002', record={'seed': seed},
            capture=self.account, phase='Pending', raw=None, readback=None, retry_at=0)
        self.edit = dict(scope=scope, ts=seed['ts'], revision=3, capture=self.account, seed=seed,
                         blocks=[], attempt=self.attempt, retry_at=0)
        self.edit_owner = json.dumps([scope, seed['ts']])
        with closing(sqlite3.connect(self.db)) as db:
            db.executescript('CREATE TABLE edit_drafts(owner TEXT PRIMARY KEY,body TEXT); CREATE TABLE edit_history(id TEXT PRIMARY KEY,owner TEXT,body TEXT);')
            db.execute('INSERT INTO edit_drafts VALUES(?,?)', (self.edit_owner, json.dumps(self.edit)))
            db.execute('INSERT INTO edit_history VALUES(?,?,?)', (self.attempt['id'], self.edit_owner, json.dumps(self.attempt)))
            db.commit()

    def test_edit_eight_commands_original_history_and_readonly_lock(self):
        before = self.db.read_bytes()
        with StorageControl(self.directory, self.expected, case='own-edit-storage') as control:
            for pair in range(4):
                self.command(control, 'hold')
                with self.assertRaisesRegex(sqlite3.OperationalError, 'locked'):
                    self.read()
                with self.assertRaisesRegex(sqlite3.OperationalError, 'readonly'):
                    control.connection.execute('DELETE FROM edit_history')
                hold = control.evidence['acknowledgments'][-1]
                self.assertEqual(hold['editInventories'], [self.edit])
                self.assertEqual(hold['editHistory'], [self.attempt])
                self.command(control, 'release')
                release = control.evidence['acknowledgments'][-1]
                for key in ('account', 'inventories', 'editInventories', 'editHistory', 'contentSha256'):
                    self.assertEqual(hold[key], release[key])
            control.finish(self.native)
        control.validate_closed(self.native)
        self.assertEqual(self.db.read_bytes(), before)

    def test_legacy_ack_shape_and_hash_exclude_edit_fields(self):
        with StorageControl(self.directory, self.expected) as control:
            self.command(control, 'hold')
            ack = control.evidence['acknowledgments'][-1]
            self.assertNotIn('editInventories', ack)
            self.assertNotIn('editHistory', ack)
            self.command(control, 'release')
            while len(self.requests) < 6:
                self.command(control, 'hold' if len(self.requests) % 2 == 0 else 'release')
            control.finish(self.native)

    def test_edit_owner_and_history_cannot_diverge(self):
        with closing(sqlite3.connect(self.db)) as db:
            changed = copy.deepcopy(self.attempt)
            changed['phase'] = 'Observed'
            db.execute('UPDATE edit_history SET body=?', (json.dumps(changed),))
            db.commit()
        with self.assertRaisesRegex(RuntimeError, 'diverged'):
            with StorageControl(self.directory, self.expected, case='own-edit-storage') as control:
                self.command(control, 'hold')

    def test_history_original_owner_key_is_checked(self):
        with closing(sqlite3.connect(self.db)) as db:
            db.execute('UPDATE edit_history SET owner=?', (json.dumps([{}, self.edit['ts']]),))
            db.commit()
        with self.assertRaisesRegex(RuntimeError, 'Foreign edit history'):
            with StorageControl(self.directory, self.expected, case='own-edit-storage') as control:
                self.command(control, 'hold')

    def test_missing_history_not_synthesized(self):
        with closing(sqlite3.connect(self.db)) as db:
            db.execute('DELETE FROM edit_history')
            db.commit()
        with self.assertRaisesRegex(RuntimeError, 'diverged'):
            with StorageControl(self.directory, self.expected, case='own-edit-storage') as control:
                self.command(control, 'hold')

    def test_edit_lock_requires_all_eight_commands(self):
        with self.assertRaisesRegex(RuntimeError, 'outstanding'):
            with StorageControl(self.directory, self.expected, case='own-edit-storage') as control:
                for _ in range(3):
                    self.command(control, 'hold')
                    self.command(control, 'release')
                control.finish(self.native)

    def completed_edit(self):
        with StorageControl(self.directory, self.expected, case='own-edit-storage') as control:
            for _ in range(4):
                self.command(control, 'hold')
                self.command(control, 'release')
            control.finish(self.native)
        self.native.process.poll = lambda: 0
        self.native.cleanup_errors = []
        return control

    def test_closed_edit_history_is_independent_readonly_and_byte_unchanged(self):
        control = self.completed_edit()
        before = self.db.read_bytes()
        value = control.closed_edit_snapshot(self.native)
        self.assertTrue(value['readOnly'])
        self.assertEqual(value['snapshot']['editHistory'], [self.attempt])
        self.assertEqual(value['snapshot']['editInventories'], [self.edit])
        self.assertEqual(self.db.read_bytes(), before)
        self.assertIsNone(control.connection)

    def test_closed_proof_requires_exit_and_clean_cleanup(self):
        control = self.completed_edit()
        self.native.process.poll = lambda: None
        with self.assertRaisesRegex(RuntimeError, 'process exit'):
            control.closed_edit_snapshot(self.native)
        self.native.process.poll = lambda: 0
        self.native.cleanup_errors = ['failed cleanup']
        with self.assertRaisesRegex(RuntimeError, 'process exit'):
            control.closed_edit_snapshot(self.native)

    def test_closed_database_replacement_and_sidecars_refused(self):
        control = self.completed_edit()
        sidecar = Path(str(self.db) + '-wal')
        sidecar.write_bytes(b'pending database')
        with self.assertRaisesRegex(RuntimeError, 'sidecar'):
            control.closed_edit_snapshot(self.native)
        sidecar.unlink()
        self.db.rename(self.db.with_suffix('.original'))
        self.db.write_bytes(b'replacement')
        with self.assertRaisesRegex(RuntimeError, 'replaced'):
            control.closed_edit_snapshot(self.native)



class DeleteLockTests(unittest.TestCase):
    receipts=LockTests.receipts
    command=LockTests.command
    read=LockTests.read

    def setUp(self):
        LockTests.setUp(self)
        import runpy
        fixture=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_delete_storage.py')))
        _,receipt=fixture['sample']()
        self.deletion=receipt['stages'][4]['deleteInventory']
        self.attempt=self.deletion['attempt']
        self.owner=json.dumps([self.deletion['scope'],self.deletion['ts']])
        with closing(sqlite3.connect(self.db)) as conn:
            conn.executescript('CREATE TABLE edit_drafts(owner TEXT PRIMARY KEY,body TEXT NOT NULL); CREATE TABLE edit_history(id TEXT PRIMARY KEY,owner TEXT NOT NULL,body TEXT NOT NULL); CREATE TABLE delete_state(owner TEXT PRIMARY KEY,body TEXT NOT NULL); CREATE TABLE delete_history(id TEXT PRIMARY KEY,owner TEXT NOT NULL,body TEXT NOT NULL);')

    def persist(self):
        with closing(sqlite3.connect(self.db)) as db:
            db.execute('INSERT INTO delete_state VALUES(?,?)',(self.owner,json.dumps(self.deletion)))
            db.execute('INSERT INTO delete_history VALUES(?,?,?)',(self.attempt['id'],self.owner,json.dumps(self.attempt)))
            db.commit()

    def reject_hold(self):
        with self.assertRaises((RuntimeError,ValueError)):
            with StorageControl(self.directory,self.expected,case='own-delete-storage') as control:self.command(control,'hold')

    def completed(self):
        with StorageControl(self.directory,self.expected,case='own-delete-storage') as control:
            for action in ('hold','release','hold','release'):self.command(control,action)
            control.finish(self.native)
        self.native.process.poll=lambda:0
        self.native.cleanup_errors=[]
        return control

    def test_four_actual_locks_empty_then_pending_no_mutation(self):
        with StorageControl(self.directory,self.expected,case='own-delete-storage') as control:
            self.command(control,'hold')
            ack=control.evidence['acknowledgments'][-1]
            self.assertEqual(ack['deleteInventories'],[]);self.assertEqual(ack['deleteHistory'],[])
            with self.assertRaisesRegex(sqlite3.OperationalError,'locked'):self.read()
            self.command(control,'release')
            self.persist()
            before=self.db.read_bytes()
            self.command(control,'hold')
            ack=control.evidence['acknowledgments'][-1]
            self.assertEqual(ack['deleteInventories'],[self.deletion]);self.assertEqual(ack['deleteHistory'],[self.attempt])
            self.assertEqual(ack['editInventories'],[]);self.assertEqual(ack['editHistory'],[])
            with self.assertRaisesRegex(sqlite3.OperationalError,'readonly'):control.connection.execute('DELETE FROM delete_history')
            self.command(control,'release');control.finish(self.native)
        self.assertEqual(before,self.db.read_bytes());control.validate_closed(self.native)
        for hold,release in zip(control.evidence['acknowledgments'][::2],control.evidence['acknowledgments'][1::2]):
            self.assertEqual(release,dict(hold,sequence=hold['sequence']+1,action='release',unchanged=True))

    def test_exact_schema_required(self):
        with closing(sqlite3.connect(self.db)) as db:db.execute('ALTER TABLE delete_state ADD COLUMN extra TEXT')
        self.reject_hold()

    def test_inventory_history_receipt_divergence_refused(self):
        self.persist()
        with closing(sqlite3.connect(self.db)) as db:
            other=copy.deepcopy(self.attempt);other['phase']='Confirmed';other['raw']={'ok':True}
            db.execute('UPDATE delete_history SET body=?',(json.dumps(other),));db.commit()
        self.reject_hold()

    def test_original_history_owner_required(self):
        self.persist()
        with closing(sqlite3.connect(self.db)) as db:
            db.execute('UPDATE delete_history SET owner=?',(json.dumps([{},self.deletion['ts']]),));db.commit()
        self.reject_hold()

    def test_absent_state_orphan_refused(self):
        self.persist()
        with closing(sqlite3.connect(self.db)) as db:db.execute('DELETE FROM delete_state');db.commit()
        self.reject_hold()

    def test_oversize_body_refused_without_reading_unbounded_json(self):
        self.persist()
        with closing(sqlite3.connect(self.db)) as db:db.execute('UPDATE delete_state SET body=?',('x'*(1024*1024+1),));db.commit()
        self.reject_hold()

    def test_bounded_state_count(self):
        with closing(sqlite3.connect(self.db)) as db:
            db.executemany('INSERT INTO delete_state VALUES(?,?)',[(str(i),'{}') for i in range(129)]);db.commit()
        self.reject_hold()

    def test_closed_original_confirmed_receipt_is_readonly(self):
        self.deletion['revision']=2;self.attempt['phase']='Confirmed';self.attempt['raw']={'ok':True,'channel':'CCOMMENT01','ts':self.deletion['ts']}
        self.persist();control=self.completed();before=self.db.read_bytes()
        value=control.closed_delete_snapshot(self.native)
        self.assertTrue(value['readOnly']);self.assertEqual(value['snapshot']['deleteInventories'],[self.deletion])
        self.assertEqual(value['snapshot']['deleteHistory'],[self.attempt]);self.assertEqual(before,self.db.read_bytes())
        self.assertIsNone(control.connection)

    def test_closed_requires_exited_clean_process_and_no_sidecar(self):
        control=self.completed();self.native.process.poll=lambda:None
        with self.assertRaisesRegex(RuntimeError,'process exit'):control.closed_delete_snapshot(self.native)
        self.native.process.poll=lambda:0;self.native.cleanup_errors=['failure']
        with self.assertRaisesRegex(RuntimeError,'process exit'):control.closed_delete_snapshot(self.native)
        self.native.cleanup_errors=[];Path(str(self.db)+'-wal').write_bytes(b'pending')
        with self.assertRaisesRegex(RuntimeError,'sidecar'):control.closed_delete_snapshot(self.native)

    def test_legacy_cases_do_not_gain_deletion_fields(self):
        with StorageControl(self.directory,self.expected,case='own-edit-storage') as control:
            self.command(control,'hold');ack=control.evidence['acknowledgments'][-1]
            self.assertNotIn('deleteInventories',ack);self.assertNotIn('deleteHistory',ack)
            self.command(control,'release')
            for _ in range(3):self.command(control,'hold');self.command(control,'release')
            control.finish(self.native)


if __name__=='__main__':unittest.main()
