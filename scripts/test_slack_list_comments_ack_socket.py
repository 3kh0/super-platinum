"""ACK/socket adversaries using pure receipt data; no native or network work."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_ack_socket.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_reaction_refresh.py')))
DIR = Path('/private/list-comments-ack-socket')


def sample(case='ack-socket-add', narrow=False):
    fixture, source = BASE['sample'](narrow)
    initial, sent = source['stages'][0], source['stages'][6]
    seed, a = initial['proof']['seed'], initial['proof']['originalA']
    b = [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[dict(type='text', text=t)])
         for t in ('Newer draft B \u65e5\U0001f98a', 'Retain this draft after the original acknowledgement.')])]
    original = copy.deepcopy(sent['model']['composeLedger']['messages'][0])
    original['text'] = 'Existing context @U_FIXTURE_OPAQUE ReferenceFinal review \u4e2d\U0001f642\n\nSecond line <literal> & *punctuation*.'
    if case == 'ack-socket-remove': original['reactions'] = [dict(name='wave', count=1, users=['U_RT_REACTOR'])]
    post = sent['model']['composeLedger']['posts'][0]
    record = dict(version=1, workspace='T1', user='U0', target=dict(document=initial['model']['initialDocument'],
        record=initial['model']['records'][V['READ']['KNOWN']], channel='CCOMMENT01', root=V['READ']['ROOT_TS']),
        root=V['I']['typed'](initial['model']['comments']['page']['messages'][0]), text=original['text'], blocks=copy.deepcopy(a),
        client_ts=post['args']['ts'], client_msg_id=post['args']['client_msg_id'])
    attempt = dict(record=record, capture=initial['model']['draftStore']['drafts'][0]['capture'], raw=None)
    ack, bi = (1, 2) if case == 'ack-first' else (4, 5)
    rows = []
    for i, name in enumerate(V['STAGES'][case]):
        row = copy.deepcopy(initial); row.pop('reactionProbe'); row.update(stage=name, capture=str(DIR/f'{fixture}-{name}.png'))
        model, g = row['model'], row['geometry']; comments = model['comments']; draft = model['draftStore']['drafts'][0]
        pending, done, newer = 0 < i < ack, i >= ack, i >= bi
        content = b if newer else [] if done else a
        if i:
            model['composeLedger'] = copy.deepcopy(sent['model']['composeLedger'])
            model['composeLedger'].update(messages=[copy.deepcopy(original)], heldPost=pending, published=False)
            model['ledger'].update(requests=[dict(copy.deepcopy(r),serial=4*batch+n+1)
                for batch in range(2) for n,r in enumerate(initial['model']['ledger']['requests'])],
                completed=8,replies=2,cancelled=0,held=False)
        draft.update(generation=21 if done else 20, blocks=copy.deepcopy(content), busy=pending,
            attempt=copy.deepcopy(attempt) if pending else None, savedRevision=5 if newer else 4 if done else 3 if pending else 2,
            capture='11111111-2222-4333-8444-777777777777' if newer else '11111111-2222-4333-8444-666666666666' if done else attempt['capture'],
            sequence=3 if newer else 2 if done else 1, status='Comment sent.' if i == ack else None)
        model['draftStore']['active'] = f"{draft['id']}:{comments['generation']}:{draft['generation']}"
        journal = dict(scope=copy.deepcopy(draft['scope']), revision=draft['savedRevision'], capture=draft['capture'],
                       blocks=copy.deepcopy(content), attempt=copy.deepcopy(draft['attempt']))
        row['proof'] = dict(seed=copy.deepcopy(seed), originalA=copy.deepcopy(a), newerB=copy.deepcopy(b) if newer else None,
            originalMessage=copy.deepcopy(original) if i else None, journal=journal,
            release=dict(delivered=True, cancelledBeforeReply=False) if done and case!='ack-first' else None)
        live = pending and i >= 2; ts = original['ts']
        typed = V['I']['typed'](original)
        if case != 'ack-first' and i >= 3:
            typed['reactions'] = [dict(name='wave', count=1, users=['U_RT_REACTOR'])] if case=='ack-socket-add' else []
        visible = ['1781524801.000001'] + ([ts] if live or done else [])
        projection = [dict(ts='1781524801.000001', reactions=[dict(name='wave', count=2)])]
        if live or done: projection.append(dict(ts=ts, reactions=[dict(name=p['name'], count=p['count']) for p in typed['reactions']]))
        comments.update(read=4 if i else 1, loading=False, error=None, deleted=[], visibleTs=visible, reactions=copy.deepcopy(projection),
            live=[copy.deepcopy(typed)] if live else [], liveTs=[ts] if live else [], accepted=[copy.deepcopy(typed)] if done else [],
            acceptedTs=[ts] if done else [], liveReactions=[ts] if pending and i==3 else [], reveal=[99,ts] if done else None)
        g['rows'] = [dict(copy.deepcopy(p), text='First page comment.' if p['ts']!='1781525001.000001' else original['text']) for p in projection]
        g.update(active='slack-list-comment-draft' if not pending else '', notices=['Comment sent.'] if i==ack else [],
                 reveal=dict(serial='99', ts=ts) if done else None)
        g['editor'].update(visible=True, editable=not pending, blocks=copy.deepcopy(content),
            text='\n' if i==ack else V['R']['visible_text'](content), bold=not done, generation=str(draft['generation']), sameNode=i!=ack)
        for c in g['controls']: c['disabled'] = (pending or i==ack) if c['id']=='slack-list-comment-send' else False
        row['ackProbe'] = dict(ts='1781524801.000001' if i==0 or pending and i==1 else ts, visibleTs=copy.deepcopy(visible),
            liveTs=copy.deepcopy(comments['liveTs']), acceptedTs=copy.deepcopy(comments['acceptedTs']),
            liveReactions=copy.deepcopy(comments['liveReactions']), channel='CCOMMENT01', editorGeneration=str(draft['generation']),
            hasMore=True, readRequests=8 if i else 4, prepared=int(i>0), posts=int(i>0))
        rows.append(row)
    if case == 'ack-first':
        specs = [('socket',original,False,2,2), ('duplicate',dict(copy.deepcopy(original),
            text='Conflicting duplicate must not replace the accepted comment',blocks=[]),False,3,3)]
    else:
        reaction = dict(type='reaction_added' if case=='ack-socket-add' else 'reaction_removed', user='U_RT_REACTOR', reaction='wave',
                        item=dict(type='message',channel='CCOMMENT01',ts=original['ts']))
        specs = [('socket',original,True,1,2),('reacted',reaction,True,2,3),('echo',original,False,5,5)]
    events = []
    for stage,frame,changed,before,after in specs:
        keys = ('read','visibleTs','liveTs','acceptedTs','liveReactions','deleted','reactions','reveal')
        events.append(dict(stage=stage,team='T1',generation=7,frame=copy.deepcopy(frame),parsed=True,changed=changed,
            before={k:copy.deepcopy(rows[before]['model']['comments'][k]) for k in keys},
            after={k:copy.deepcopy(rows[after]['model']['comments'][k]) for k in keys}))
    return fixture, dict(ok=True,case=case,fixture=fixture,noLiveNetwork=True,readiness=source['readiness'],
        initialRecords=source['initialRecords'],stages=rows,ackEvents=events,final=copy.deepcopy(rows[-1]['model']))


class AckSocketTests(unittest.TestCase):
    def check(self, f, r): return V['validate'](f,r,DIR,png=False)
    def reject(self, change, index=3, case='ack-socket-add'):
        f,r=sample(case); change(r['stages'][index])
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError)): self.check(f,r)

    def test_three_cases_both_widths(self):
        for case in V['STAGES']:
            for narrow in (False,True):self.check(*sample(case,narrow))

    def test_dispatch_binds_exact_requested_case(self):
        for case in V['STAGES']:
            stages,validate=V['C']['case_contract'](case)
            self.assertEqual(stages,V['STAGES'][case]); f,r=sample(case);validate(f,r,DIR,png=False)
            for other in V['STAGES']:
                if other!=case:
                    f,r=sample(other)
                    with self.assertRaises(RuntimeError):validate(f,r,DIR,png=False)

    def test_fixed_b_unicode_and_paragraphs(self):
        self.assertEqual([ord(c) for c in V['B_TEXT'] if ord(c)>127],[0x65e5,0x1f98a])
        self.reject(lambda s:s['geometry']['editor'].update(text=s['geometry']['editor']['text'].replace('\n\n','\n')),0)
        self.reject(lambda s:s['geometry']['editor'].update(text='  '),4)
        self.reject(lambda s:s['geometry']['editor'].update(text=s['geometry']['editor']['text'].replace('\n','\n\n')),5)

    def test_readiness_case_stage_final(self):
        for change in (lambda r:r.update(case='incoming'),lambda r:r['stages'].pop(),lambda r:r['readiness'].update(pid=False),lambda r:r.update(final={})):
            f,r=sample();change(r)
            with self.assertRaises(RuntimeError):self.check(f,r)

    def test_owner_raw_sources_root_and_pagination(self):
        for key,value in [('account','foreign'),('epoch',99),('noSession',False),('noTransport',False),('writes',1),('networkCalls',1)]:
            self.reject(lambda s:s['model'].update({key:value}))
        for key,value in [('generation',99),('root','other'),('hasMore',False),('loading',True),('error','unexpected')]:
            self.reject(lambda s:s['model']['comments'].update({key:value}))
        self.reject(lambda s:s['model']['comments']['page']['messages'][0].update(text='Other root'))
        self.reject(lambda s:s['model'].update(records={}))

    def test_no_extra_read_post_policy_or_replay(self):
        for i in range(1,7):
            self.reject(lambda s:s['model']['ledger']['requests'].append({}),i)
            self.reject(lambda s:s['model']['composeLedger']['posts'].append({}),i)
        self.reject(lambda s:s['model']['comments'].update(read=99))
        for key in ('messages','policies','preflights','prepared'):
            self.reject(lambda s:s['model']['composeLedger'][key].append({}))
        self.reject(lambda s:s['model']['composeLedger'].update(published=True))

    def test_held_post_and_original_pending_journal(self):
        for i in (1,2,3):
            self.reject(lambda s:s['proof']['journal'].update(attempt=None),i)
            self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(busy=False),i)
            self.reject(lambda s:s['geometry']['editor'].update(editable=True),i)
            self.reject(lambda s:s['model']['composeLedger'].update(heldPost=False),i)
        for key,value in [('blocks',[]),('client_msg_id','other'),('client_ts','other'),('workspace','other'),('user','other'),('text','other')]:
            self.reject(lambda s:s['proof']['journal']['attempt']['record'].update({key:value}))
        self.reject(lambda s:s['proof']['journal']['attempt'].update(raw={'ok':True}))
        self.reject(lambda s:s['proof']['journal']['attempt']['record']['target'].update(record={}))

    def test_raw_ack_immutable_and_remove_initial_wave_pinned(self):
        for case in ('ack-socket-add','ack-socket-remove'):
            for i in range(1,7):
                self.reject(lambda s:s['proof']['originalMessage'].update(text='mutated'),i,case)
                self.reject(lambda s:s['model']['composeLedger']['messages'][0].update(reactions=[] if case.endswith('remove') else [dict(name='wave',count=1,users=['U_RT_REACTOR'])]),i,case)
        f,r=sample('ack-socket-remove')
        for row in r['stages'][1:]:
            row['proof']['originalMessage'].pop('reactions');row['model']['composeLedger']['messages'][0].pop('reactions')
        with self.assertRaises(RuntimeError):self.check(f,r)

    def test_add_and_explicit_empty_remove_survive_actual_ack(self):
        for case,reactions in [('ack-socket-add',[]),('ack-socket-remove',[dict(name='wave',count=1,users=['U_RT_REACTOR'])])]:
            self.reject(lambda s:s['model']['comments']['live'][0].update(reactions=reactions),3,case)
            for i in (4,5,6):self.reject(lambda s:s['model']['comments']['accepted'][0].update(reactions=reactions),i,case)
            self.reject(lambda s:s['model']['comments'].update(liveReactions=[]),3,case)
            self.reject(lambda s:s['model']['comments'].update(liveReactions=['1781525001.000001']),4,case)

    def test_socket_not_ack_and_no_live_accepted_duplicate(self):
        self.reject(lambda s:s['model']['comments'].update(acceptedTs=['1781525001.000001']),2)
        self.reject(lambda s:s['model']['comments']['accepted'].append(copy.deepcopy(s['model']['comments']['live'][0])),2)
        self.reject(lambda s:s['model']['comments']['live'].append(copy.deepcopy(s['model']['comments']['accepted'][0])),4)
        self.reject(lambda s:s['model']['comments']['visibleTs'].append('1781525001.000001'),4)
        self.reject(lambda s:s['geometry']['rows'].append(copy.deepcopy(s['geometry']['rows'][-1])),4)

    def test_actual_release_and_one_reveal(self):
        for i in (4,5,6):
            self.reject(lambda s:s['proof']['release'].update(delivered=False,cancelledBeforeReply=True),i)
            self.reject(lambda s:s['model']['comments']['reveal'].__setitem__(0,100),i)
        self.reject(lambda s:s['proof']['release'].update(delivered=1),4)
        self.reject(lambda s:s['model']['comments'].update(reveal=[99,'1781525001.000001']),2)
        self.reject(lambda s:s['geometry']['reveal'].update(ts='other'),4)

    def test_exact_post_uuid_payload_and_original_source(self):
        for key,value in [('thread_ts','other'),('client_msg_id','wrong'),('channel','other'),('blocks','[]')]:
            self.reject(lambda s:s['model']['composeLedger']['posts'][0]['args'].update({key:value}),1)
        self.reject(lambda s:s['proof']['originalMessage'].update(user='other'),1)

    def test_exact_a_and_b_all_three_custody_copies(self):
        for i in range(7):
            if i==4:continue
            self.reject(lambda s:s['proof']['journal'].update(blocks=[]),i)
            self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(blocks=[]),i)
            self.reject(lambda s:s['geometry']['editor'].update(blocks=[]),i)
        self.reject(lambda s:s['proof']['journal'].update(revision=999),6)
        self.reject(lambda s:s['proof']['journal'].update(capture='11111111-2222-4333-8444-999999999999'),6)
        self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(sequence=99),6)

    def test_style_opaque_paragraph_not_self_attested(self):
        f,r=sample()
        for row in r['stages']:row['proof']['originalA'][0]['elements'][0]['elements'][-2]['style']={}
        with self.assertRaises(RuntimeError):self.check(f,r)
        f,r=sample()
        for row in r['stages'][5:]:row['proof']['newerB'][0]['elements'][0]['elements'][0]['style']={'bold':True}
        with self.assertRaises(RuntimeError):self.check(f,r)
        self.reject(lambda s:s['geometry']['editor'].update(bold=False),2)

    def test_native_generation_node_and_focus_only_within_groups(self):
        for i in range(7):self.reject(lambda s:s['geometry']['editor'].update(sameNode=not s['geometry']['editor']['sameNode']),i)
        for i in (2,3,5,6):self.reject(lambda s:s['geometry'].update(active='unexpected'),i)
        self.reject(lambda s:s['geometry']['editor'].update(generation='999'),5)
        self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(generation=22),6)

    def test_events_identity_changed_and_full_before_after(self):
        for case in V['STAGES']:
            f,r=sample(case)
            for i in range(len(r['ackEvents'])):
                for key,value in [('changed',not r['ackEvents'][i]['changed']),('parsed',False),('generation',6),('team','other')]:
                    bad=copy.deepcopy(r);bad['ackEvents'][i][key]=value
                    with self.assertRaises(RuntimeError):self.check(f,bad)
                bad=copy.deepcopy(r);bad['ackEvents'][i]['after']['read']+=1
                with self.assertRaises(RuntimeError):self.check(f,bad)

    def test_dom_reactions_body_probe_and_admission(self):
        self.reject(lambda s:s['geometry']['rows'][-1].update(reactions=[]))
        self.reject(lambda s:s['geometry']['rows'][-1].update(text='wrong body'))
        self.reject(lambda s:s['ackProbe'].update(posts=False))
        self.reject(lambda s:s['geometry']['controls'][-1].update(disabled=False))
        self.reject(lambda s:s['geometry']['controls'][-1].update(disabled=True),5)
        self.reject(lambda s:s['geometry'].update(notices=[]),4)
        self.reject(lambda s:s['geometry'].update(viewport=[float('nan'),380]))

    def test_capture_binding_and_png_validation(self):
        self.reject(lambda s:s.update(capture='/foreign.png'))
        f,r=sample();seen=[]
        with patch.dict(V['H'],{'png_signature':lambda p,d:seen.append((p,d))}):V['validate'](f,r,DIR,dimensions=[1280,780])
        self.assertEqual(seen,[(DIR/f'{f}-{name}.png',[1280,780]) for name in V['STAGES'][r['case']]])


if __name__=='__main__':unittest.main()
