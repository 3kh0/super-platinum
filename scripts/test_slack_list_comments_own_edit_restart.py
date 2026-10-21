"""Closed SQLite transfer and exact fresh-process recovery oracle regressions."""
import copy
from contextlib import closing
import json
from pathlib import Path
import runpy
import sqlite3
import struct
import tempfile
import unittest
from unittest import mock
import zlib

S = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit_restart_storage.py')))


def png():
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 1, 1, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(b'\0\0\0\0')) + chunk(b'IEND', b'')


def database(path):
    with closing(sqlite3.connect(path)) as db:
        for name, columns in S['TABLES'].items():
            db.execute(f'CREATE TABLE {name}({",".join(c + " TEXT NOT NULL" for c in columns)})')
        db.execute('PRAGMA user_version=1')
        db.execute('INSERT INTO metadata VALUES(?)', ('8624eb74-23f4-4bee-9872-e37f392485c0',))
        db.execute('INSERT INTO drafts VALUES(?,?)', ('reply-owner', '{"blocks":[],"capture":"reply"}'))
        db.execute('INSERT INTO edit_drafts VALUES(?,?)', ('edit-owner', '{"blocks":[],"capture":"edit"}'))
        db.commit()


class TransferTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='sp-edit-restart-test-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.binary = self.root / 'closed-fixture.exe'
        self.binary.write_bytes(b'closed-test-binary')
        self.fixture = 'slack-list'
        self.case = 'own-edit-restart-unsent-restore'
        self.exercise = self.case.replace('-restore', '-exercise')
        self.outer = self.root / 'exercise'
        self.outer.mkdir()
        self.directory = self.outer / ('slack-list-comments-compose-' + 'a' * 32)
        self.directory.mkdir()
        self.copy_dir = self.root / ('slack-list-comments-compose-' + 'b' * 32)
        self.copy_dir.mkdir()
        self.destination = self.copy_dir / 'comments.sqlite'
        self.source = self.directory / 'comments.sqlite'
        database(self.source)
        ready = dict(version=1, nonce='a' * 32, pid=101, fixture=self.fixture)
        (self.directory / 'owner.json').write_text(json.dumps({k: v for k, v in ready.items() if k != 'pid'}), encoding='utf-8')
        (self.directory / 'ready.json').write_text(json.dumps(ready), encoding='utf-8')
        capture = self.directory / 'slack-list-exercise-draft.png'
        capture.write_bytes(png())
        native = dict(ok=True, case=self.exercise, fixture=self.fixture, readiness=ready,
                      stages=[dict(stage='exercise-draft', capture=str(capture))])
        self.value = dict(ok=True, fixture=self.fixture, case=self.exercise, processExited=True,
            closedLogRevalidated=True, cleanupErrors=[], binary=str(self.binary), binarySha256=S['sha'](self.binary),
            nonce=ready['nonce'], pid=ready['pid'], readiness=ready, native=native, readyDirectory=str(self.directory),
            processCreationFiletime=100, exitIdentity=dict(creation_filetime=100, exit_filetime=200),
            cleanupProcessIdentity=dict(creation_filetime=100, exit_filetime=200), expectedPngDimensions=[1, 1],
            pointer=dict(movement_observed=False, restore_observed=True, restore_attempted=True))
        self.receipt = self.outer / 'slack-list.json'
        self.write_wrapper()
        lines = ['slack-list-comments-compose-waiting: ' + json.dumps(ready)]
        lines += ['slack-list-comments-compose-stage: ' + json.dumps(dict(stage='exercise-draft', edge=edge)) for edge in ('before', 'after')]
        lines += ['slack-list-comments-compose-check: ' + json.dumps(native)]
        self.log = self.outer / 'slack-list.log'
        self.log.write_text('\n'.join(lines) + '\n', encoding='utf-8')

    def write_wrapper(self):
        self.receipt.write_text(json.dumps(self.value), encoding='utf-8')

    def transfer(self):
        return S['transfer'](self.receipt, self.destination, fixture=self.fixture, case=self.case, binary=self.binary)

    def rejected(self):
        with self.assertRaises((RuntimeError, OSError, ValueError, sqlite3.DatabaseError)):
            self.transfer()

    def test_exact_closed_bytes_only_into_fresh_directory(self):
        original = self.source.read_bytes()
        evidence = self.transfer()
        self.assertEqual(original, self.source.read_bytes())
        self.assertEqual(original, self.destination.read_bytes())
        self.assertEqual(evidence['source']['sha256'], evidence['destination']['sha256'])
        self.assertEqual([p.name for p in self.copy_dir.iterdir()], ['comments.sqlite'])
        self.assertNotEqual(evidence['source']['identity']['inode'], evidence['destination']['identity']['inode'])

    def test_current_seven_table_journal_accepts_only_empty_deletion_tables(self):
        with closing(sqlite3.connect(self.source)) as db:
            db.execute('CREATE TABLE delete_state(owner TEXT PRIMARY KEY,body TEXT NOT NULL)')
            db.execute('CREATE TABLE delete_history(id TEXT PRIMARY KEY,owner TEXT NOT NULL,body TEXT NOT NULL)')
        evidence=self.transfer()
        self.assertEqual(evidence['source']['inventories']['delete_state'],[])
        self.assertEqual(evidence['source']['inventories']['delete_history'],[])
        self.assertEqual(evidence['source']['sha256'],evidence['destination']['sha256'])

    def test_partial_or_nonempty_deletion_tables_refuse_old_edit_transfer(self):
        with closing(sqlite3.connect(self.source)) as db:
            db.execute('CREATE TABLE delete_state(owner TEXT PRIMARY KEY,body TEXT NOT NULL)')
        self.rejected()
        with closing(sqlite3.connect(self.source)) as db:
            db.execute('CREATE TABLE delete_history(id TEXT PRIMARY KEY,owner TEXT NOT NULL,body TEXT NOT NULL)')
            db.execute('INSERT INTO delete_state VALUES(?,?)',('owner','{}'));db.commit()
        self.rejected()

    def test_running_or_unvalidated_process_cannot_transfer(self):
        for key in ('processExited', 'closedLogRevalidated', 'ok'):
            with self.subTest(key=key):
                self.value[key] = False
                self.write_wrapper()
                self.rejected()
                self.value[key] = True

    def test_failed_cleanup_cannot_transfer(self):
        self.value['cleanupErrors'] = [{'stage': 'process'}]
        self.write_wrapper()
        self.rejected()

    def test_retained_process_identity_required(self):
        self.value['cleanupProcessIdentity']['creation_filetime'] = 101
        self.write_wrapper()
        self.rejected()

    def test_binary_cannot_change_between_processes(self):
        self.binary.write_bytes(b'different')
        self.rejected()

    def test_wrong_phase_case_or_width_cannot_transfer(self):
        for key, value in (('case', 'own-edit-restart-unknown-exercise'), ('fixture', 'slack-list-narrow')):
            original = self.value[key]
            self.value[key] = value
            self.write_wrapper()
            self.rejected()
            self.value[key] = original

    def test_original_pid_bound_marker_required(self):
        marker = self.directory / 'ready.json'
        value = json.loads(marker.read_text())
        value['pid'] += 1
        marker.write_text(json.dumps(value))
        self.rejected()

    def test_closed_log_and_png_revalidated(self):
        self.log.write_text(self.log.read_text().rstrip('\n'))
        self.rejected()

    def test_foreign_or_corrupt_original_png_rejected(self):
        (self.directory / 'slack-list-exercise-draft.png').write_bytes(b'not-a-png')
        self.rejected()

    def test_existing_destination_never_overwritten(self):
        self.destination.write_bytes(b'retain')
        self.rejected()
        self.assertEqual(self.destination.read_bytes(), b'retain')

    def test_nonempty_sqlite_sidecar_refused(self):
        for suffix in ('-wal', '-shm', '-journal'):
            sidecar = Path(str(self.source) + suffix)
            sidecar.write_bytes(b'not-closed')
            self.rejected()
            sidecar.unlink()

    def test_duplicate_keys_and_unexpected_schema_refused(self):
        with closing(sqlite3.connect(self.source)) as db:
            db.execute('UPDATE edit_drafts SET body=?', ('{"phase":"Pending","phase":"Observed"}',))
            db.commit()
        self.rejected()

    def test_missing_database_is_not_created(self):
        self.source.unlink()
        self.rejected()
        self.assertFalse(self.source.exists())

    def test_unrelated_tables_refused(self):
        with closing(sqlite3.connect(self.source)) as db:
            db.execute('CREATE TABLE session(token TEXT)')
            db.commit()
        self.rejected()


V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit_restart.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_edit.py')))
O, B = V['O'], BASE['B']
DIRECTORY = Path('/private/own-edit-restart')


def native_pair(mode='unknown', narrow=False):
    fixture, base = BASE['sample'](narrow)
    template = base['stages'][0]
    scope = template['journal']['scope']
    seed = V['seed_value'](template['model'])
    edited = O['received']()['blocks'][:1]
    edited[0]['elements'][-1]['elements'][-1]['text'] += ' Persisted edit B \u65e5\U0001f98a'
    newer = copy.deepcopy(edited)
    newer[0]['elements'][-1]['elements'][-1]['text'] += ' Newer edit C \u65e5\U0001f642'
    empty = dict(scope=scope, ts=O['TS'], revision=0, capture='', seed=None, blocks=[], attempt=None, retry_at=0)
    saved = dict(scope=scope, ts=O['TS'], revision=2, capture='00000000-0000-4000-8000-000000000071',
                 seed=seed, blocks=edited, attempt=None, retry_at=0)
    original_attempt = dict(id='00000000-0000-4000-8000-000000000072',
        record=dict(version=1, seed=seed, blocks=edited + O['received']()['blocks'][1:], text=V['G']['display'](edited)),
        capture=saved['capture'], phase='Uncertain', raw=None, readback=None, retry_at=0)
    closed = copy.deepcopy(saved)
    if mode == 'unknown':
        closed.update(revision=4, attempt=copy.deepcopy(original_attempt))
    readset = template['model']['comments']['ledger']['requests']
    receipts = []
    for restore in (False, True):
        case = 'own-edit-restart-' + mode + ('-restore' if restore else '-exercise')
        rows = []
        ready = copy.deepcopy(base['readiness'])
        ready.update(pid=202 if restore else 101, nonce=('b' if restore else 'a') * 32)
        remembered = dict(ok=True, scope='list-edit:100:77', mount='restart-mount', generation='77', blocks=copy.deepcopy(edited))
        final_attempt = copy.deepcopy(original_attempt)
        if mode == 'unknown':
            page = copy.deepcopy(seed['http'])
            page['messages'] = [O['update_row'](edited) if r.get('ts') == O['TS'] else r for r in page['messages']]
            final_attempt.update(phase='Observed', readback=dict(target=copy.deepcopy(seed['target']), raw=page))
        for i, name in enumerate(V['STAGES'][case]):
            row = copy.deepcopy(template)
            row.update(stage=name, capture=str(DIRECTORY / ('restore' if restore else 'exercise') / f'{fixture}-{name}.png'))
            m, f, edit = (row['model'][k] for k in ('comments', 'fixture', 'edit'))
            g = row['geometry']
            active, unknown, observed, notice = V['native_state'](case, name)
            published = restore and mode == 'unknown' and i >= 4
            cleared = name in ('cancelled', 'reopened')
            current_blocks = newer if restore and mode == 'unknown' and i >= 3 and not cleared else edited
            read_count = V['counts'](case)[i]
            m['ledger'] = dict(requests=[dict(copy.deepcopy(q), serial=j + 1) for j, q in enumerate(readset * (read_count // 4))],
                completed=read_count, replies=read_count // 4, cancelled=0, held=False)
            c = m['comments']
            c['generation'] = 200 if name == 'reopened' else 100
            if published:
                c['page']['messages'] = [O['update_row'](edited) if r.get('ts') == O['TS'] else r for r in c['page']['messages']]
                c['pageMessages'] = copy.deepcopy(c['page']['messages'])
            draft = m['draftStore']['drafts'][0]
            draft.update(id='restored' if name == 'reopened' else 'original', generation=40 if name == 'reopened' else 20)
            m['draftStore']['active'] = f"{draft['id']}:{c['generation']}:{draft['generation']}"
            issued = name == 'exercise-unconfirmed'
            f.update(policy=[dict(request=q, response=r) for _ in range(i if not restore else 0) for q, r in O['policy_cycle']()],
                prepared=[O['wire'](edited)] if issued else [], updates=[O['wire'](edited)] if issued else [],
                responses=[dict(disposition='unconfirmed', raw=None)] if issued else [], futureRows=[O['update_row'](edited)] if issued else [],
                held=False, releases=[], published=published, publishedRow=O['update_row'](edited) if published else None)
            inv = copy.deepcopy(closed if restore or issued else saved if i else empty)
            if restore and mode == 'unknown' and i >= 3:
                inv.update(revision=5, capture='00000000-0000-4000-8000-000000000073', blocks=copy.deepcopy(newer))
            if published:
                inv.update(revision=6, attempt=copy.deepcopy(final_attempt))
            if cleared:
                inv.update(revision=7 if mode == 'unknown' else 3, capture='00000000-0000-4000-8000-000000000074', seed=None, blocks=[], attempt=None)
            row['editInventory'] = inv
            attempt_count = int(mode == 'unknown' and ((restore and i >= 1) or issued))
            edit.update(opening=False, events=0, retired=[], active=None, attempts=[dict(id=123, ts=O['TS'],
                blocks=copy.deepcopy(edited) + O['received']()['blocks'][1:], phase='observed' if published else 'unknown',
                receipt=None, retryAt=0)] if attempt_count else [])
            row['model']['capture'] = None
            if active:
                edit['active'] = dict(scope=remembered['scope'], generation=77, ts=O['TS'], current=not observed,
                    blocks=copy.deepcopy(current_blocks), revision=0 if restore and i <= 2 else 1, busy=False, composing=False,
                    nativeReady=True, captureFailed=False, notice=notice, source=O['received'](), preserved=O['received']()['blocks'][1:],
                    attempt=123 if attempt_count else None)
                row['model']['capture'] = dict(scope=remembered['scope'], generation=77, mount=remembered['mount'],
                    failed=False, canSave=not unknown and not observed, canCancel=not unknown, canCheck=unknown, canRetry=False, busy=False)
            p = dict(source=dict(received=O['received'](), boot=O['boot']()), replySeed=None if restore else template['proof']['replySeed'],
                replyBlocks=copy.deepcopy(template['proof']['replyBlocks']), initialEditInventory=copy.deepcopy(closed if restore else empty),
                initialReplyInventory=copy.deepcopy(template['journal']) if restore else dict(scope=scope, revision=0, capture='', blocks=[], attempt=None),
                processId=ready['pid'], scenario=mode, phase='restore' if restore else 'exercise', editSuffix=V['EDIT'], newerSuffix=V['NEWER'])
            if i >= 1:
                p.update(editedBlocks=copy.deepcopy(edited), editRemembered=copy.deepcopy(remembered))
            if mode == 'unknown' and ((restore and i >= 1) or issued):
                p['blocked'] = dict(ok=True, saveDisabled=True, cancelDisabled=True)
            if restore and mode == 'unknown' and i >= 3:
                p['newerBlocks'] = copy.deepcopy(newer)
            if published:
                p['publication'] = dict(attemptId=original_attempt['id'], record=copy.deepcopy(original_attempt['record']),
                    row=O['update_row'](edited), originalReceipt=None, updates=0, fromPersistedRecord=True)
            if name == 'reopened':
                p['runtimeForgotten'] = True
            row['proof'] = p
            controls = [('slack-list-comment-edit-cancel', unknown), ('slack-list-comment-edit-save', unknown or observed)] if active else [
                ('slack-list-comments-back', False), ('slack-list-comment-send', False)]
            if unknown:
                controls.append(('slack-list-comment-edit-check', False))
            g.update(editing=active, controls=[dict(id=id, disabled=disabled, visible=True, hit=True, rect=B['rect']()) for id, disabled in controls],
                edit=dict(blocks=copy.deepcopy(current_blocks), sameNode=True, editable=True, rect=B['rect']()) if active else None,
                notice=dict(text=notice, visible=True, hit=True, rect=B['rect']()) if notice else None,
                text=B['textproof'](), inertTextPaint=None, captureFault=None, replyNotice=None,
                row=dict(text=(O['update_row'](edited) if published else O['received']())['text'] + ' Preserved original context', rect=B['rect']()))
            g['reply'].update(sameNode=name != 'reopened', hidden=active, inert=active, visible=not active, composing=False)
            rows.append(row)
        receipts.append(dict(ok=True, case=case, fixture=fixture, readiness=ready, stages=rows,
            final=copy.deepcopy(rows[-1]['model']), editInventory=copy.deepcopy(rows[-1]['editInventory']),
            initialRecords=base['initialRecords'], noLiveNetwork=True))
    return fixture, receipts[0], receipts[1]


def sql_proof(row, history, *, name='original'):
    scope = row['journal']['scope']
    values = dict(drafts=[row['journal']], history=[], edit_drafts=[row['editInventory']], edit_history=history)
    tables = dict(metadata=[dict(account=row['model']['comments']['account'])], drafts=[], history=[], edit_drafts=[], edit_history=[])
    for table, records in values.items():
        for record in records:
            stored = dict(owner=json.dumps(scope if table == 'drafts' else [scope, O['TS']]), body=json.dumps(record))
            if table == 'edit_history':
                stored['id'] = record['id']
            tables[table].append(stored)
    return dict(path='/private/' + name + '.sqlite', sha256='a' * 64, account=row['model']['comments']['account'], tables=tables, inventories=values)


class RestartOracleTests(unittest.TestCase):
    def check(self, fixture, receipt):
        phase = 'restore' if receipt['case'].endswith('-restore') else 'exercise'
        return V['validator'](receipt['case'])(fixture, receipt, DIRECTORY / phase, png=False)

    def reject(self, mutation, index=3, *, mode='unknown', restore=True):
        fixture, exercise, second = native_pair(mode)
        receipt = second if restore else exercise
        mutation(receipt['stages'][index])
        receipt['final'] = copy.deepcopy(receipt['stages'][-1]['model'])
        receipt['editInventory'] = copy.deepcopy(receipt['stages'][-1]['editInventory'])
        with self.assertRaises((RuntimeError, KeyError, ValueError, TypeError, IndexError)):
            self.check(fixture, receipt)

    def test_all_four_cases_both_widths_and_exact_sql_pair(self):
        for mode in ('unsent', 'unknown'):
            for narrow in (False, True):
                fixture, exercise, restore = native_pair(mode, narrow)
                self.check(fixture, exercise)
                self.check(fixture, restore)
                history = [exercise['editInventory']['attempt']] if mode == 'unknown' else []
                original = sql_proof(exercise['stages'][-1], history)
                copied = copy.deepcopy(original)
                copied['path'] = '/private/copied.sqlite'
                final_history = [restore['stages'][4]['editInventory']['attempt']] if mode == 'unknown' else []
                final = sql_proof(restore['stages'][-1], final_history, name='final')
                self.assertTrue(V['validate_pair'](mode, exercise, restore, original, copied, final))

    def test_restore_cannot_seed_reply_from_runtime(self):
        self.reject(lambda r: r['proof'].update(replySeed={'blocks': []}), 0)
        self.reject(lambda r: r['proof']['initialReplyInventory'].update(capture='runtime-copy'), 0)

    def test_inert_restore_has_no_update_or_policy(self):
        self.reject(lambda r: r['model']['fixture']['updates'].append(O['wire'](r['proof']['editedBlocks'])), 1)
        self.reject(lambda r: r['model']['fixture']['policy'].append({'method': 'users.prefs.get'}), 1)

    def test_saved_original_seed_target_http_and_context_exact(self):
        self.reject(lambda r: r['editInventory']['seed']['target'].update(channel='COTHER'), 1)
        self.reject(lambda r: r['editInventory']['seed']['http']['messages'].pop(), 1)
        self.reject(lambda r: r['editInventory']['seed'].update(preserved=[]), 1)

    def test_unknown_original_uuid_and_receipt_preserved(self):
        self.reject(lambda r: r['editInventory']['attempt'].update(id='00000000-0000-4000-8000-000000000099'), 2)
        self.reject(lambda r: r['editInventory']['attempt'].update(raw={'ok': True}), 4)
        self.reject(lambda r: r['editInventory']['attempt'].update(phase='Confirmed'), 4)

    def test_observed_is_separate_positive_page_not_ack(self):
        self.reject(lambda r: r['editInventory']['attempt'].update(readback=None), 4)
        self.reject(lambda r: r['model']['edit']['attempts'][0].update(phase='confirmed'), 4)
        self.reject(lambda r: r['proof']['publication'].update(originalReceipt={'ok': True}), 4)

    def test_negative_check_cannot_resolve_or_rewrite(self):
        self.reject(lambda r: r['editInventory'].update(revision=999), 2)
        self.reject(lambda r: r['model']['edit']['attempts'][0].update(phase='observed'), 2)

    def test_newer_native_and_sql_buffer_retained_after_old_readback(self):
        self.reject(lambda r: r['editInventory'].update(blocks=r['proof']['editedBlocks']), 4)
        self.reject(lambda r: r['geometry']['edit'].update(blocks=r['proof']['editedBlocks']), 4)
        self.reject(lambda r: r['editInventory'].update(capture='00000000-0000-4000-8000-000000000098'), 4)

    def test_newer_capture_must_not_replace_attempt_capture(self):
        self.reject(lambda r: r['editInventory']['attempt'].update(capture=r['editInventory']['capture']), 3)

    def test_independent_reply_and_opaque_content_survive(self):
        self.reject(lambda r: r['journal'].update(revision=999), 4)
        self.reject(lambda r: r['geometry']['reply']['blocks'][0]['elements'][0]['elements'][1]['opaque'].update(retain=[]), 4)

    def test_native_mount_generation_and_controls(self):
        self.reject(lambda r: r['model']['capture'].update(mount='new'), 3)
        self.reject(lambda r: r['geometry']['edit'].update(sameNode=False), 3)
        self.reject(lambda r: r['geometry']['controls'][1].update(disabled=False), 3)

    def test_pending_unknown_cancel_is_not_allowed(self):
        self.reject(lambda r: r['model']['capture'].update(canCancel=True), 2)

    def test_cancel_clears_only_edit_and_preserves_history(self):
        self.reject(lambda r: r['editInventory'].update(seed=r['proof']['initialEditInventory']['seed']), 5)
        fixture, exercise, restore = native_pair()
        old = sql_proof(exercise['stages'][-1], [exercise['editInventory']['attempt']])
        copied = copy.deepcopy(old); copied['path'] = '/private/copy.sqlite'
        final = sql_proof(restore['stages'][-1], [])
        with self.assertRaises(RuntimeError):
            V['validate_pair']('unknown', exercise, restore, old, copied, final)

    def test_sql_account_and_row_owner_exact(self):
        fixture, exercise, restore = native_pair()
        old = sql_proof(exercise['stages'][-1], [exercise['editInventory']['attempt']])
        copied = copy.deepcopy(old); copied['path'] = '/private/copy.sqlite'
        final = sql_proof(restore['stages'][-1], [restore['stages'][4]['editInventory']['attempt']])
        final['tables']['edit_history'][0]['owner'] = json.dumps([{}, O['TS']])
        with self.assertRaises(RuntimeError):
            V['validate_pair']('unknown', exercise, restore, old, copied, final)

    def test_reopen_requires_actual_forget_and_reply_node_change(self):
        self.reject(lambda r: r['proof'].update(runtimeForgotten=False), 6)
        self.reject(lambda r: r['geometry']['reply'].update(sameNode=True), 6)

    def test_clipped_text_or_missing_recovery_notice_rejected(self):
        self.reject(lambda r: r['geometry']['text'].update(ok=False, visibleLines=0), 1)
        self.reject(lambda r: r['geometry'].update(notice=None), 1)


class RunnerBoundaryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.runner = runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
        cls.pair = runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-own-edit-restart-native.py')))

    def test_explicit_case_contracts_and_legacy_dispatch(self):
        for case, stages in V['STAGES'].items():
            actual, validate = self.runner['case_contract'](case)
            self.assertEqual(actual, stages)
            self.assertTrue(callable(validate))
        self.assertEqual(self.runner['case_contract']('own-edit')[0], O['STAGES']['own-edit'])

    def test_restore_requires_closed_source_before_process_start(self):
        with tempfile.TemporaryDirectory(prefix='sp-edit-runner-test-') as name:
            with self.assertRaisesRegex(RuntimeError, 'paired restart'):
                self.runner['run_fixture']('slack-list', Path(name) / 'unused.exe', Path(name) / 'captures',
                    'own-edit-restart-unsent-restore')
            self.assertFalse((Path(name) / 'captures').exists())

    def test_legacy_cases_refuse_restart_source(self):
        with tempfile.TemporaryDirectory(prefix='sp-edit-runner-test-') as name:
            with self.assertRaisesRegex(RuntimeError, 'paired restart'):
                self.runner['run_fixture']('slack-list', Path(name) / 'unused.exe', Path(name) / 'captures',
                    'own-edit', restart_from=Path(name) / 'untrusted.json')
            self.assertFalse((Path(name) / 'captures').exists())

    def test_second_process_not_started_after_incomplete_first(self):
        fn = self.pair['run_pair']
        calls = []
        def first(*args, **kwargs):
            calls.append((args, kwargs))
            return dict(ok=True, processExited=False, closedLogRevalidated=True, cleanupErrors=[])
        with tempfile.TemporaryDirectory(prefix='sp-edit-pair-test-') as name:
            with mock.patch.dict(fn.__globals__, build_proof=lambda *args: {'sha': 'fixed'}), mock.patch.dict(fn.__globals__['C'], run_fixture=first), mock.patch('builtins.print'):
                with self.assertRaisesRegex(RuntimeError, 'First process must close'):
                    fn('unsent', 'slack-list', Path(name) / 'unused.exe', Path(name) / 'captures')
            self.assertEqual(len(calls), 1)
            report = json.loads((Path(name) / 'captures/restart.json').read_text())
            self.assertFalse(report['ok'])

    def test_build_proof_rejects_changed_compiled_input(self):
        fn = self.pair['build_proof']
        with tempfile.TemporaryDirectory(prefix='sp-edit-source-test-') as name:
            root = Path(name).resolve()
            binary, result = root / 'native.exe', root / 'result.json'
            binary.write_bytes(b'exact binary')
            source = root / 'source.rs'
            source.write_text('fn original() {}')
            (root / 'input-hashes.json').write_text(json.dumps({'source.rs': S['sha'](source)}))
            result.write_text(json.dumps(dict(exitCode=0, inputsUnchanged=True, binarySha256=S['sha'](binary))))
            with mock.patch.dict(fn.__globals__, ROOT=root):
                self.assertEqual(fn(binary, result)['inputCount'], 1)
                source.write_text('fn changed() {}')
                with self.assertRaisesRegex(RuntimeError, 'Compiled source changed'):
                    fn(binary, result)


if __name__ == '__main__':
    unittest.main()
