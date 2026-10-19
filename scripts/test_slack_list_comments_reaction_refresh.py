"""Adversarial reaction/read race receipts, without a native process or Slack IO."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_reaction_refresh.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_reactions_native.py')))
DIR = Path('/private/list-comments-reaction-refresh')


def sample(narrow=False):
    fixture, base = BASE['sample'](narrow)
    original, sent = base['stages'][0], base['stages'][5]
    a, seed = original['proof']['blocks'], original['proof']['seed']
    seed['blocks'][0]['elements'][0]['elements'].append(dict(type='text', text='\n'))
    # Match the existing seed's terminal BR: inserted bold text precedes its LF.
    first_elements = a[0]['elements'][0]['elements']
    first_elements[-1]['text'] = first_elements[-1]['text'][:-1]
    first_elements.append(dict(type='text', text='\n'))
    for source in (original, sent):
        if source is original:
            source['model']['draftStore']['drafts'][0]['blocks'] = copy.deepcopy(a)
            source['proof']['journal']['blocks'] = copy.deepcopy(a)
            source['geometry']['editor']['blocks'] = copy.deepcopy(a)
        else:
            for post in source['model']['composeLedger']['posts'] + source['model']['composeLedger']['prepared']:
                post['args']['blocks'] = json.dumps(a)
            source['model']['composeLedger']['messages'][0]['blocks'] = copy.deepcopy(a)
    b = [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[dict(type='text', text=text)])
         for text in ('Newer draft B \u65e5\U0001f98a', 'Retain this draft during comment refresh.')])]
    first, tail = V['FIRST_TS'], sent['model']['composeLedger']['messages'][0]['ts']
    initial_page = copy.deepcopy(original['model']['comments']['page'])
    initial_page['channel'] = 'CCOMMENT01'
    initial_page['messages'][1]['reactions'] = [dict(name='wave', count=2, users=['U1', 'U2'])]
    updated_page = copy.deepcopy(initial_page)
    updated_page['messages'][1]['reactions'] = [dict(name='wave', count=3, users=['U1', 'U2', 'U_RT_REACTOR'])]
    held_tail = copy.deepcopy(updated_page)
    held_tail['messages'].extend(copy.deepcopy(sent['model']['composeLedger']['messages']))
    final_page = copy.deepcopy(held_tail)
    final_page['messages'][-1]['reactions'] = [dict(name='wave', count=1, users=['U_RT_REACTOR'])]
    rows = []; serial = 1
    for i, name in enumerate(V['STAGES']):
        row = copy.deepcopy(sent if i >= 6 else original)
        row.update(stage=name, capture=str(DIR / f'{fixture}-{name}.png'))
        model, g = row['model'], row['geometry']; comments = model['comments']
        if i in (1, 3, 5, 6, 8, 9, 11): serial += 3
        comments.update(read=serial, loading=i in (1, 2, 8), error=V['ERROR'] if i in (3, 4, 9, 10) else None,
                        page=copy.deepcopy(initial_page if i < 5 else final_page if i == 11 else updated_page))
        requests = [dict(copy.deepcopy(r), serial=4 * batch + j + 1)
                    for batch in range(V['READS'][i] // 4) for j, r in enumerate(original['model']['ledger']['requests'])]
        model['ledger'].update(requests=requests, replies=V['READS'][i] // 4, completed=V['COMPLETED'][i],
                               cancelled=V['CANCELLED'][i], held=i in (1, 2, 3, 8, 9))
        model['composeLedger']['published'] = i >= 7
        draft = model['draftStore']['drafts'][0]
        journal = copy.deepcopy(row['proof']['journal'])
        if i >= 7:
            draft.update(blocks=copy.deepcopy(b), capture='11111111-2222-4333-8444-777777777777', sequence=3, savedRevision=5)
            journal.update(blocks=copy.deepcopy(b), capture=draft['capture'], revision=5)
            g['editor'].update(blocks=copy.deepcopy(b), text='Newer draft B \u65e5\U0001f98a\nRetain this draft during comment refresh.', bold=False)
        row['proof'] = dict(seed=copy.deepcopy(seed), originalA=copy.deepcopy(a), newerB=copy.deepcopy(b) if i >= 7 else None,
            journal=journal, heldResponse=copy.deepcopy(initial_page if i in (1, 2, 3, 4) else held_tail if i in (8, 9, 10) else None),
            release=dict(delivered=False, cancelledBeforeReply=True) if i in (4, 10) else None,
            serverUpdate=dict(ts=first, reactions=copy.deepcopy(updated_page['messages'][1]['reactions']), heldUnchanged=True) if i in (3, 4)
            else dict(ts=tail, reactions=copy.deepcopy(final_page['messages'][-1]['reactions']), heldUnchanged=True) if i in (9, 10) else None)
        counts = [2 if i < 3 else 3] + ([1 if i >= 9 else 0] if i >= 6 else [])
        projected = [dict(ts=ts, reactions=[dict(name='wave', count=count)] if count else [])
                     for ts, count in zip(comments['visibleTs'], counts)]
        comments['reactions'] = copy.deepcopy(projected)
        for dom, projected_row in zip(g['rows'], projected): dom['reactions'] = copy.deepcopy(projected_row['reactions'])
        g['notices'] = [V['ERROR']] if i in (3, 4, 9, 10) else ['Loading comments\u2026'] if comments['loading'] else []
        # A real Refresh may hold focus; deltas/release must preserve it, not enforce autofocusing.
        g['active'] = 'slack-list-comments-refresh' if i in (1, 2, 3, 4, 5, 8, 9, 10, 11) else 'slack-list-comment-draft'
        for c in g['controls']:
            c['disabled'] = comments['loading'] if c['id'] in ('slack-list-comments-next', 'slack-list-comments-refresh') else i == 6 if c['id'] == 'slack-list-comment-send' else False
        row['reactionProbe'].update(ts=tail if i >= 6 else first, count=counts[-1], readRequests=V['READS'][i])
        rows.append(row)
    events = []
    for stage, team, generation, channel, ts, user, changed in (
        ('ignored-held', 'T1', 7, 'CCOMMENT01', first, 'U1', False),
        ('ignored-held', 'T_OTHER', 7, 'CCOMMENT01', first, 'U_RT_REACTOR', False),
        ('ignored-held', 'T1', 6, 'CCOMMENT01', first, 'U_RT_REACTOR', False),
        ('ignored-held', 'T1', 7, 'COTHER', first, 'U_RT_REACTOR', False),
        ('ignored-held', 'T1', 7, 'CCOMMENT01', '1999999999.000001', 'U_RT_REACTOR', False),
        ('updated', 'T1', 7, 'CCOMMENT01', first, 'U_RT_REACTOR', True),
        ('tail-updated', 'T1', 7, 'CCOMMENT01', tail, 'U_RT_REACTOR', True)):
        events.append(dict(stage=stage, team=team, generation=generation, changed=changed,
                           frame=dict(type='reaction_added', user=user, reaction='wave', item=dict(type='message', channel=channel, ts=ts))))
    return fixture, dict(base, case='reaction-refresh', stages=rows, reactionEvents=events, final=copy.deepcopy(rows[-1]['model']))


class ReactionRefreshTests(unittest.TestCase):
    def check(self, fixture, receipt):
        return V['validate'](fixture, receipt, DIR, png=False)

    def reject(self, change, index=3):
        fixture, receipt = sample(); change(receipt['stages'][index])
        with self.assertRaises((RuntimeError, KeyError, TypeError, ValueError)): self.check(fixture, receipt)

    def test_both_widths(self):
        for narrow in (False, True): self.check(*sample(narrow))

    def test_dispatch_and_independent_unicode(self):
        stages, validate = V['C']['case_contract']('reaction-refresh')
        self.assertEqual(stages, V['STAGES']); self.assertEqual(validate.__name__, 'validate')
        self.assertEqual([ord(c) for c in V['B_TEXT'] if ord(c) > 127], [0x65e5, 0x1f98a])

    def test_case_order_ownership_and_final(self):
        for mutate in (lambda r:r.update(case='reactions'), lambda r:r['stages'].pop(),
                       lambda r:r['readiness'].update(pid=False), lambda r:r.update(final={})):
            f, r = sample(); mutate(r)
            with self.assertRaises(RuntimeError): self.check(f, r)

    def test_isolation_and_source(self):
        for key, value in [('account','other'), ('noSession',False), ('noTransport',False), ('writes',1), ('networkCalls',1), ('epoch',99)]:
            self.reject(lambda s:s['model'].update({key:value}))
        self.reject(lambda s:s['model'].update(records={}))
        self.reject(lambda s:s['model']['comments'].update(root='other'))
        self.reject(lambda s:s['model']['comments'].update(hasMore=False))

    def test_unknown_duplicate_events_cannot_cancel(self):
        self.reject(lambda s:s['model']['ledger'].update(cancelled=1, completed=8), 2)
        self.reject(lambda s:s['model']['comments'].update(read=99), 2)
        self.reject(lambda s:s['model']['comments'].update(loading=False), 2)

    def test_changed_reaction_retires_read_without_loading_or_automatic_retry(self):
        for i in (3, 9):
            self.reject(lambda s:s['model']['comments'].update(read=4 if i == 3 else 16), i)
            self.reject(lambda s:s['model']['comments'].update(loading=True), i)
            self.reject(lambda s:s['model']['comments'].update(error=None), i)
            self.reject(lambda s:s['model']['ledger'].update(cancelled=0), i)
            self.reject(lambda s:s['model']['ledger']['requests'].append(copy.deepcopy(s['model']['ledger']['requests'][0])), i)

    def test_actual_transport_release_must_be_cancelled(self):
        for i in (4, 10):
            self.reject(lambda s:s['proof'].update(release=dict(delivered=True, cancelledBeforeReply=False)), i)
            self.reject(lambda s:s['model']['ledger'].update(held=True), i)
            self.reject(lambda s:s['model']['ledger'].update(completed=7), i)

    def test_raw_held_reply_is_old_and_immutable(self):
        self.reject(lambda s:s['proof']['heldResponse']['messages'][1]['reactions'][0].update(count=3), 3)
        self.reject(lambda s:s['proof']['heldResponse']['messages'][-1].update(reactions=[dict(name='wave', count=1)]), 9)
        self.reject(lambda s:s['proof'].update(heldResponse=None), 4)
        self.reject(lambda s:s['proof']['serverUpdate'].update(heldUnchanged=False))

    def test_late_release_does_not_replace_page_or_projection(self):
        self.reject(lambda s:s['model']['comments']['page']['messages'][1].update(text='clobbered'), 4)
        self.reject(lambda s:s['model']['comments']['reactions'][0]['reactions'][0].update(count=2), 4)
        self.reject(lambda s:s['geometry']['rows'][-1]['reactions'][0].update(count=0), 10)

    def test_fresh_reads_keep_new_counts_and_cursor(self):
        for i in (5, 11):
            self.reject(lambda s:s['model']['comments']['page']['messages'][1]['reactions'][0].update(count=2), i)
            self.reject(lambda s:s['model']['comments']['page']['response_metadata'].update(next_cursor='other'), i)
            self.reject(lambda s:s['model']['comments'].update(error=V['ERROR']), i)
        self.reject(lambda s:s['model']['comments']['page']['messages'][-1]['reactions'][0].update(count=0), 11)

    def test_exact_read_factories_and_counts(self):
        for i in range(12): self.reject(lambda s:s['model']['ledger'].update(completed=-1), i)
        self.reject(lambda s:s['model']['ledger']['requests'][3]['args'].update(limit='100'))
        self.reject(lambda s:s['model']['ledger']['requests'][3]['args'].update(cursor='other'))
        self.reject(lambda s:s['model']['ledger'].update(cancelled=True))

    def test_exact_event_routing(self):
        for i in range(7):
            f, r = sample(); r['reactionEvents'][i]['changed'] = not r['reactionEvents'][i]['changed']
            with self.assertRaises(RuntimeError): self.check(f, r)
        f, r = sample(); r['reactionEvents'][0]['frame']['user'] = 'U_RT_REACTOR'
        with self.assertRaises(RuntimeError): self.check(f, r)

    def test_model_dom_probe_independence(self):
        for i in (2, 3, 6, 9, 11):
            self.reject(lambda s:s['reactionProbe'].update(count=99), i)
            self.reject(lambda s:s['geometry']['rows'][0]['reactions'][0].update(count=99), i)
            self.reject(lambda s:s['model']['comments']['reactions'][0]['reactions'][0].update(count=99), i)
        self.reject(lambda s:s['geometry']['rows'][-1]['reactions'][0].update(count=True), 9)
        self.reject(lambda s:s['geometry']['rows'].append(copy.deepcopy(s['geometry']['rows'][0])))

    def test_native_node_generation_focus(self):
        for i in (0, 2, 3, 5, 7, 9, 11): self.reject(lambda s:s['geometry']['editor'].update(sameNode=False), i)
        self.reject(lambda s:s['geometry'].update(active='unexpected'), 3)
        self.reject(lambda s:s['geometry']['editor'].update(generation='999'))
        f, r = sample(); r['stages'][6]['geometry']['editor']['sameNode'] = False; self.check(f, r)

    def test_rich_native_model_and_journal_exact(self):
        for i in (0, 3, 5, 6, 7, 9, 11):
            self.reject(lambda s:s['proof']['journal'].update(blocks=[{}]), i)
            self.reject(lambda s:s['geometry']['editor'].update(blocks=[{}]), i)
            self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(blocks=[{}]), i)
            self.reject(lambda s:s['proof']['journal'].update(revision=999), i)
        self.reject(lambda s:s['geometry']['editor'].update(bold=False))

    def test_proof_literal_cannot_self_attest(self):
        f, r = sample()
        for row in r['stages'][7:]: row['proof']['newerB'][0]['elements'][1]['elements'][0]['text'] = 'Wrong literal'
        with self.assertRaises(RuntimeError): self.check(f, r)

    def test_no_pending_draft_storage_or_attempt(self):
        for key in ('busy','saving','loading','captureFailed','terminalPending'):
            self.reject(lambda s:s['model']['draftStore']['drafts'][0].update({key:True}))
        self.reject(lambda s:s['proof']['journal'].update(attempt={}))

    def test_empty_scaffold_only_sent_and_exact_paragraphs(self):
        f, r = sample(); r['stages'][6]['geometry']['editor']['text'] = '\n'; self.check(f, r)
        for text in (' ', '\n\n', '\t', 'lost'):
            self.reject(lambda s:s['geometry']['editor'].update(text=text), 6)
        self.reject(lambda s:s['geometry']['editor'].update(text=s['geometry']['editor']['text'].replace('\n','\n\n')), 7)

    def test_preserved_native_a_receipt_paragraph_boundaries(self):
        path = Path(__file__).resolve().parents[1] / 'tmp/list-comments-reaction-refresh-20261018/native-a/slack-list/slack-list.json'
        if not path.is_file(): self.skipTest('Preserved offline native receipt is local evidence')
        outer = json.loads(path.read_text(encoding='utf-8')); receipt = outer['terminalReceipts'][0]
        self.assertFalse(outer['ok']); self.assertIn('Original rich content/style changed', outer['error'])
        a = receipt['stages'][0]['proof']['originalA']; seed = receipt['stages'][0]['proof']['seed']['blocks']
        self.assertEqual(seed[0]['elements'][0]['elements'][-1], dict(type='text', text='\n'))
        self.assertEqual(a[0]['elements'][0]['elements'][-1], dict(type='text', text='\n'))
        self.assertIn('\n\n', receipt['stages'][0]['geometry']['editor']['text'])
        self.assertEqual(receipt['stages'][7]['geometry']['editor']['text'], V['B_TEXT'])
        V['validate']('slack-list', receipt, Path(outer['readyDirectory']), png=False)

    def test_terminal_break_style_and_opaque_prefix_remain_exact(self):
        for change in (
            lambda a:a[0]['elements'][0]['elements'][-1].update(style=dict(bold=True)),
            lambda a:a[0]['elements'][0]['elements'][-2].update(style={}),
            lambda a:a[0]['elements'][0]['elements'][0].update(text='Changed context '),
            lambda a:a[0]['elements'][0]['elements'][1]['opaque'].update(retain=[9]),
        ):
            f, r = sample()
            for row in r['stages']: change(row['proof']['originalA'])
            with self.assertRaises(RuntimeError): self.check(f, r)
        f, r = sample()
        for row in r['stages'][7:]: row['proof']['newerB'][0]['elements'][0]['elements'][0]['text'] += '\n'
        with self.assertRaises(RuntimeError): self.check(f, r)

    def test_exact_one_send_positive_receipt_and_publication(self):
        self.reject(lambda s:s['model']['composeLedger']['posts'].clear(), 6)
        self.reject(lambda s:s['model']['composeLedger']['messages'][0].update(user='OTHER'), 6)
        self.reject(lambda s:s['model']['composeLedger'].update(published=True), 6)
        self.reject(lambda s:s['model']['composeLedger'].update(published=False), 7)
        self.reject(lambda s:s['model']['composeLedger']['messages'][0].update(reactions=[dict(name='wave', count=1)]), 9)

    def test_notices_controls_and_pngs(self):
        self.reject(lambda s:s['geometry'].update(notices=[]))
        self.reject(lambda s:s['geometry']['controls'][0].update(hit=False))
        self.reject(lambda s:s['geometry']['controls'][-1].update(disabled=True), 8)
        self.reject(lambda s:s['geometry'].update(viewport=[float('nan'), 380]))
        self.reject(lambda s:s.update(capture='/foreign.png'))
        f, r = sample(); seen = []
        with patch.dict(V['H'], {'png_signature':lambda p,d:seen.append((p,d))}):
            V['validate'](f, r, DIR, dimensions=[1280,780])
        self.assertEqual(seen, [(DIR / f'{f}-{name}.png', [1280,780]) for name in V['STAGES']])


if __name__ == '__main__': unittest.main()
