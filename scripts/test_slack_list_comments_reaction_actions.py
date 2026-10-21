"""Portable adversarial receipts for closed List reaction actions; no native IO."""
import copy
import json
from pathlib import Path
import runpy
import unittest

V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_reaction_actions.py')))
BASE=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_edits.py')))
DIR=Path('/private/list-reaction-actions')
TS,CH,ROOT='1781524801.000001','CCOMMENT01','1781524800.000001'


def raw_page(members):
    page=copy.deepcopy(BASE['sample']()[1]['stages'][0]['model']['comments']['page'])
    page['messages'][1]['reactions']=[dict(name='wave',count=len(members),users=members)] if members else []
    return page


def form(add):
    return dict(method='reactions.add' if add else 'reactions.remove',args=dict(channel=CH,timestamp=TS,name='wave'))


def sample(case='reaction-actions',narrow=False):
    fixture,base=BASE['sample']('edits',narrow);first=base['stages'][0];initial=first['model']
    target=dict(document=initial['document'],record=initial['records']['R_LIST_000'],channel=CH,root=ROOT)
    success=case=='reaction-actions';rows=[];events=[];actions=[];prepared=[];issued=[];outcomes=[]
    updates=[];releases=[];inputs=[];lone=[];serial=initial['comments']['read'];page=copy.deepcopy(initial['comments']['page'])
    full=dict(ok=True,type='message',channel=CH,message=dict(ts=TS,thread_ts=ROOT,reactions=[]))
    full_reads=[]
    for i,name in enumerate(V['STAGES'][case]):
        row=copy.deepcopy(first);row.update(stage=name,capture=str(DIR/f'{fixture}-{name}.png'))
        m,g,p=row['model'],row['geometry'],row['proof'];c=m['comments']
        if success:
            reads=(4,8,8,8,8,12,12,12,16,16,16,16,20,24,24,24)[i]
            own=i in (3,4,5,9,14,15);members=['U1','U2']+(['U0'] if own else [])
            if i in (1,5,8,12):
                add=i!=5;identity=101+len(actions)
                actions.append(dict(id=identity,target=copy.deepcopy(target),ts=TS,name='wave',user='U0',adding=add,
                    phase='sent',raw=None,socketObserved=False,socketRevision=0,canonicalObserved=False,
                    readbackResolved=False,published=False,retryAt=0,fullReadback=None))
                entry=dict(id=identity,request=form(add),user='U0');prepared.append(entry);issued.append(copy.deepcopy(entry))
            if i in (3,6,9,10):
                for a in actions:a.update(socketObserved=True,socketRevision=a['socketRevision']+1)
            if i in (4,7,11,14):
                actions[-1].update(phase='confirmed',raw=dict(ok=True),published=i==14)
                outcomes.append(dict(id=actions[-1]['id'],disposition='response',raw=dict(ok=True)))
                releases.append(dict(stage=name,value=dict(delivered=True,cancelledBeforeReply=False)))
            if i in (3,6,9,10):
                future=raw_page(members);updates.append(dict(stage=name,value=dict(ts=TS,own=own,reactions=future['messages'][1]['reactions'],response=future)))
            if i in (1,2,5,8,12):
                inputs.append(dict(stage=name,value=dict(ok=True,disabled=i==2,pressed=str(i==5).lower(),type='button',focused=i!=2)))
            if i in (1,5,8,12,13,14):serial+=3
            loading=i==13;held=i in (13,14);cancelled=int(i>=14)
            completed=reads-int(loading);reaction_held=len(prepared)>len(outcomes)
            status=None if not actions else 'Updating the reaction. Your draft is unchanged.' if reaction_held else \
                'Reaction updated.' if i>=14 else 'Reaction response retained. Newer comment updates are shown.'
            disabled=reaction_held
            p['heldResponse']=copy.deepcopy(page) if i>=13 else None
            p['release']=dict(delivered=False,cancelledBeforeReply=True) if i==15 else None
        else:
            reads=(4,8,12,16,16,20,24,24,28,32)[i];completed=reads;cancelled=0;held=False;loading=False;reaction_held=False
            members=['U1','U2'] if i<6 else ['U1','U2','U0'] if i==6 else ['U0'] if i<9 else []
            own='U0' in members;disabled=i in (3,5,8,9)
            if i in (1,2,3,5,8):
                identity=101+len(actions);raw=dict(ok=False,error='invalid_name') if i==1 else dict(ok='true',opaque='malformed-reaction-receipt') if i==8 else None
                phase='rejected' if i==1 else 'not-sent' if i in (2,3) else 'unknown'
                actions.append(dict(id=identity,target=copy.deepcopy(target),ts=TS,name='wave',user='U0',adding=i!=8,
                    phase=phase,raw=raw,socketObserved=False,socketRevision=0,canonicalObserved=False,
                    readbackResolved=False,published=False,retryAt=9999999999 if i==3 else 0,fullReadback=None))
                if i!=3:
                    entry=dict(id=identity,request=form(i!=8),user='U0');prepared.append(entry)
                    if i!=2:issued.append(copy.deepcopy(entry))
                    outcomes.append(dict(id=identity,disposition='not-requested' if i==2 else 'unconfirmed' if i==5 else 'response',raw=raw))
                inputs.append(dict(stage=name,value=dict(ok=True,disabled=False,pressed=str(i==8).lower(),type='button',focused=True)))
            if i in (6,9):
                actions[-1].update(phase='observed',canonicalObserved=True,readbackResolved=True,fullReadback=copy.deepcopy(full) if i==9 else None)
                page=raw_page(members)
            if i==9:full_reads=[dict(request=dict(method='reactions.get',args=dict(channel=CH,timestamp=TS,full='true')),response=copy.deepcopy(full))]
            if i in (6,7,9):
                future=raw_page(members);updates.append(dict(stage=name,value=dict(ts=TS,own=own,reactions=future['messages'][1]['reactions'],response=future)))
            if i in (1,2,3,5,6,8,9):serial+=3
            status=None if not actions else {'rejected':'Slack rejected the reaction. Your draft is unchanged.',
                'not-sent':'The reaction was not sent. Refresh comments before trying again.',
                'unknown':'Reaction delivery is unconfirmed. Refresh comments to check before trying again.',
                'observed':'The requested reaction state was observed in refreshed comments.'}[actions[-1]['phase']]
            if i==7:
                before=copy.deepcopy(rows[6]['model']['actionProbe'])
                for user,count in [('U1',2),('U2',1)]:
                    after=copy.deepcopy(before);after['count']=count
                    lone.append(dict(stage=name,team='T1',generation=7,parsed=True,
                        frame=dict(type='reaction_removed',user=user,reaction='wave',item=dict(type='message',channel=CH,ts=TS)),before=before,after=after))
                    before=copy.deepcopy(after)
            if lone:p['loneEvents']=copy.deepcopy(lone)
        c.update(page=copy.deepcopy(page),pageMessages=[V['I']['typed'](raw) for raw in page['messages']],
            read=serial,loading=loading,error=V['I']['ERROR'] if success and i>=14 else None,
            retryAt=9999999999 if not success and i==3 else 0,retryAfter=2 if not success and i==3 else 0)
        reactions=[dict(name='wave',count=len(members),users=members)] if members else []
        c['pageMessages'][1]['reactions']=copy.deepcopy(reactions)
        projection=[dict(ts=TS,reactions=[dict(name='wave',count=len(members))] if members else [])]
        c.update(reactions=copy.deepcopy(projection),visibleTs=[TS],live=[],liveTs=[],accepted=[],acceptedTs=[],liveReactions=[],editCustody=[],deleted=[],reveal=None)
        m['ledger'].update(requests=[dict(copy.deepcopy(r),serial=4*b+j+1) for b in range(reads//4) for j,r in enumerate(initial['ledger']['requests'])],
            replies=reads//4,completed=completed,cancelled=cancelled,held=held)
        m['composeLedger']['preflights']=[dict(method='conversations.genericInfo',args={'updated_channels':'{"CCOMMENT01":0}'}) for _ in prepared]
        m['reactionLedger']=dict(prepared=copy.deepcopy(prepared),issued=copy.deepcopy(issued),outcomes=copy.deepcopy(outcomes),
            completed=len(outcomes),held=reaction_held,networkCalls=0,fullReads=copy.deepcopy(full_reads))
        m['reactionActions']=dict(attempts=copy.deepcopy(actions),pending=sum(a['phase'] in ('sent','unknown') for a in actions))
        m['actionProbe']=dict(ts=TS,name='wave',generation=c['generation'],own=own,count=len(members),available=not disabled,status=status)
        g['rows']=[dict(ts=TS,text='First page comment. Review ready \u4e2d\U0001f642.',reactions=projection[0]['reactions'])]
        g['active']='slack-list-comment-draft' if i==0 else ''
        g['notices']=[V['I']['ERROR']] if c['error'] else ['Loading comments\u2026'] if loading else []
        for control in g['controls']:control['disabled']=(loading or not success and i==3) if control['id'] in ('slack-list-comments-next','slack-list-comments-refresh') else not success and i==3 if control['id']=='slack-list-comment-send' else False
        box=copy.deepcopy(g['editor']['rect'])
        g['action']=dict(button=dict(tag='BUTTON',type='button',name='wave',disabled=disabled,ariaDisabled=str(disabled).lower(),
            pressed=str(own).lower(),own=own,count=len(members),visible=True,hit=True,rect=box) if members else None,
            status=dict(text=status,role='status',visible=True,outsideScroll=True,rect=box) if status else None,tools=0,
            editorText=dict(clip=copy.deepcopy(box),
                lines=[dict(rect=copy.deepcopy(box),fragments=[dict(rect=copy.deepcopy(box),hit=True)],fullVisible=True,hit=True)],visibleLines=1,ok=True))
        p.update(serverUpdates=copy.deepcopy(updates),releases=copy.deepcopy(releases),inputs=copy.deepcopy(inputs))
        rows.append(row)
        if success and i in (3,6,9,10):
            events.append(dict(stage=name,team='T1',generation=7,parsed=True,
                frame=dict(type='reaction_added' if own else 'reaction_removed',user='U0',reaction='wave',item=dict(type='message',channel=CH,ts=TS)),
                before=copy.deepcopy(rows[i-1]['model']['actionProbe']),after=copy.deepcopy(m['actionProbe'])))
    return fixture,dict(ok=True,case=case,fixture=fixture,noLiveNetwork=True,readiness=base['readiness'],
        initialRecords=base['initialRecords'],stages=rows,reactionEvents=events,final=copy.deepcopy(rows[-1]['model']))


class ReactionActionTests(unittest.TestCase):
    def check(self,f,r):return V['validate'](f,r,DIR,png=False)
    def reject(self,change,index=3,case='reaction-actions'):
        f,r=sample(case);change(r['stages'][index])
        with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError)):self.check(f,r)

    def test_both_cases_both_widths(self):
        for case in V['STAGES']:
            for narrow in (False,True):self.check(*sample(case,narrow))

    def test_dispatch_is_bound_to_case_and_exact_stages(self):
        for case in V['STAGES']:
            stages,validate=V['C']['case_contract'](case);self.assertEqual(stages,V['STAGES'][case])
            f,r=sample(case);validate(f,r,DIR,png=False);r['case']='reaction-failures' if case=='reaction-actions' else 'reaction-actions'
            with self.assertRaises(RuntimeError):validate(f,r,DIR,png=False)

    def test_exact_request_factory_and_self(self):
        for key,value in [('channel','COTHER'),('timestamp',ROOT),('name','smile'),('user','U1')]:
            def change(s,k=key,v=value):
                item=s['model']['reactionLedger']['prepared'][0]
                if k=='user':item[k]=v
                else:item['request']['args'][k]=v
            self.reject(change)
        self.reject(lambda s:s['model']['reactionLedger']['prepared'][0]['request'].update(method='reactions.remove'))
        self.reject(lambda s:s['model']['reactionLedger']['prepared'][0]['request']['args'].update(ts=TS))

    def test_duplicate_pending_request_or_automatic_retry(self):
        self.reject(lambda s:s['model']['reactionLedger']['issued'].append(copy.deepcopy(s['model']['reactionLedger']['issued'][0])),2)
        self.reject(lambda s:s['model']['ledger']['requests'].append(copy.deepcopy(s['model']['ledger']['requests'][0])),4,'reaction-failures')

    def test_outcome_response_is_not_readback(self):
        self.reject(lambda s:s['model']['reactionLedger']['outcomes'][0].update(raw=dict(ok=True)),1,'reaction-failures')
        self.reject(lambda s:s['model']['reactionLedger']['outcomes'][2].update(disposition='response',raw=dict(ok=True)),6,'reaction-failures')
        self.reject(lambda s:s['model']['reactionActions']['attempts'][3].update(raw=dict(ok=True)),6,'reaction-failures')
        self.reject(lambda s:s['model']['reactionActions']['attempts'][4].update(raw=dict(ok=True)),9,'reaction-failures')

    def test_json_types_cannot_alias_boolean_dispositions(self):
        self.reject(lambda s:s['model']['reactionLedger']['outcomes'][0]['raw'].update(ok=1),4)
        self.reject(lambda s:s['model']['reactionActions']['attempts'][0].update(adding=1),1)
        self.reject(lambda s:s['proof']['releases'][0]['value'].update(delivered=1),4)
        self.reject(lambda s:s['model']['reactionLedger']['fullReads'][0]['response'].update(ok=1),9,'reaction-failures')

    def test_no_offline_or_rate_dispatch(self):
        self.reject(lambda s:s['model']['reactionLedger']['issued'].append(s['model']['reactionLedger']['prepared'][1]),2,'reaction-failures')
        self.reject(lambda s:s['model']['composeLedger']['preflights'].append(s['model']['composeLedger']['preflights'][0]),3,'reaction-failures')

    def test_exact_own_membership_and_other_reactors(self):
        self.reject(lambda s:s['model']['comments']['pageMessages'][1]['reactions'][0].update(users=['U0'],count=1))
        self.reject(lambda s:s['model']['actionProbe'].update(own=False))
        self.reject(lambda s:s['geometry']['action']['button'].update(pressed='false'))
        self.reject(lambda s:s['model']['comments']['pageMessages'][1]['reactions'][0].update(users=['U1','U2','U0']),7,'reaction-failures')

    def test_newer_opposite_socket_not_overwritten_by_ack(self):
        self.reject(lambda s:s['model']['actionProbe'].update(own=True,count=3),11)
        self.reject(lambda s:s['model']['reactionActions']['attempts'][2].update(socketObserved=False),11)
        self.reject(lambda s:s['model']['reactionActions']['attempts'][2].update(published=True),11)

    def test_exact_original_page_and_held_body(self):
        self.reject(lambda s:s['model']['comments']['page']['messages'][1].update(reactions=[]))
        self.reject(lambda s:s['proof']['heldResponse']['messages'][1].update(reactions=[]),14)
        self.reject(lambda s:s['proof']['release'].update(delivered=True),15)
        self.reject(lambda s:s['model']['ledger'].update(cancelled=0),15)

    def test_full_absence_requires_exact_full_true_response(self):
        for change in [lambda s:s['model']['reactionLedger'].update(fullReads=[]),
                       lambda s:s['model']['reactionLedger']['fullReads'][0]['request']['args'].update(full='false'),
                       lambda s:s['model']['reactionLedger']['fullReads'][0]['response']['message'].pop('reactions'),
                       lambda s:s['model']['reactionActions']['attempts'][4].update(fullReadback=None),
                       lambda s:s['model']['reactionActions']['attempts'][4].update(readbackResolved=False)]:
            self.reject(change,9,'reaction-failures')

    def test_complete_remove_removes_final_pill(self):
        self.reject(lambda s:s['geometry']['action'].update(button=dict(tag='BUTTON')),9,'reaction-failures')
        self.reject(lambda s:s['model']['comments']['pageMessages'][1].update(reactions=[dict(name='wave',count=1,users=['U0'])]),9,'reaction-failures')

    def test_exact_draft_native_model_disk_and_mount(self):
        self.reject(lambda s:s['geometry']['editor'].update(sameNode=False))
        self.reject(lambda s:s['geometry']['editor'].update(generation='999'))
        self.reject(lambda s:s['geometry']['editor'].update(text=s['geometry']['editor']['text'].replace('\n\n','\n')))
        self.reject(lambda s:s['proof']['journal'].update(blocks=[]))
        self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(busy=True))
        self.reject(lambda s:s['proof']['journal'].update(capture='ffffffff-ffff-4fff-8fff-ffffffffffff'))

    def test_editor_box_alone_cannot_mask_clipped_text(self):
        self.reject(lambda s:s['geometry']['action'].pop('editorText'),13)
        self.reject(lambda s:s['geometry']['action']['editorText'].update(lines=[],visibleLines=0),13)
        def below(s):
            text=s['geometry']['action']['editorText'];r=text['lines'][0]['rect']
            r.update(top=r['bottom']+5,bottom=r['bottom']+5+r['height'])
            text['lines'][0]['fragments'][0]['rect']=copy.deepcopy(r)
            text['lines'][0]['fullVisible']=False;text['visibleLines']=0
        self.reject(below,13)
        def forged_visibility(s):
            below(s);text=s['geometry']['action']['editorText']
            text['lines'][0]['fullVisible']=True;text['visibleLines']=1
        self.reject(forged_visibility,14)

    def test_real_line_must_be_hit_testable_and_count_exact(self):
        self.reject(lambda s:s['geometry']['action']['editorText']['lines'][0].update(hit=False),13)
        self.reject(lambda s:s['geometry']['action']['editorText']['lines'][0]['fragments'][0].update(hit=False),13)
        self.reject(lambda s:s['geometry']['action']['editorText']['lines'][0].update(fragments=[]),13)
        self.reject(lambda s:s['geometry']['action']['editorText'].update(visibleLines=2),14)
        self.reject(lambda s:s['geometry']['action']['editorText'].update(visibleLines=True),14)
        self.reject(lambda s:s['geometry']['action']['editorText']['clip'].update(height=9999),15)
        self.reject(lambda s:s['geometry']['action']['editorText']['lines'][0]['rect'].update(height=float('nan')),15)

    def test_preserved_clipped_native_run_cannot_pass_box_only_evidence(self):
        path=Path(__file__).resolve().parents[1]/'tmp/list-comments-reaction-actions-20261019/reaction-actions-a/slack-list-narrow/slack-list-narrow.json'
        if not path.is_file():self.skipTest('Preserved clipped native receipt is local evidence')
        outer=json.loads(path.read_text(encoding='utf-8'))
        self.assertTrue(outer['ok'])
        rows={r['stage']:r for r in outer['native']['stages']}
        # These originals were visually rejected by root: the editor box and
        # toolbar existed while every draft text line was below the clip. The
        # previous receipt did not measure text ranges at all. Do not invent
        # range coordinates or retrofit this old receipt into an accepted one.
        for stage in ('read-held','local-acknowledged','read-released'):
            g=rows[stage]['geometry']
            self.assertTrue(g['editor']['visible']);self.assertTrue(g['editor']['blocks'])
            self.assertNotIn('editorText',g['action'])
            with self.assertRaisesRegex(RuntimeError,'Missing real draft text clip'):V['editor_text'](g)

    def test_exact_accessible_control_semantics(self):
        for key,value in [('tag','SPAN'),('type','submit'),('disabled',False),('ariaDisabled','false'),('hit',False),('visible',False)]:
            self.reject(lambda s,k=key,v=value:s['geometry']['action']['button'].update({k:v}),1)
        self.reject(lambda s:s['geometry']['action'].update(tools=1))
        self.reject(lambda s:s['geometry']['action']['status'].update(outsideScroll=False))
        self.reject(lambda s:s['geometry']['action']['status'].update(role='alert'))
        self.reject(lambda s:s['geometry']['action']['status'].update(text='Reaction updated.'),5,'reaction-failures')

    def test_rate_deadline_and_unknown_lock(self):
        self.reject(lambda s:s['model']['comments'].update(retryAfter=0),3,'reaction-failures')
        self.reject(lambda s:s['model']['comments'].update(retryAt=9999999999,retryAfter=0),4,'reaction-failures')
        self.reject(lambda s:s['model']['actionProbe'].update(available=True),5,'reaction-failures')
        self.reject(lambda s:s['model']['reactionActions'].update(pending=0),8,'reaction-failures')

    def test_account_cooldown_disables_send_but_expiry_restores_it(self):
        def send(s,disabled):
            next(c for c in s['geometry']['controls'] if c['id']=='slack-list-comment-send')['disabled']=disabled
        self.reject(lambda s:send(s,False),3,'reaction-failures')
        self.reject(lambda s:send(s,True),4,'reaction-failures')
        self.reject(lambda s:send(s,True),5,'reaction-failures')

    def test_preserved_failure_c_native_account_cooldown(self):
        path=Path(__file__).resolve().parents[1]/'tmp/list-comments-reaction-actions-20261019/reaction-failures-c/slack-list/slack-list.json'
        if not path.is_file():self.skipTest('Preserved failure native receipt is local evidence')
        outer=json.loads(path.read_text(encoding='utf-8'));receipt=outer['terminalReceipts'][0]
        self.assertFalse(outer['ok']);self.assertEqual(outer['error'],'Read/draft controls changed with reaction action')
        self.assertEqual(len(receipt['stages']),10)
        for row in receipt['stages']:
            send=next(c for c in row['geometry']['controls'] if c['id']=='slack-list-comment-send')
            self.assertIs(send['disabled'],row['stage']=='rate-limited')
        V['validate']('slack-list',receipt,Path(outer['readyDirectory']),png=False)

    def test_event_provenance_is_parsed_exact_current_scope(self):
        for key,value in [('team','T_OTHER'),('generation',6),('parsed',False)]:
            f,r=sample();r['reactionEvents'][0][key]=value
            with self.assertRaises(RuntimeError):self.check(f,r)
        f,r=sample();r['reactionEvents'][0]['frame']['user']='U_OTHER'
        with self.assertRaises(RuntimeError):self.check(f,r)
        self.reject(lambda s:s['proof']['loneEvents'][0]['frame'].update(user='U0'),7,'reaction-failures')

    def test_no_hidden_send_or_owner_replacement(self):
        self.reject(lambda s:s['model']['composeLedger']['posts'].append(dict(method='chat.postMessage')))
        self.reject(lambda s:s['model'].update(noSession=False))
        self.reject(lambda s:s['model'].update(account='ffffffff-ffff-4fff-8fff-ffffffffffff'))
        self.reject(lambda s:s['model']['reactionActions']['attempts'][0]['target'].update(root='foreign'))

    def test_capture_final_and_closed_lifecycle_edges(self):
        self.reject(lambda s:s.update(capture='/foreign.png'))
        f,r=sample();r['final']={}
        with self.assertRaises(RuntimeError):self.check(f,r)
        f,r=sample();C=V['C']
        class Log:
            def receipts(self,prefix):
                if prefix==C['WAITING']:return [r['readiness']]
                if prefix==C['PREFIX']:return [r]
                return []
        with self.assertRaisesRegex(RuntimeError,'Capture lifecycle edges'):
            C['validate_log'](Log(),f,DIR,r['readiness'],None,'reaction-actions')


if __name__=='__main__':unittest.main()
