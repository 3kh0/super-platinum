"""Portable own-edit receipts; each mutation targets an independent custody claim."""
import copy
import json
from pathlib import Path
import runpy
import unittest

V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit.py')))
B = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_file_navigation.py')))
DIRECTORY = Path('/private/own-edit')


def inert_paint(raw):
    return dict(locked=True, parentClass='slack-list-comment-compose-editor',
        lines=[dict(rect=copy.deepcopy(line['rect']), fullVisible=line['fullVisible'],
            fragments=[dict(rect=copy.deepcopy(fragment['rect']), hitParent=True) for fragment in line['fragments']]) for line in raw['lines']],
        visibleLines=sum(line['fullVisible'] for line in raw['lines']), ok=True)


def sample(narrow=False):
    fixture, base = B['sample']('file-navigation', narrow)
    template = base['stages'][0]['model']
    seed = base['stages'][0]['proof']['seed']
    seed['blocks'] = V['G']['seed_blocks']()
    reply = copy.deepcopy(seed['blocks'])
    reply[0]['elements'][0]['elements'][-1] = dict(type='text', text=V['REPLY'])
    cancelled = V['received']()['blocks'][:1]
    cancelled[0]['elements'][-1]['elements'][-1]['text'] += V['CANCEL']
    edited = V['received']()['blocks'][:1]
    edited[0]['elements'][-1]['elements'][-1]['text'] += V['CHANGE']
    outgoing = V['wire'](edited)
    ack = dict(ok=True, channel=V['CHANNEL'], ts=V['TS'], message=V['update_row'](edited))
    inventory = dict(scope=seed['scope'], revision=3, capture='00000000-0000-4000-8000-000000000001', blocks=reply, attempt=None)
    readset = copy.deepcopy(template['ledger']['requests'][:4])
    page = copy.deepcopy(template['comments']['page'])
    page['messages'] = [V['received']() if r.get('ts') == V['TS'] else r for r in page['messages']]
    rows = []
    for index, name in enumerate(V['STAGES']['own-edit']):
        m = copy.deepcopy(template)
        m.update(noSession=False, noTransport=False, networkCalls=0, writes=0)
        count = V['READS'][index]
        m['ledger'] = dict(requests=[dict(copy.deepcopy(q), serial=j + 1) for j, q in enumerate(readset * (count // 4))],
            replies=count // 4, completed=count, cancelled=0, held=False)
        m['composeLedger'] = dict(prepared=[], posts=[], preflights=[], policies=[], messages=[], published=False, held=False, heldPost=False)
        m['reactionLedger'] = dict(prepared=[], issued=[], outcomes=[], completed=0, held=False, networkCalls=0, fullReads=[])
        comments = m['comments']
        comments.update(active=True, record=V['READ']['KNOWN'], root=V['READ']['ROOT_TS'], loading=False, error=None,
            deleted=[], visibleTs=[V['TS']], generation=100 if index < 6 else 200, page=copy.deepcopy(page),
            reactions=[dict(ts=V['TS'], reactions=[dict(name='wave', count=1)])])
        if index >= 5:
            comments['page']['messages'] = [copy.deepcopy(ack['message']) if r.get('ts') == V['TS'] else r for r in comments['page']['messages']]
        comments['pageMessages'] = copy.deepcopy(comments['page']['messages'])
        if index == 4:
            comments['pageMessages'] = [copy.deepcopy(ack['message']) if r.get('ts') == V['TS'] else r for r in comments['pageMessages']]
        draft = m['draftStore']['drafts'][0]
        draft.update(scope=seed['scope'], blocks=copy.deepcopy(reply), capture=inventory['capture'], savedRevision=3,
            attempt=None, error=None, busy=False, saving=False, loading=False, terminalPending=False, captureFailed=False,
            native=True, id='restored' if index == 6 else 'original', generation=40 if index == 6 else 20)
        m['draftStore']['active'] = f"{draft['id']}:{comments['generation']}:{draft['generation']}"
        f = dict(enabled=True, networkCalls=0, httpCapability=False, received=V['received'](), boot=V['boot'](),
            policy=[dict(request=q, response=r) for _ in range(V['POLICIES'][index]) for q, r in V['policy_cycle']()],
            prepared=[copy.deepcopy(outgoing)] if index >= 3 else [], updates=[copy.deepcopy(outgoing)] if index >= 3 else [],
            responses=[dict(disposition='accepted', raw=copy.deepcopy(ack))] if index >= 3 else [],
            futureRows=[copy.deepcopy(ack['message'])] if index >= 3 else [], held=index == 3,
            releases=[V['RELEASE']] if index >= 4 else [], published=index >= 5,
            publishedRow=copy.deepcopy(ack['message']) if index >= 5 else None)
        attempt = dict(id=123, ts=V['TS'], blocks=copy.deepcopy(edited) + V['received']()['blocks'][1:],
            phase='sent' if index == 3 else 'confirmed', receipt=None if index == 3 else copy.deepcopy(ack), retryAt=0)
        active = dict(scope='list-edit:100:77', generation=77, ts=V['TS'], current=True,
            blocks=copy.deepcopy(cancelled if index == 1 else edited), revision=1, busy=index == 3,
            notice='Saving the edited comment\u2026' if index == 3 else None, composing=False,
            source=V['received'](), preserved=V['received']()['blocks'][1:], attempt=123 if index == 3 else None) if index in (1, 3) else None
        edit = dict(opening=False, events=0, active=active, attempts=[attempt] if index >= 3 else [])
        p = dict(source=dict(received=V['received'](), boot=V['boot']()), replySeed=copy.deepcopy(seed), replyBlocks=copy.deepcopy(reply), actions=[])
        if index >= 1:
            p['cancelledBlocks'] = copy.deepcopy(cancelled)
        if index >= 3:
            p.update(editedBlocks=copy.deepcopy(edited), doubleSave=dict(ok=True, disabled=True))
        if index >= 4:
            p['release'] = V['RELEASE']
        if index == 6:
            p['runtimeForgotten'] = True
        editing = index in (1, 3)
        names = ['slack-list-comment-edit-cancel', 'slack-list-comment-edit-save'] if editing else ['slack-list-comments-back', 'slack-list-comment-send']
        g = dict(ok=True, noOverflow=True, viewport=[480, 800] if narrow else [1100, 800], editing=editing,
            controls=[dict(id=id, text=id, visible=True, hit=True, disabled=index == 3, rect=B['rect']()) for id in names],
            text=B['textproof'](), reply=dict(blocks=copy.deepcopy(reply), sameNode=index != 6, visible=not editing,
                hidden=editing, inert=editing, composing=False, rect=B['rect']()),
            edit=dict(blocks=copy.deepcopy(cancelled if index == 1 else edited), editable=index == 1, rect=B['rect']()) if editing else None,
            notice=dict(text='Saving the edited comment\u2026', visible=True, hit=True, rect=B['rect']()) if index == 3 else None,
            row=dict(text=(V['received']()['text'] if index < 4 else ack['message']['text']) + ' Preserved original context', rect=B['rect']()))
        if index == 3:
            g['text'].update(ok=False, visibleLines=0)
            for line in g['text']['lines']:
                line['hit'] = False
                for fragment in line['fragments']:
                    fragment['hit'] = False
            g['inertTextPaint'] = inert_paint(g['text'])
        rows.append(dict(stage=name, model=dict(comments=m, fixture=f, edit=edit), geometry=g, proof=p,
            journal=copy.deepcopy(inventory), capture=str(DIRECTORY / f'{fixture}-{name}.png')))
    return fixture, dict(ok=True, case='own-edit', fixture=fixture, readiness=base['readiness'], stages=rows,
        final=copy.deepcopy(rows[-1]['model']), initialRecords=base['initialRecords'], noLiveNetwork=True)


class OwnEditTests(unittest.TestCase):
    def check(self, fixture, receipt):
        return V['validator']('own-edit')(fixture, receipt, DIRECTORY, png=False)

    def reject(self, mutation, index=3):
        fixture, receipt = sample()
        mutation(receipt['stages'][index])
        receipt['final'] = copy.deepcopy(receipt['stages'][-1]['model'])
        with self.assertRaises((RuntimeError, KeyError, TypeError, ValueError, IndexError)):
            self.check(fixture, receipt)

    def test_both_widths_and_exact_final(self):
        for narrow in (False, True):
            fixture, receipt = sample(narrow)
            self.assertIs(self.check(fixture, receipt), receipt)

    def test_duplicate_update(self):
        self.reject(lambda r: r['model']['fixture']['updates'].append(copy.deepcopy(r['model']['fixture']['updates'][0])))

    def test_plain_fallback_is_not_rich_update(self):
        def change(row):
            for key in ('prepared', 'updates'):
                row['model']['fixture'][key][0]['args']['text'] = 'fallback'
        self.reject(change)

    def test_wrong_channel_or_ts(self):
        for key, value in (('channel', 'COTHER'), ('ts', V['READ']['ROOT_TS'])):
            def change(row):
                for k in ('prepared', 'updates'):
                    row['model']['fixture'][k][0]['args'][key] = value
            self.reject(change)

    def test_context_is_not_editable_or_dropped(self):
        def change(row):
            for key in ('prepared', 'updates'):
                args = row['model']['fixture'][key][0]['args']
                blocks = json.loads(args['blocks'])
                blocks[-1]['edit_context_opaque']['keep'] = False
                args['blocks'] = json.dumps(blocks)
        self.reject(change)

    def test_ack_rewrite_or_wrong_author(self):
        self.reject(lambda r: r['model']['fixture']['responses'][0]['raw']['message'].update(user='UOTHER'), 4)

    def test_http_page_not_replaced_by_ack(self):
        self.reject(lambda r: r['model']['comments']['comments']['page']['messages'].__setitem__(1, copy.deepcopy(r['model']['fixture']['futureRows'][0])), 4)

    def test_no_reply_post(self):
        self.reject(lambda r: r['model']['comments']['composeLedger']['posts'].append({'method': 'chat.postMessage'}))

    def test_reply_journal_unchanged(self):
        self.reject(lambda r: r['journal'].update(revision=99))

    def test_reply_text_opaque_style_survive(self):
        self.reject(lambda r: r['geometry']['reply']['blocks'][0]['elements'][0]['elements'][1]['opaque'].update(retain=[]))

    def test_reply_mount_hidden_and_inert_only_during_edit(self):
        self.reject(lambda r: r['geometry']['reply'].update(hidden=False), 1)
        self.reject(lambda r: r['geometry']['reply'].update(inert=False), 3)
        self.reject(lambda r: r['geometry']['reply'].update(sameNode=False), 2)

    def test_pending_cannot_save_or_edit(self):
        self.reject(lambda r: r['geometry']['controls'][1].update(disabled=False))
        self.reject(lambda r: r['geometry']['edit'].update(editable=True))
        self.reject(lambda r: r['model']['edit']['active'].update(busy=False))

    def test_clipped_or_occluded_editor_rejected(self):
        self.reject(lambda r: r['geometry']['inertTextPaint'].update(ok=False, visibleLines=0))
        self.reject(lambda r: r['geometry']['inertTextPaint']['lines'][0]['fragments'][0].update(hitParent=False))

    def test_cancel_does_not_publish(self):
        self.reject(lambda r: r['geometry']['row'].update(text=r['geometry']['row']['text'] + V['CANCEL']), 2)

    def test_cancelled_text_cannot_become_saved_text(self):
        self.reject(lambda r: r['proof'].update(editedBlocks=copy.deepcopy(r['proof']['cancelledBlocks'])))

    def test_original_reactions_retained(self):
        self.reject(lambda r: r['model']['comments']['comments']['pageMessages'][1].update(reactions=[]), 4)

    def test_policy_missing_assignment_not_false(self):
        self.reject(lambda r: r['model']['fixture']['policy'][2]['response'].update(assignments={}), 1)

    def test_policy_read_not_reused_for_save(self):
        self.reject(lambda r: r['model']['fixture']['policy'].__delitem__(slice(8, 12)))

    def test_foreign_root_or_extra_read(self):
        self.reject(lambda r: r['model']['comments']['ledger']['requests'][3]['args'].update(ts='1781524999.000001'))

    def test_confirmed_is_not_readback_observed(self):
        self.reject(lambda r: r['model']['edit']['attempts'][0].update(phase='observed'), 5)

    def test_real_reopen_required(self):
        self.reject(lambda r: r['proof'].update(runtimeForgotten=False), 6)
        self.reject(lambda r: r['model']['comments']['draftStore']['drafts'][0].update(id='original'), 6)

    def test_raw_response_not_early_publication(self):
        self.reject(lambda r: r['model']['fixture'].update(published=True, publishedRow=r['model']['fixture']['futureRows'][0]))

    def test_wrong_case_and_capture_path(self):
        with self.assertRaises(RuntimeError):
            V['validator']('unrecognized-edit-case')
        self.reject(lambda r: r.update(capture='/somewhere/foreign.png'))


def custody_sample(narrow=False):
    fixture, base = sample(narrow)
    template = base['stages'][0]
    reply = template['proof']['replyBlocks']
    rejected, unknown = V['received']()['blocks'][:1], V['received']()['blocks'][:1]
    rejected[0]['elements'][-1]['elements'][-1]['text'] += V['REJECTED_TEXT']
    unknown[0]['elements'][-1]['elements'][-1]['text'] += V['UNKNOWN_TEXT']
    readset = template['model']['comments']['ledger']['requests']
    def button(id):
        return dict(ok=True, proof=dict(id=id, disabled=False, visible=True, hit=True, rect=B['rect']()))
    rows = []
    for i, name in enumerate(V['STAGES']['own-edit-custody']):
        row = copy.deepcopy(template)
        row.update(stage=name, capture=str(DIRECTORY / f'{fixture}-{name}.png'))
        m, f, edit, p, g = row['model']['comments'], row['model']['fixture'], row['model']['edit'], row['proof'], row['geometry']
        read_count = V['CUSTODY_READS'][i]
        m['ledger'] = dict(requests=[dict(copy.deepcopy(q), serial=j + 1) for j, q in enumerate(readset * (read_count // 4))],
            replies=read_count // 4, completed=read_count, cancelled=0, held=False)
        c = m['comments']
        c['generation'] = 200 if i == 7 else 100
        if i >= 6:
            c['page']['messages'] = [V['update_row'](unknown) if r.get('ts') == V['TS'] else r for r in c['page']['messages']]
            c['pageMessages'] = copy.deepcopy(c['page']['messages'])
        draft = m['draftStore']['drafts'][0]
        draft.update(id='restored' if i == 7 else 'original', generation=40 if i == 7 else 20,
            error=V['COMPOSING_ERROR'] if i == 1 else None)
        m['draftStore']['active'] = f"{draft['id']}:{c['generation']}:{draft['generation']}"
        count = 2 if i >= 4 else 1 if i >= 2 else 0
        f.update(policy=[dict(request=q, response=r) for _ in range(V['CUSTODY_POLICIES'][i]) for q, r in V['policy_cycle']()],
            prepared=[V['wire'](b) for b in (rejected, unknown)[:count]], updates=[V['wire'](b) for b in (rejected, unknown)[:count]],
            responses=[dict(disposition='rejected', raw=copy.deepcopy(V['REJECTED_RAW'])), dict(disposition='unconfirmed', raw=None)][:count],
            futureRows=[V['update_row'](unknown)] if count == 2 else [], held=False, releases=[],
            published=i >= 6, publishedRow=V['update_row'](unknown) if i >= 6 else None)
        edit['attempts'] = [dict(id=123 + j, ts=V['TS'], blocks=copy.deepcopy(b) + V['received']()['blocks'][1:],
            phase='rejected' if j == 0 else 'observed' if i >= 6 else 'unknown', receipt=copy.deepcopy(V['REJECTED_RAW']) if j == 0 else None,
            retryAt=0) for j, b in enumerate((rejected, unknown)[:count])]
        active = i in (2, 4, 5)
        notice = {2: V['REJECTED_NOTICE'], 4: V['UNKNOWN_NOTICE'], 5: V['CHECKED_NOTICE']}.get(i)
        edit['active'] = dict(scope='list-edit:100:77', generation=77, ts=V['TS'], current=True, blocks=copy.deepcopy(rejected if i == 2 else unknown),
            revision=1, busy=False, notice=notice, composing=False, source=V['received'](), preserved=V['received']()['blocks'][1:],
            attempt=123 if i == 2 else 124) if active else None
        if i >= 1:
            p['compositionStart'] = dict(ok=True, active=True, native=dict(blocks=copy.deepcopy(reply), composing=True))
        if i >= 2:
            p.update(compositionEnd=dict(ok=True, active=False, native=dict(blocks=copy.deepcopy(reply), composing=False)),
                replyRecovery=button('slack-list-comment-retry'), rejectedBlocks=copy.deepcopy(rejected))
        if i >= 4:
            p.update(unknownBlocks=copy.deepcopy(unknown), blockedControls=dict(ok=True, saveDisabled=True, cancelDisabled=True))
        p['checks'] = [dict(stage=n, value=button('slack-list-comment-edit-check')) for n in (['unconfirmed-checked', 'observed'] if i >= 6 else ['unconfirmed-checked'] if i == 5 else [])]
        if i == 7:
            p['runtimeForgotten'] = True
        names = ['slack-list-comment-edit-cancel', 'slack-list-comment-edit-save'] if active else ['slack-list-comments-back', 'slack-list-comment-send']
        if i in (4, 5):
            names.append('slack-list-comment-edit-check')
        g.update(editing=active, controls=[dict(id=id, disabled=(i == 1 and id.endswith('-send')) or (i in (4, 5) and not id.endswith('-check')),
            visible=True, hit=True, rect=B['rect']()) for id in names],
            edit=dict(blocks=copy.deepcopy(rejected if i == 2 else unknown), editable=True, rect=B['rect']()) if active else None,
            notice=dict(text=notice, visible=True, hit=True, rect=B['rect']()) if notice else None,
            replyNotice=dict(text=V['COMPOSING_ERROR'], visible=True, hit=True, rect=B['rect']()) if i == 1 else None,
            row=dict(text=(V['received']()['text'] if i < 6 else V['update_row'](unknown)['text']) + ' Preserved original context', rect=B['rect']()))
        g['reply'].update(sameNode=i != 7, visible=not active, hidden=active, inert=active, composing=i == 1)
        rows.append(row)
    base.update(case='own-edit-custody', stages=rows, final=copy.deepcopy(rows[-1]['model']))
    return fixture, base


class CustodyTests(unittest.TestCase):
    def check(self, fixture, receipt):
        return V['validator']('own-edit-custody')(fixture, receipt, DIRECTORY, png=False)

    def reject(self, mutate, index=4):
        f, r = custody_sample()
        mutate(r['stages'][index])
        r['final'] = copy.deepcopy(r['stages'][-1]['model'])
        with self.assertRaises((RuntimeError, KeyError, ValueError, TypeError, IndexError)):
            self.check(f, r)

    def test_both_widths(self):
        for narrow in (False, True):
            f, r = custody_sample(narrow)
            self.assertIs(self.check(f, r), r)

    def test_composition_refused_without_opening_or_requests(self):
        self.reject(lambda r: r['geometry']['reply'].update(composing=False), 1)
        self.reject(lambda r: r['model']['fixture']['updates'].append({}), 1)
        self.reject(lambda r: r['model']['edit'].update(active={}), 1)

    def test_composition_requires_native_event_and_retry(self):
        self.reject(lambda r: r['proof']['compositionStart']['native'].update(composing=False), 1)
        self.reject(lambda r: r['proof']['replyRecovery']['proof'].update(disabled=True), 2)

    def test_rejection_retains_exact_B_and_original_reply(self):
        self.reject(lambda r: r['geometry']['edit']['blocks'][0]['elements'][0]['elements'][0]['style'].update(bold=False), 2)
        self.reject(lambda r: r['model']['edit']['attempts'][0].update(phase='confirmed'), 2)

    def test_cancel_restores_reply_without_publishing_rejected_body(self):
        self.reject(lambda r: r['geometry']['reply'].update(hidden=True), 3)
        self.reject(lambda r: r['geometry']['row'].update(text=r['geometry']['row']['text'] + V['REJECTED_TEXT']), 3)

    def test_unknown_disables_save_cancel_but_allows_check(self):
        for idx in (0, 1):
            self.reject(lambda r: r['geometry']['controls'][idx].update(disabled=False))
        self.reject(lambda r: r['geometry']['controls'][2].update(disabled=True))

    def test_unknown_duplicate_click_cannot_replay(self):
        self.reject(lambda r: r['model']['fixture']['updates'].append(copy.deepcopy(r['model']['fixture']['updates'][-1])))
        self.reject(lambda r: r['proof']['blockedControls'].update(cancelDisabled=False))

    def test_negative_check_preserves_original_mount_and_unknown(self):
        self.reject(lambda r: r['model']['edit']['attempts'][1].update(phase='observed'), 5)
        self.reject(lambda r: r['model']['edit']['active'].update(generation=999), 5)

    def test_observed_is_not_original_ack(self):
        self.reject(lambda r: r['model']['edit']['attempts'][1].update(phase='confirmed'), 6)
        self.reject(lambda r: r['model']['fixture']['responses'][1].update(raw={'ok': True}), 6)
        self.reject(lambda r: r['model']['edit']['attempts'][1].update(receipt={'ok': True}), 6)

    def test_positive_check_requires_explicit_published_readback(self):
        self.reject(lambda r: r['model']['fixture'].update(published=False), 6)
        self.reject(lambda r: r['proof']['checks'].__delitem__(1), 6)
        self.reject(lambda r: r['model']['comments']['comments']['page']['messages'][1].update(user='UOTHER'), 6)

    def test_reply_never_consumed_or_unmounted_during_unknown(self):
        self.reject(lambda r: r['journal'].update(blocks=[]))
        self.reject(lambda r: r['geometry']['reply'].update(sameNode=False))

    def test_result_check_not_silently_another_save_policy_request(self):
        self.reject(lambda r: r['model']['fixture']['policy'].extend(copy.deepcopy(r['model']['fixture']['policy'][:4])), 5)


class InertPaintTests(unittest.TestCase):
    def pair(self):
        _, receipt = sample()
        g = receipt['stages'][3]['geometry']
        return g['text'], g['inertTextPaint'], g['viewport']

    def reject(self, mutate):
        raw, paint, viewport = self.pair()
        mutate(raw, paint)
        with self.assertRaises(RuntimeError):
            V['inert_text'](raw, paint, viewport)

    def test_raw_inert_hits_preserved(self):
        raw, paint, viewport = self.pair()
        original = copy.deepcopy(raw)
        V['inert_text'](raw, paint, viewport)
        self.assertEqual(raw, original)
        self.assertFalse(raw['ok'])
        self.assertEqual(raw['visibleLines'], 0)

    def test_wrong_parent_or_unlocked_refused(self):
        self.reject(lambda r, p: p.update(parentClass='overlay'))
        self.reject(lambda r, p: p.update(locked=False))

    def test_omitted_extra_or_relocated_fragments_refused(self):
        self.reject(lambda r, p: p['lines'][0]['fragments'].clear())
        self.reject(lambda r, p: p['lines'].append(copy.deepcopy(p['lines'][0])))
        self.reject(lambda r, p: p['lines'][0]['fragments'][0]['rect'].update(left=11))

    def test_forged_line_union_refused(self):
        def corrupt(raw, paint):
            for item in (raw['lines'][0], paint['lines'][0]):
                item['rect'].update(right=211, width=201)
        self.reject(corrupt)

    def test_clipping_not_repaired_by_paint_flag(self):
        self.reject(lambda r, p: r['clip'].update(top=15, height=15))

    def test_raw_child_hit_not_silently_rewritten(self):
        self.reject(lambda r, p: r['lines'][0]['fragments'][0].update(hit=True))

    def test_count_and_original_flags_must_match(self):
        self.reject(lambda r, p: p.update(visibleLines=2))
        self.reject(lambda r, p: r.update(ok=True, visibleLines=1))
        self.reject(lambda r, p: p['lines'][0].update(fullVisible=False))

    def test_optional_clipped_second_line_keeps_one_real_visible_line(self):
        raw, paint, viewport = self.pair()
        line = copy.deepcopy(raw['lines'][0])
        line.update(fullVisible=False)
        for rect in (line['rect'], line['fragments'][0]['rect']):
            rect.update(top=1000, bottom=1020)
        raw['lines'].append(line)
        paint['lines'].append(dict(rect=copy.deepcopy(line['rect']), fullVisible=False,
            fragments=[dict(rect=copy.deepcopy(line['fragments'][0]['rect']), hitParent=False)]))
        V['inert_text'](raw, paint, viewport)

    def test_paint_exception_does_not_apply_to_interactive_stage(self):
        fixture, receipt = sample()
        receipt['stages'][1]['geometry']['inertTextPaint'] = copy.deepcopy(receipt['stages'][3]['geometry']['inertTextPaint'])
        with self.assertRaises(RuntimeError):
            V['validate'](fixture, receipt, DIRECTORY, png=False)

    def test_retained_actual_saving_probe_has_false_hits_not_clipped_text(self):
        path = Path(__file__).resolve().parents[1] / 'tmp/list-comment-own-edit-20261019/normal-a/slack-list/slack-list.json'
        if not path.exists():
            self.skipTest('Retained local native failure not present')
        record = json.loads(path.read_text(encoding='utf-8'))['terminalReceipts'][0]
        error = record['error']
        actual, _ = json.JSONDecoder().raw_decode(error.split('List DOM did not settle: ', 1)[1])
        self.assertFalse(actual['ok'])
        self.assertFalse(actual['text']['ok'])
        self.assertFalse(actual['edit']['editable'])
        self.assertEqual(actual['text']['visibleLines'], 0)
        self.assertTrue(all(line['fullVisible'] and not line['hit'] for line in actual['text']['lines']))
        # This unit supplies an explicit synthetic parent-hit witness to exercise
        # geometry arithmetic; it does not certify the failed native screenshot.
        raw = copy.deepcopy(actual['text'])
        V['inert_text'](raw, inert_paint(raw), actual['viewport'])
        self.assertEqual(raw, actual['text'])

    def test_retained_custody_missing_notice_has_useful_strict_failure(self):
        path = Path(__file__).resolve().parents[1] / 'tmp/list-comment-own-edit-20261019/custody-normal-a/slack-list/slack-list.json'
        if not path.exists():
            self.skipTest('Retained local native failure not present')
        receipt = json.loads(path.read_text(encoding='utf-8'))['terminalReceipts'][0]
        with self.assertRaisesRegex(RuntimeError, 'Composition refusal is not visible'):
            V['validate_custody']('slack-list', receipt, Path(receipt['stages'][0]['capture']).parent, png=False)


if __name__ == '__main__':
    unittest.main()
