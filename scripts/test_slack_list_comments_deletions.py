"""Deletion receipt adversaries; pure offline data without native or network work."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_deletions.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_reaction_refresh.py')))
DIR = Path('/private/list-comments-deletions')


def sample(narrow=False):
    fixture, receipt = BASE['sample'](narrow)
    receipt['case'] = 'deletions'; receipt.pop('reactionEvents')
    rows = receipt['stages']
    first, tail = V['FIRST_TS'], rows[6]['model']['composeLedger']['messages'][0]['ts']
    page = copy.deepcopy(rows[0]['model']['comments']['page'])
    stale_tail = copy.deepcopy(page)
    stale_tail['messages'].extend(copy.deepcopy(rows[6]['model']['composeLedger']['messages']))
    b = copy.deepcopy(rows[7]['proof']['newerB'])
    b[0]['elements'][1]['elements'][0]['text'] = 'Retain this draft during comment deletion.'
    for i, row in enumerate(rows):
        row.update(stage=V['STAGES'][i], capture=str(DIR / f'{fixture}-{V["STAGES"][i]}.png'))
        comments, g, proof = row['model']['comments'], row['geometry'], row['proof']
        comments['page'] = copy.deepcopy(stale_tail if i == 11 else page)
        comments['deleted'] = [] if i < 3 else [first] if i < 9 else [first, tail]
        visible = [first] if i < 3 else [tail] if 6 <= i < 9 else [V['READ']['ROOT_TS']]
        comments['visibleTs'] = visible
        projected = [dict(ts=ts, reactions=[dict(name='wave', count=2)] if ts == first else []) for ts in visible]
        comments['reactions'] = copy.deepcopy(projected)
        g['rows'] = [dict(copy.deepcopy(p), text='First page comment.' if p['ts'] == first else
                    'Record comment root' if p['ts'] == V['READ']['ROOT_TS'] else 'Accepted original A') for p in projected]
        probe = row.pop('reactionProbe'); probe.pop('count'); probe['visibleTs'] = copy.deepcopy(visible)
        row['deletionProbe'] = probe
        proof.pop('serverUpdate')
        proof['serverResponse'] = copy.deepcopy(stale_tail if i >= 7 else page)
        proof['heldResponse'] = copy.deepcopy(page if i in (1, 2, 3, 4) else stale_tail if i in (8, 9, 10) else None)
        if i >= 7:
            proof['newerB'] = copy.deepcopy(b); proof['journal']['blocks'] = copy.deepcopy(b)
            row['model']['draftStore']['drafts'][0]['blocks'] = copy.deepcopy(b)
            g['editor'].update(blocks=copy.deepcopy(b), text='Newer draft B \u65e5\U0001f98a\nRetain this draft during comment deletion.')
    events = []
    for stage, team, generation, channel, ts, changed, before, after in (
        ('ignored-held', 'T_OTHER', 7, 'CCOMMENT01', first, False, 1, 1),
        ('ignored-held', 'T1', 6, 'CCOMMENT01', first, False, 1, 1),
        ('ignored-held', 'T1', 7, 'COTHER', first, False, 1, 1),
        ('ignored-held', 'T1', 7, 'CCOMMENT01', '1999999999.000001', False, 1, 1),
        ('ignored-held', 'T1', 7, 'CCOMMENT01', V['READ']['ROOT_TS'], False, 1, 1),
        ('deleted', 'T1', 7, 'CCOMMENT01', first, True, 2, 3),
        ('deleted', 'T1', 7, 'CCOMMENT01', first, False, 3, 3),
        ('tail-deleted', 'T1', 7, 'CCOMMENT01', tail, True, 8, 9),
        ('tail-deleted', 'T1', 7, 'CCOMMENT01', tail, False, 9, 9)):
        events.append(dict(stage=stage, team=team, generation=generation, changed=changed,
            before={k:copy.deepcopy(rows[before]['model']['comments'][k]) for k in ('read','visibleTs','deleted')},
            after={k:copy.deepcopy(rows[after]['model']['comments'][k]) for k in ('read','visibleTs','deleted')},
            frame=dict(type='message', subtype='message_deleted', channel=channel, deleted_ts=ts, event_ts='1781525900.000001')))
    receipt.update(deletionEvents=events, final=copy.deepcopy(rows[-1]['model']))
    return fixture, receipt


class DeletionsTests(unittest.TestCase):
    def check(self, fixture, receipt):
        return V['validate'](fixture, receipt, DIR, png=False)

    def reject(self, change, index=3):
        f, r = sample(); change(r['stages'][index])
        with self.assertRaises((RuntimeError, KeyError, TypeError, ValueError)): self.check(f, r)

    def test_both_widths(self):
        for narrow in (False, True): self.check(*sample(narrow))

    def test_dispatch_and_unicode(self):
        stages, validator = V['C']['case_contract']('deletions')
        self.assertEqual(stages, V['STAGES']); self.assertEqual(validator.__name__, 'validate')
        self.assertEqual([ord(c) for c in V['B_TEXT'] if ord(c) > 127], [0x65e5, 0x1f98a])

    def test_envelope_order_owner_and_final(self):
        for mutate in (lambda r:r.update(case='reaction-refresh'), lambda r:r['stages'].pop(),
                       lambda r:r['readiness'].update(pid=False), lambda r:r.update(final={})):
            f, r = sample(); mutate(r)
            with self.assertRaises(RuntimeError): self.check(f, r)
        for key, value in [('account','other'), ('noSession',False), ('noTransport',False), ('writes',1), ('networkCalls',1), ('epoch',99)]:
            self.reject(lambda s:s['model'].update({key:value}))
        self.reject(lambda s:s['model'].update(records={}))

    def test_ignored_frames_keep_read_and_rows(self):
        self.reject(lambda s:s['model']['ledger'].update(cancelled=1, completed=8), 2)
        self.reject(lambda s:s['model']['comments'].update(read=99), 2)
        self.reject(lambda s:s['model']['comments'].update(visibleTs=[]), 2)
        self.reject(lambda s:s['model']['comments'].update(deleted=[V['FIRST_TS']]), 2)

    def test_current_deletion_removes_both_model_and_dom(self):
        for i in (3, 4, 5, 9, 10, 11):
            self.reject(lambda s:s['model']['comments']['visibleTs'].append(V['FIRST_TS']), i)
            self.reject(lambda s:s['geometry']['rows'].append(dict(ts=V['FIRST_TS'], text='resurrected', reactions=[])), i)
            self.reject(lambda s:s['deletionProbe']['visibleTs'].append(V['FIRST_TS']), i)
        self.reject(lambda s:s['model']['comments']['reactions'].append(dict(ts=V['FIRST_TS'], reactions=[])))

    def test_deletion_retains_root_page_cursor_and_source(self):
        self.reject(lambda s:s['model']['comments'].update(root='other'))
        self.reject(lambda s:s['model']['comments'].update(hasMore=False))
        self.reject(lambda s:s['model']['comments']['page']['messages'].pop(0))
        self.reject(lambda s:s['model']['comments']['page']['response_metadata'].update(next_cursor='other'))
        self.reject(lambda s:s['model']['comments']['page']['messages'][1].update(text='rewritten raw'))

    def test_last_reply_deletion_exposes_exact_surviving_root(self):
        for i in (3, 4, 5, 9, 10, 11):
            self.reject(lambda s:s['model']['comments'].update(visibleTs=[]), i)
            self.reject(lambda s:s['geometry']['rows'].clear(), i)
            self.reject(lambda s:s['geometry']['rows'][0].update(text='Wrong root body'), i)
            self.reject(lambda s:s['model']['comments']['deleted'].append(V['READ']['ROOT_TS']), i)
        f, r = sample()
        r['stages'][0]['model']['comments']['page']['messages'][0]['text'] = 'Changed root source'
        with self.assertRaises(RuntimeError): self.check(f, r)

    def test_preserved_native_a_root_fallback_receipt(self):
        path = Path(__file__).resolve().parents[1] / 'tmp/list-comments-deletions-20261018/native-a/slack-list/slack-list.json'
        if not path.is_file(): self.skipTest('Preserved offline native receipt is local evidence')
        outer = json.loads(path.read_text(encoding='utf-8')); receipt = outer['terminalReceipts'][0]
        self.assertFalse(outer['ok']); self.assertIn('Deleted reply resurrected or visible rows changed', outer['error'])
        for i in (3, 4, 5, 9, 10, 11):
            row = receipt['stages'][i]
            self.assertEqual(row['model']['comments']['visibleTs'], [V['READ']['ROOT_TS']])
            self.assertEqual([r['ts'] for r in row['geometry']['rows']], [V['READ']['ROOT_TS']])
            self.assertTrue(row['geometry']['rows'][0]['text'].endswith('Record comment root'))
        V['validate']('slack-list', receipt, Path(outer['readyDirectory']), png=False)

    def test_visit_tombstones_survive_explicit_reads(self):
        for i in (3, 4, 5, 6, 8, 9, 10, 11):
            self.reject(lambda s:s['model']['comments'].update(deleted=[]), i)
        self.reject(lambda s:s['model']['comments']['deleted'].append(V['READ']['ROOT_TS']), 9)
        self.reject(lambda s:s['model']['comments']['deleted'].append('foreign'), 5)

    def test_stale_http_bodies_are_actually_present_not_rewritten_for_success(self):
        for i in (3, 5, 9, 11):
            self.reject(lambda s:s['proof']['serverResponse']['messages'].pop(1), i)
        self.reject(lambda s:s['proof']['serverResponse']['messages'].pop(), 11)
        self.reject(lambda s:s['proof']['heldResponse']['messages'].pop(1), 4)
        self.reject(lambda s:s['proof']['heldResponse']['messages'].pop(), 10)
        self.reject(lambda s:s['proof'].update(serverUpdate={}))

    def test_read_cancellation_and_no_auto_request(self):
        for i in (3, 9):
            self.reject(lambda s:s['model']['comments'].update(loading=True), i)
            self.reject(lambda s:s['model']['comments'].update(error=None), i)
            self.reject(lambda s:s['model']['ledger'].update(cancelled=0), i)
            self.reject(lambda s:s['model']['ledger']['requests'].append(copy.deepcopy(s['model']['ledger']['requests'][0])), i)
        self.reject(lambda s:s['model']['comments'].update(read=4), 3)

    def test_transport_release_cancelled_and_stable(self):
        for i in (4, 10):
            self.reject(lambda s:s['proof'].update(release=dict(delivered=True, cancelledBeforeReply=False)), i)
            self.reject(lambda s:s['model']['ledger'].update(held=True), i)
            self.reject(lambda s:s['proof']['release'].update(cancelledBeforeReply=1), i)

    def test_exact_event_identity_scope_and_duplicate_observations(self):
        for i in range(9):
            f, r = sample(); r['deletionEvents'][i]['changed'] = not r['deletionEvents'][i]['changed']
            with self.assertRaises(RuntimeError): self.check(f, r)
        for i in (4, 6, 8):
            f, r = sample(); r['deletionEvents'][i]['after']['read'] += 1
            with self.assertRaises(RuntimeError): self.check(f, r)
        for key, value in [('channel','foreign'), ('subtype','message_changed'), ('deleted_ts',V['READ']['ROOT_TS'])]:
            f, r = sample(); r['deletionEvents'][5]['frame'][key] = value
            with self.assertRaises(RuntimeError): self.check(f, r)

    def test_no_duplicate_send_and_positive_identity(self):
        self.reject(lambda s:s['model']['composeLedger']['posts'].clear(), 6)
        self.reject(lambda s:s['model']['composeLedger']['messages'][0].update(user='other'), 6)
        self.reject(lambda s:s['model']['composeLedger']['messages'][0].update(blocks=[]), 9)
        self.reject(lambda s:s['model']['composeLedger'].update(published=True), 6)
        self.reject(lambda s:s['model']['composeLedger'].update(published=False), 7)
        self.reject(lambda s:s['model']['composeLedger']['preflights'].append({}), 8)

    def test_native_node_binding_and_focus_within_held_segments(self):
        for i in (0, 2, 3, 5, 7, 9, 11): self.reject(lambda s:s['geometry']['editor'].update(sameNode=False), i)
        self.reject(lambda s:s['geometry'].update(active='unexpected'), 3)
        self.reject(lambda s:s['geometry'].update(active='unexpected'), 10)
        self.reject(lambda s:s['geometry']['editor'].update(generation='999'))
        f, r = sample()
        for i in (1, 2, 3, 4, 8, 9, 10): r['stages'][i]['geometry']['active'] = ''
        r['stages'][6]['geometry']['editor']['sameNode'] = False
        self.check(f, r)

    def test_exact_rich_model_native_and_fresh_journal(self):
        for i in (0, 3, 5, 6, 7, 9, 11):
            self.reject(lambda s:s['proof']['journal'].update(blocks=[{}]), i)
            self.reject(lambda s:s['geometry']['editor'].update(blocks=[{}]), i)
            self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(blocks=[{}]), i)
            self.reject(lambda s:s['proof']['journal'].update(revision=999), i)
        self.reject(lambda s:s['geometry']['editor'].update(bold=False))

    def test_seed_terminal_break_bold_opaque_and_plain_b_exact(self):
        for change in (
            lambda a:a[0]['elements'][0]['elements'][-1].update(style=dict(bold=True)),
            lambda a:a[0]['elements'][0]['elements'][-2].update(style={}),
            lambda a:a[0]['elements'][0]['elements'][1]['opaque'].update(retain=[9]),
        ):
            f, r = sample()
            for row in r['stages']: change(row['proof']['originalA'])
            with self.assertRaises(RuntimeError): self.check(f, r)
        f, r = sample()
        for row in r['stages'][7:]: row['proof']['newerB'][0]['elements'][0]['elements'][0]['text'] += '\n'
        with self.assertRaises(RuntimeError): self.check(f, r)

    def test_empty_editor_scaffold_and_paragraphs_not_trimmed(self):
        f, r = sample(); r['stages'][6]['geometry']['editor']['text'] = '\n'; self.check(f, r)
        for text in (' ', '\n\n', '\t', 'lost'):
            self.reject(lambda s:s['geometry']['editor'].update(text=text), 6)
        self.reject(lambda s:s['geometry']['editor'].update(text=s['geometry']['editor']['text'].replace('\n','\n\n')), 7)

    def test_loading_retry_notices_and_controls(self):
        self.reject(lambda s:s['geometry'].update(notices=[]))
        self.reject(lambda s:s['geometry'].update(notices=[]), 1)
        self.reject(lambda s:s['geometry']['controls'][-1].update(disabled=True), 8)

    def test_scalar_types_geometry_and_pngs(self):
        self.reject(lambda s:s['model']['ledger'].update(cancelled=True))
        self.reject(lambda s:s['deletionProbe'].update(posts=True), 6)
        self.reject(lambda s:s['geometry']['controls'][0].update(hit=False))
        self.reject(lambda s:s['geometry'].update(viewport=[float('nan'),380]))
        self.reject(lambda s:s.update(capture='/foreign.png'))
        f, r = sample(); seen = []
        with patch.dict(V['H'], {'png_signature':lambda p,d:seen.append((p,d))}):
            V['validate'](f, r, DIR, dimensions=[1280,780])
        self.assertEqual(seen, [(DIR / f'{f}-{name}.png', [1280,780]) for name in V['STAGES']])


if __name__ == '__main__': unittest.main()
