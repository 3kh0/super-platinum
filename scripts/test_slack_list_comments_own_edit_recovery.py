"""Portable original-node recovery receipts and independent adversarial mutations."""
import copy
import json
import runpy
import unittest
from pathlib import Path

V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit_recovery.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_edit.py')))
O, B = V['O'], BASE['B']
DIRECTORY = Path('/private/own-edit-recovery')


def sample(narrow=False):
    fixture, receipt = BASE['sample'](narrow)
    template = receipt['stages'][0]
    first, unknown = O['received']()['blocks'][:1], O['received']()['blocks'][:1]
    first[0]['elements'][-1]['elements'][-1]['text'] += V['RECOVERY']
    unknown[0]['elements'][-1]['elements'][-1]['text'] += V['UNKNOWN']
    remembered = dict(ok=True, scope='list-edit:100:77', mount='edit-mount-1', generation='77', blocks=first)
    second = dict(ok=True, scope='list-edit:100:88', mount='edit-mount-2', generation='88', blocks=unknown)
    readset = template['model']['comments']['ledger']['requests']
    def button(id):
        return dict(ok=True, proof=dict(id=id, disabled=False, visible=True, hit=True, rect=B['rect']()))
    rows = []
    for i, stage in enumerate(V['STAGES']):
        row = copy.deepcopy(template)
        row.update(stage=stage, capture=str(DIRECTORY / f'{fixture}-{stage}.png'))
        model, p, g = row['model'], row['proof'], row['geometry']
        m, f, edit = (model[k] for k in ('comments', 'fixture', 'edit'))
        n = V['READS'][i]
        m['ledger'] = dict(requests=[dict(copy.deepcopy(q), serial=j + 1) for j, q in enumerate(readset * (n // 4))],
            replies=n // 4, completed=n, cancelled=0, held=False)
        c = m['comments']
        c['generation'] = 200 if i == 11 else 100
        expected = V['changed_row']() if i in (4, 5) else O['update_row'](unknown) if i >= 10 else O['received']()
        c['pageMessages'] = [copy.deepcopy(expected) if r.get('ts') == O['TS'] else r for r in c['pageMessages']]
        if i >= 10:
            c['page']['messages'] = [copy.deepcopy(expected) if r.get('ts') == O['TS'] else r for r in c['page']['messages']]
        draft = m['draftStore']['drafts'][0]
        draft.update(id='restored' if i == 11 else 'original', generation=40 if i == 11 else 20)
        m['draftStore']['active'] = f"{draft['id']}:{c['generation']}:{draft['generation']}"
        f.update(policy=[dict(request=q, response=r) for _ in range(V['POLICIES'][i] // 4) for q, r in O['policy_cycle']()],
            prepared=[O['wire'](unknown)] if i >= 6 else [], updates=[O['wire'](unknown)] if i >= 6 else [],
            responses=[dict(disposition='unconfirmed', raw=None)] if i >= 6 else [],
            futureRows=[O['update_row'](unknown)] if i >= 6 else [], held=False, releases=[], published=i >= 10,
            publishedRow=O['update_row'](unknown) if i >= 10 else None)
        edit.update(opening=False, events=int(i >= 4), retired=[], attempts=[dict(id=123, ts=O['TS'],
            blocks=copy.deepcopy(unknown) + O['received']()['blocks'][1:], phase='observed' if i >= 10 else 'unknown',
            receipt=None, retryAt=0)] if i >= 6 else [], active=None)
        model['capture'] = None
        ref = remembered if i < 6 else second
        active = i in V['ACTIVE']
        if active:
            edit['active'] = dict(scope=ref['scope'], generation=int(ref['generation']), ts=O['TS'], current=i != 4,
                blocks=copy.deepcopy(ref['blocks']), revision=2, busy=False, composing=False,
                nativeReady=i not in (2, 7), captureFailed=i in (2, 7), notice=V['notice'](i, model=True),
                source=O['received'](), preserved=O['received']()['blocks'][1:], attempt=123 if i >= 6 else None)
            model['capture'] = dict(scope=ref['scope'], generation=int(ref['generation']), mount=ref['mount'],
                failed=i in (2, 7), canSave=i in (1, 3), canCancel=i in (1, 3, 4), canCheck=i in (6, 8, 9),
                canRetry=i in (2, 7), busy=False)
        if i >= 1:
            p.update(recoveryBlocks=copy.deepcopy(first), firstEdit=copy.deepcopy(remembered))
        if i >= 2:
            p['firstFailure'] = dict(copy.deepcopy(remembered), armed=True)
        if i >= 3:
            p['recoveries'] = [dict(stage='capture-recovered', value=button('slack-list-comment-edit-retry'))]
        if i >= 4:
            p['staleSave'] = dict(ok=True, disabled=True)
        if i >= 6:
            p.update(unknownBlocks=copy.deepcopy(unknown), unknownEdit=copy.deepcopy(second),
                refreshAfterStale=button('slack-list-comments-refresh'), blockedControls=dict(ok=True, saveDisabled=True, cancelDisabled=True))
        if i >= 7:
            p['unknownFailure'] = dict(copy.deepcopy(second), armed=True)
        if i >= 8:
            p['recoveries'].append(dict(stage='unconfirmed-recovered', value=button('slack-list-comment-edit-retry')))
        if i >= 9:
            p['checks'] = [dict(stage='unconfirmed-checked', value=button('slack-list-comment-edit-check'))]
        if i >= 10:
            p['checks'].append(dict(stage='observed', value=button('slack-list-comment-edit-check')))
        if i == 11:
            p['runtimeForgotten'] = True
        g.update(editing=active, controls=[dict(id=id, text=id, disabled=disabled, visible=True, hit=True, rect=B['rect']())
            for id, disabled in V['controls'](i)], text=B['textproof'](), inertTextPaint=None, replyNotice=None,
            edit=dict(blocks=copy.deepcopy(ref['blocks']), sameNode=True, editable=True, rect=B['rect']()) if active else None,
            notice=dict(text=V['notice'](i), visible=True, hit=True, rect=B['rect']()) if V['notice'](i) else None,
            row=dict(text=expected['text'] + ' Second editable line. Preserved original context', rect=B['rect']()),
            captureFault=None if i in (0, 1, 6) else dict(scope=ref['scope'], mount=ref['mount'], generation=ref['generation'],
                count=1, armed=False, originalNode=active, apiRestored=True))
        g['reply'].update(sameNode=i != 11, hidden=active, inert=active, visible=not active, composing=False)
        rows.append(row)
    event = dict(team='T1', generation=0, frame=dict(type='message', subtype='message_changed', channel=O['CHANNEL'], message=V['changed_row']()),
        parsed=True, witness=dict(kind='Changed', outerChannel=O['CHANNEL'], typedTs=O['TS'], typedChannel=O['CHANNEL'], raw=V['changed_row']()),
        before={k: copy.deepcopy(rows[3]['model'][k]) for k in ('comments', 'edit')},
        after={k: copy.deepcopy(rows[4]['model'][k]) for k in ('comments', 'edit')})
    for row in rows[4:]:
        row['proof']['sourceChanged'] = copy.deepcopy(event)
    receipt.update(case='own-edit-recovery', stages=rows, final=copy.deepcopy(rows[-1]['model']))
    return fixture, receipt


class RecoveryTests(unittest.TestCase):
    def check(self, fixture, receipt):
        return V['validate'](fixture, receipt, DIRECTORY, png=False)

    def reject(self, mutation, index=7):
        fixture, receipt = sample()
        mutation(receipt['stages'][index])
        receipt['final'] = copy.deepcopy(receipt['stages'][-1]['model'])
        with self.assertRaises((RuntimeError, ValueError, KeyError, TypeError, IndexError)):
            self.check(fixture, receipt)

    def test_complete_both_widths(self):
        for narrow in (False, True):
            fixture, receipt = sample(narrow)
            self.assertIs(self.check(fixture, receipt), receipt)

    def test_native_generation_is_canonical_string(self):
        self.reject(lambda r: r['proof']['unknownEdit'].update(generation=88), 6)
        self.reject(lambda r: r['proof']['firstEdit'].update(generation='077'), 1)

    def test_recovery_same_node_and_mount(self):
        self.reject(lambda r: r['geometry']['edit'].update(sameNode=False), 8)
        self.reject(lambda r: r['model']['capture'].update(mount='replacement'), 8)

    def test_capture_failure_cannot_save_cancel_or_check(self):
        for key in ('canSave', 'canCancel', 'canCheck'):
            self.reject(lambda r, key=key: r['model']['capture'].update({key: True}))
        self.reject(lambda r: r['geometry']['controls'][1].update(disabled=False))

    def test_fault_must_restore_api_and_fire_exactly_once(self):
        for key, value in (('count', 0), ('count', 2), ('armed', True), ('apiRestored', False), ('originalNode', False)):
            self.reject(lambda r, key=key, value=value: r['geometry']['captureFault'].update({key: value}))

    def test_fault_not_falsely_bound_after_cancel(self):
        self.reject(lambda r: r['geometry']['captureFault'].update(originalNode=True), 5)

    def test_retry_does_not_clear_unknown(self):
        self.reject(lambda r: r['model']['edit']['attempts'][0].update(phase='confirmed'), 8)
        self.reject(lambda r: r['model']['capture'].update(canSave=True), 8)

    def test_retry_button_evidence_required(self):
        self.reject(lambda r: r['proof']['recoveries'][-1]['value']['proof'].update(disabled=True), 8)

    def test_snapshot_failure_buffer_style_preserved(self):
        self.reject(lambda r: r['geometry']['edit']['blocks'][0]['elements'][0]['elements'][0]['style'].update(bold=False))
        self.reject(lambda r: r['model']['edit']['active']['blocks'][0]['elements'][1]['elements'][0].update(text='lost'))

    def test_reply_journal_and_original_opaque_survive(self):
        self.reject(lambda r: r['journal'].update(capture='00000000-0000-4000-8000-000000000002'))
        self.reject(lambda r: r['geometry']['reply']['blocks'][0]['elements'][0]['elements'][1]['opaque'].update(retain=[]))

    def test_reply_not_exposed_while_edit_active(self):
        self.reject(lambda r: r['geometry']['reply'].update(hidden=False), 8)
        self.reject(lambda r: r['geometry']['reply'].update(inert=False), 8)

    def test_stale_source_provenance_and_disabled_save(self):
        self.reject(lambda r: r['proof']['sourceChanged']['frame'].update(channel='COTHER'), 4)
        self.reject(lambda r: r['proof']['sourceChanged']['witness'].update(kind='Replied'), 4)
        self.reject(lambda r: r['model']['capture'].update(canSave=True), 4)
        self.reject(lambda r: r['proof']['staleSave'].update(disabled=False), 4)

    def test_socket_does_not_replace_http_page(self):
        self.reject(lambda r: r['model']['comments']['comments']['page']['messages'].__setitem__(1, V['changed_row']()), 4)

    def test_event_must_not_mutate_reply_or_edit(self):
        self.reject(lambda r: r['proof']['sourceChanged']['after']['comments']['draftStore']['drafts'][0].update(blocks=[]), 4)

    def test_no_duplicate_update_or_other_endpoint(self):
        self.reject(lambda r: r['model']['fixture']['updates'].append(copy.deepcopy(r['model']['fixture']['updates'][0])), 8)
        self.reject(lambda r: r['model']['fixture']['prepared'][0].update(path='/api/chat.delete'), 8)
        self.reject(lambda r: r['model']['comments']['composeLedger']['posts'].append({}), 8)

    def test_original_update_preserves_context(self):
        def mutate(row):
            for key in ('prepared', 'updates'):
                args = row['model']['fixture'][key][0]['args']
                blocks = json.loads(args['blocks'])
                blocks.pop()
                args['blocks'] = json.dumps(blocks)
        self.reject(mutate, 6)

    def test_readback_is_observed_not_ack(self):
        self.reject(lambda r: r['model']['edit']['attempts'][0].update(phase='confirmed'), 10)
        self.reject(lambda r: r['model']['fixture']['responses'][0].update(raw={'ok': True}), 10)
        self.reject(lambda r: r['model']['fixture'].update(published=True), 9)

    def test_negative_check_retains_unknown_exact_node(self):
        self.reject(lambda r: r['model']['edit']['attempts'][0].update(phase='observed'), 9)
        self.reject(lambda r: r['geometry']['edit'].update(sameNode=False), 9)

    def test_no_unobserved_check_or_read(self):
        self.reject(lambda r: r['proof'].update(checks=[]), 9)
        self.reject(lambda r: r['model']['comments']['ledger'].update(completed=28), 9)

    def test_fresh_policy_is_required_only_actual_open_and_save(self):
        self.reject(lambda r: r['model']['fixture']['policy'].pop(), 6)
        self.reject(lambda r: r['model']['fixture']['policy'][2]['response'].update(assignments={}), 1)

    def test_visible_guidance_and_text_required(self):
        self.reject(lambda r: r['geometry'].update(notice=None))
        self.reject(lambda r: r['geometry']['text'].update(ok=False, visibleLines=0))
        self.reject(lambda r: r['geometry']['controls'][-1].update(hit=False))

    def test_uncertain_unsent_text_not_in_transcript(self):
        self.reject(lambda r: r['geometry']['row'].update(text=r['geometry']['row']['text'] + V['UNKNOWN']), 9)

    def test_reopen_requires_real_forget_and_new_binding(self):
        self.reject(lambda r: r['proof'].update(runtimeForgotten=False), 11)
        self.reject(lambda r: r['geometry']['reply'].update(sameNode=True), 11)
        self.reject(lambda r: r['model']['comments']['draftStore']['drafts'][0].update(id='original'), 11)

    def test_foreign_capture_and_incomplete_terminal(self):
        self.reject(lambda r: r.update(capture='/foreign.png'))
        fixture, receipt = sample()
        receipt['stages'].pop()
        with self.assertRaises(RuntimeError):
            self.check(fixture, receipt)


if __name__ == '__main__':
    unittest.main()
