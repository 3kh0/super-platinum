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


if __name__=='__main__':unittest.main()
