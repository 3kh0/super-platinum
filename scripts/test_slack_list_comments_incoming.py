"""Incoming-reply receipt adversaries; no native process or Slack traffic."""
import copy
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_incoming.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_deletions.py')))
DIR = Path('/private/list-comments-incoming')


def raw_message(ordinal):
    text = 'Live comment one \u4e2d\U0001f642' if ordinal == 1 else 'Live comment two \u65e5\U0001f98a'
    return dict(type='message', channel='CCOMMENT01', ts=f'178152480{ordinal + 3}.000001',
                thread_ts=V['READ']['ROOT_TS'], user='U1', text=text, opaque=dict(source='incoming', ordinal=ordinal),
                blocks=[dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
                    dict(type='text', text=text, style={'bold':True} if ordinal == 1 else {'italic':True})])])])


def sample(narrow=False):
    fixture, base = BASE['sample'](narrow); initial = base['stages'][0]
    page = initial['model']['comments']['page']; first, second = raw_message(1), raw_message(2)
    readback = copy.deepcopy(page); readback['messages'].extend([copy.deepcopy(first), copy.deepcopy(second)])
    rows = []; serial = 1
    for i, name in enumerate(V['STAGES']):
        row = copy.deepcopy(initial); model, g, proof = row['model'], row['geometry'], row['proof']
        row.update(stage=name, capture=str(DIR / f'{fixture}-{name}.png')); row.pop('deletionProbe')
        proof.pop('newerB')
        proof.update(heldResponse=copy.deepcopy(page) if i in (4,5,6) else None,
            release=dict(delivered=False,cancelledBeforeReply=True) if i == 6 else None,
            serverResponse=copy.deepcopy(readback if i >= 10 else page))
        if i in (4,5,7,10): serial += 3
        visible = ['1781524801.000001'] + ([first['ts']] if i >= 1 else []) + ([second['ts']] if 5 <= i < 9 else [])
        live_raw = [] if i == 0 or i >= 10 else [first,second] if 5 <= i < 9 else [first]
        live = []
        for raw in live_raw:
            typed = dict.fromkeys(('user','bot_id','username','bot_profile','icons','type','subtype','ts','client_msg_id',
                'text','team','channel','thread_ts','parent_user_id','reply_count','reply_users_count','latest_reply','edited','message'))
            typed.update({k:[] for k in ('reply_users','reactions','blocks','files','attachments')})
            typed.update(copy.deepcopy(raw))
            if raw['ts'] == second['ts'] and i == 8: typed['reactions'] = [dict(name='wave',count=1,users=['U_RT_REACTOR'])]
            live.append(typed)
        projection = [dict(ts=ts,reactions=[dict(name='wave',count=2 if ts=='1781524801.000001' else 1)]
                      if ts=='1781524801.000001' or ts==second['ts'] and i==8 else []) for ts in visible]
        comments = model['comments']
        comments.update(read=serial,loading=i==4,error=V['ERROR'] if i in (5,6) else None,
            page=copy.deepcopy(readback if i>=10 else page),deleted=[second['ts']] if i>=9 else [],
            live=live,liveTs=[r['ts'] for r in live],visibleTs=visible,reactions=copy.deepcopy(projection))
        requests = [dict(copy.deepcopy(r),serial=4*batch+n+1) for batch in range(V['READS'][i]//4)
                    for n,r in enumerate(initial['model']['ledger']['requests'])]
        model['ledger'].update(requests=requests,replies=V['READS'][i]//4,completed=V['COMPLETED'][i],
            cancelled=V['CANCELLED'][i],held=i in (4,5))
        g['rows'] = [dict(copy.deepcopy(p),text='First page comment.' if p['ts']=='1781524801.000001'
                    else first['text'] if p['ts']==first['ts'] else second['text']) for p in projection]
        g['notices'] = ['Loading comments\u2026'] if i==4 else [V['ERROR']] if i in (5,6) else []
        g['active'] = 'slack-list-comment-draft' if i<4 else ''
        for control in g['controls']:
            control['disabled'] = i==4 and control['id'] in ('slack-list-comments-next','slack-list-comments-refresh')
        row['incomingProbe'] = dict(ts='1781524801.000001' if i==0 else second['ts'] if 5<=i<=8 else first['ts'],
            visibleTs=copy.deepcopy(visible),liveTs=copy.deepcopy(comments['liveTs']),channel='CCOMMENT01',
            editorGeneration=str(model['draftStore']['drafts'][0]['generation']),hasMore=True,
            readRequests=V['READS'][i],prepared=0,posts=0)
        rows.append(row)
    conflict = dict(copy.deepcopy(first),text='Conflicting duplicate must be ignored',blocks=[])
    foreign = dict(copy.deepcopy(first),ts='1781524806.000001')
    specs = [('arrived','T1',7,first,True,0,1),('duplicate','T1',7,first,False,1,1),
        ('duplicate','T1',7,conflict,False,1,1),('foreign','T_OTHER',7,foreign,False,2,2),
        ('foreign','T1',6,foreign,False,2,2),('foreign','T1',7,dict(copy.deepcopy(foreign),channel='COTHER'),False,2,2),
        ('foreign','T1',7,dict(copy.deepcopy(foreign),thread_ts='1781524999.000001'),False,2,2),
        ('foreign','T1',7,dict(copy.deepcopy(foreign),ts='not-a-timestamp'),False,2,2),
        ('foreign','T1',7,dict(copy.deepcopy(foreign),_hidden_stub=True),False,2,2),
        ('arrived-held','T1',7,second,True,4,5),
        ('reacted','T1',7,dict(type='reaction_added',user='U_RT_REACTOR',reaction='wave',item=dict(type='message',channel='CCOMMENT01',ts=second['ts'])),True,7,8),
        ('deleted','T1',7,dict(type='message',subtype='message_deleted',channel='CCOMMENT01',deleted_ts=second['ts'],event_ts='1781525900.000001'),True,8,9),
        ('final-duplicate','T1',7,first,False,10,10)]
    events=[]
    for stage,team,generation,frame,changed,before,after in specs:
        keys=('read','visibleTs','liveTs','deleted','reactions')
        events.append(dict(stage=stage,team=team,generation=generation,frame=frame,changed=changed,parsed=True,
            before={k:copy.deepcopy(rows[before]['model']['comments'][k]) for k in keys},
            after={k:copy.deepcopy(rows[after]['model']['comments'][k]) for k in keys}))
    return fixture,dict(ok=True,case='incoming',fixture=fixture,noLiveNetwork=True,readiness=base['readiness'],
        initialRecords=base['initialRecords'],stages=rows,incomingEvents=events,final=copy.deepcopy(rows[-1]['model']))


class IncomingTests(unittest.TestCase):
    def check(self,f,r): return V['validate'](f,r,DIR,png=False)
    def reject(self,change,index=1):
        f,r=sample();change(r['stages'][index])
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError)):self.check(f,r)

    def test_both_widths(self):
        for narrow in (False,True):self.check(*sample(narrow))

    def test_dispatch_and_fixed_unicode(self):
        stages,validator=V['C']['case_contract']('incoming')
        self.assertEqual(stages,V['STAGES']);self.assertEqual(validator.__name__,'validate')
        self.assertEqual([ord(c) for c in V['incoming'](1)['text'] if ord(c)>127],[0x4e2d,0x1f642])
        self.assertEqual([ord(c) for c in V['incoming'](2)['text'] if ord(c)>127],[0x65e5,0x1f98a])

    def test_case_order_ready_final(self):
        for change in (lambda r:r.update(case='deletions'),lambda r:r['stages'].pop(),
                       lambda r:r['readiness'].update(pid=False),lambda r:r.update(final={})):
            f,r=sample();change(r)
            with self.assertRaises(RuntimeError):self.check(f,r)

    def test_owner_isolation_canonical_source(self):
        for key,value in [('account','other'),('noSession',False),('noTransport',False),('writes',1),('networkCalls',1),('epoch',99)]:
            self.reject(lambda s:s['model'].update({key:value}))
        self.reject(lambda s:s['model'].update(records={}))
        self.reject(lambda s:s['model']['comments'].update(root='other'))
        self.reject(lambda s:s['model']['comments']['page']['messages'][0].update(text='Wrong root'))

    def test_live_message_exact_body_styles_opaque_and_order(self):
        for key,value in [('text','conflict'),('blocks',[]),('user','OTHER'),('thread_ts','other'),('opaque',{}),('files',[{}])]:
            self.reject(lambda s:s['model']['comments']['live'][0].update({key:value}))
        self.reject(lambda s:s['model']['comments']['live'].reverse(),5)
        self.reject(lambda s:s['model']['comments']['liveTs'].reverse(),5)
        self.reject(lambda s:s['model']['comments']['live'][1]['blocks'][0]['elements'][0]['elements'][0].update(style={'bold':True}),5)

    def test_duplicate_conflict_foreign_do_not_mutate_or_read(self):
        for i in (2,3,11):
            self.reject(lambda s:s['model']['comments'].update(read=999),i)
            self.reject(lambda s:s['model']['ledger']['requests'].append({}),i)
        self.reject(lambda s:s['model']['comments']['live'][0].update(text='Conflicting duplicate must be ignored'),2)

    def test_model_dom_probe_independent(self):
        for i in (1,5,7,8,10,11):
            self.reject(lambda s:s['model']['comments']['visibleTs'].pop(),i)
            self.reject(lambda s:s['geometry']['rows'].pop(),i)
            self.reject(lambda s:s['incomingProbe']['visibleTs'].pop(),i)
        self.reject(lambda s:s['geometry']['rows'].append(copy.deepcopy(s['geometry']['rows'][-1])))
        self.reject(lambda s:s['geometry']['rows'][-1].update(text='Wrong native body'))

    def test_read_retirement_only_during_held_arrival(self):
        self.reject(lambda s:s['model']['comments'].update(loading=True),5)
        self.reject(lambda s:s['model']['comments'].update(read=4),5)
        self.reject(lambda s:s['model']['comments'].update(error=None),5)
        self.reject(lambda s:s['model']['ledger'].update(cancelled=0),5)
        for i in (1,8,9):self.reject(lambda s:s['model']['comments'].update(read=99),i)

    def test_held_raw_body_and_actual_cancellation(self):
        self.reject(lambda s:s['proof']['heldResponse']['messages'].append(raw_message(2)),5)
        self.reject(lambda s:s['proof']['release'].update(delivered=True,cancelledBeforeReply=False),6)
        self.reject(lambda s:s['proof']['release'].update(cancelledBeforeReply=1),6)
        self.reject(lambda s:s['model']['ledger'].update(held=True),6)

    def test_stale_refresh_keeps_both_live_rows_without_fake_server_update(self):
        self.reject(lambda s:s['proof']['serverResponse']['messages'].append(raw_message(1)),7)
        self.reject(lambda s:s['model']['comments']['page']['messages'].append(raw_message(1)),7)
        self.reject(lambda s:s['model']['comments']['live'].pop(),7)
        self.reject(lambda s:s['model']['comments'].update(error=V['ERROR']),7)

    def test_reaction_targets_only_second_live_message(self):
        self.reject(lambda s:s['model']['comments']['live'][1].update(reactions=[]),8)
        self.reject(lambda s:s['geometry']['rows'][-1]['reactions'][0].update(count=2),8)
        self.reject(lambda s:s['model']['comments']['live'][0].update(reactions=[dict(name='wave',count=1,users=['U_RT_REACTOR'])]),8)

    def test_delete_keeps_live_one_and_tombstones_live_two(self):
        self.reject(lambda s:s['model']['comments'].update(deleted=[]),9)
        self.reject(lambda s:s['model']['comments']['visibleTs'].append(V['LIVE2']),9)
        self.reject(lambda s:s['model']['comments']['liveTs'].append(V['LIVE2']),9)
        self.reject(lambda s:s['geometry']['rows'].append(dict(ts=V['LIVE2'],text='resurrected',reactions=[])),11)

    def test_canonical_readback_dedups_and_filters_stale_deleted_row(self):
        for i in (10,11):
            self.reject(lambda s:s['proof']['serverResponse']['messages'].pop(),i)
            self.reject(lambda s:s['model']['comments']['page']['messages'].pop(),i)
            self.reject(lambda s:s['model']['comments']['live'].append(V['typed'](raw_message(1))),i)
            self.reject(lambda s:s['model']['comments']['liveTs'].append(V['LIVE1']),i)
            self.reject(lambda s:s['model']['comments'].update(deleted=[]),i)

    def test_no_post_prepared_policy_or_publication(self):
        for key in ('prepared','posts','messages','preflights','policies'):
            self.reject(lambda s:s['model']['composeLedger'][key].append({}),8)
        for key in ('published','held','heldPost'):
            self.reject(lambda s:s['model']['composeLedger'].update({key:True}),8)

    def test_exact_event_frames_and_before_after(self):
        for i in range(13):
            f,r=sample();r['incomingEvents'][i]['changed']=not r['incomingEvents'][i]['changed']
            with self.assertRaises(RuntimeError):self.check(f,r)
        for i in (1,2,3,4,5,6,7,8,12):
            f,r=sample();r['incomingEvents'][i]['after']['read']+=1
            with self.assertRaises(RuntimeError):self.check(f,r)
        f,r=sample();r['incomingEvents'][7]['parsed']=False
        with self.assertRaises(RuntimeError):self.check(f,r)

    def test_rich_draft_native_model_journal_every_stage(self):
        for i in range(12):
            self.reject(lambda s:s['proof']['journal'].update(blocks=[]),i)
            self.reject(lambda s:s['geometry']['editor'].update(blocks=[]),i)
            self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(blocks=[]),i)
        self.reject(lambda s:s['proof']['journal'].update(revision=999),9)
        self.reject(lambda s:s['geometry']['editor'].update(text=s['geometry']['editor']['text'].replace('\n\n','\n')))

    def test_seed_and_style_not_self_attested(self):
        for change in (lambda a:a[0]['elements'][0]['elements'][-1].update(style={'bold':True}),
                       lambda a:a[0]['elements'][0]['elements'][-2].update(style={}),
                       lambda a:a[0]['elements'][0]['elements'][1]['opaque'].update(retain=[9])):
            f,r=sample()
            for row in r['stages']:change(row['proof']['originalA'])
            with self.assertRaises(RuntimeError):self.check(f,r)

    def test_node_generation_focus_preserved_within_event_groups(self):
        for i in range(12):self.reject(lambda s:s['geometry']['editor'].update(sameNode=False),i)
        for i in (1,2,3,5,6,8,9,11):self.reject(lambda s:s['geometry'].update(active='unexpected'),i)
        self.reject(lambda s:s['geometry']['editor'].update(generation='999'),7)

    def test_notices_controls_types_and_captures(self):
        self.reject(lambda s:s['geometry'].update(notices=[]),5)
        self.reject(lambda s:s['geometry'].update(notices=[]),4)
        self.reject(lambda s:s['geometry']['controls'][-1].update(disabled=True),4)
        self.reject(lambda s:s['incomingProbe'].update(posts=False))
        self.reject(lambda s:s['model']['ledger'].update(cancelled=True),5)
        self.reject(lambda s:s['geometry'].update(viewport=[float('nan'),380]))
        self.reject(lambda s:s.update(capture='/foreign.png'))
        f,r=sample();seen=[]
        with patch.dict(V['H'],{'png_signature':lambda p,d:seen.append((p,d))}):V['validate'](f,r,DIR,dimensions=[1280,780])
        self.assertEqual(seen,[(DIR/f'{f}-{name}.png',[1280,780]) for name in V['STAGES']])


if __name__=='__main__':unittest.main()
