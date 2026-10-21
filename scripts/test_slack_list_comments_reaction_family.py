"""Portable per-child family receipts; no native dispatch or network."""
import copy
from pathlib import Path
import runpy
import unittest

V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_reaction_family.py')))
BASE=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_reaction_actions.py')))
CACHE={n:BASE['sample'](narrow=n) for n in (False,True)}
DIR=Path('/private/list-reaction-family')
OWN=['thumbsup','thumbsup::skin-tone-3','thumbsup::skin-tone-2','thumbsup::skin-tone-4']
SELECTED='thumbsup::skin-tone-6'
GLYPHS=dict(zip(OWN+[SELECTED,'+1::skin-tone-3','wave'],[
    '\U0001f44d','\U0001f44d\U0001f3fc','\U0001f44d\U0001f3fb','\U0001f44d\U0001f3fd',
    '\U0001f44d\U0001f3ff','\U0001f44d\U0001f3fc','\U0001f44b']))


def members(removed):
    source=[dict(name=n,count=2 if j==0 else 1,users=['U0','U1'] if j==0 else ['U0']) for j,n in enumerate(OWN)]
    for r in source:
        if r['name'] in removed:r['users'].remove('U0');r['count']-=1
    return [r for r in source if r['count']]+[
        dict(name=SELECTED,count=1,users=['U2']),dict(name='+1::skin-tone-3',count=1,users=['U0']),dict(name='wave',count=1,users=['U1'])]


def sample(case='reaction-family',narrow=False):
    fixture,base=copy.deepcopy(CACHE[narrow]);first=base['stages'][0];success=case=='reaction-family'
    original=copy.deepcopy(first['model']['comments']['page'])
    original['messages'][1].update(reactions=members([]),family_fixture_opaque={'keep':'raw bases are distinct'})
    future=copy.deepcopy(original);future['messages'][1]['reactions']=members([OWN[0],OWN[2]])
    target=dict(document=first['model']['document'],record=first['model']['records']['R_LIST_000'],channel='CCOMMENT01',root='1781524800.000001')
    full=dict(request=dict(method='reactions.get',args=dict(channel='CCOMMENT01',timestamp='1781524801.000001',full='true')),
              response=dict(ok=True,type='message',channel='CCOMMENT01',message=dict(ts='1781524801.000001',thread_ts='1781524800.000001',reactions=members([OWN[0],OWN[2]]))))
    rows=[];inputs=[];releases=[];updates=[]
    for i,stage in enumerate(V['STAGES'][case]):
        row=copy.deepcopy(first);row.update(stage=stage,capture=str(DIR/f'{fixture}-{stage}.png'))
        m,g,p=row['model'],row['geometry'],row['proof'];c=m['comments']
        if success:
            reads=[4,8,8,8,8,8,8,12,12,12][i];issued=[0,1,1,1,2,3,4,4,4,4][i];done=[0,0,0,0,1,2,3,3,4,4][i]
            codes=['','sqqq','sqqq','sqqq','csqq','ccsq','cccs','cccs','cccc','cccc'][i]
            removed=OWN[:[0,0,0,1,1,2,3,3,4,4][i]]
            groupnames=OWN;parents=[100]*4;plans=[OWN]*4;indices=list(range(issued));serial=[10,20,20,20,20,20,20,30,40,40][i]
            status=None if not i else 'Updating the reaction family. Your draft is unchanged.' if i<8 else 'Reaction family: 4 confirmed. Newer comment updates are shown.'
        else:
            reads=[4,8,8,12,16,16,20,24,24,24][i];issued=[0,3,3,3,4,4,4,5,6,6][i];done=[0,3,3,3,4,4,4,4,5,6][i]
            codes=['','crun','crun','cron','cronrn','cronrn','cronrn','cronrnsq','cronrncs','cronrncc'][i]
            removed=[] if not i else [OWN[0]] if i<3 else [OWN[0],OWN[2]] if i<8 else OWN[:3] if i==8 else OWN
            groupnames=OWN+[OWN[1],OWN[3]]*2;parents=[100]*4+[104]*2+[106]*2;plans=[OWN]*4+[[OWN[1],OWN[3]]]*4
            indices=[0,1,2,4,6,7][:issued];serial=[10,20,20,30,40,40,50,60,60,60][i]
            status=[None,'Reaction family: 1 confirmed, 1 rejected, 1 unconfirmed, 1 not sent. Refresh comments to check delivery.',
                    'Reaction family: 1 confirmed, 1 rejected, 1 unconfirmed, 1 not sent. Refresh comments to check delivery.',
                    'Reaction family: 1 confirmed, 1 observed, 1 rejected, 1 not sent. Newer comment updates are shown.',
                    'Reaction family: 1 rejected, 1 not sent.','Reaction family: 1 rejected, 1 not sent.','Reaction family: 1 rejected, 1 not sent.',
                    'Updating the reaction family. Your draft is unchanged.','Updating the reaction family. Your draft is unchanged.','Reaction family: 2 confirmed.'][i]
        phase={'s':'sent','q':'queued','c':'confirmed','r':'rejected','u':'unknown','n':'not-sent','o':'observed'}
        actions=[]
        for j,code in enumerate(codes):
            socket=success and j==0 and i>=3;observed=not success and j==2 and i>=3
            raw={'ok':True} if code=='c' else {'ok':False,'error':'invalid_name'} if not success and j==1 else None
            actions.append(dict(id=100+j,target=copy.deepcopy(target),ts='1781524801.000001',name=groupnames[j],user='U0',adding=False,
                                phase=phase[code],raw=raw,socketObserved=socket,socketRevision=int(socket),canonicalObserved=observed,
                                readbackResolved=observed,published=code=='c' and not socket,retryAt=9999999999 if not success and j==4 else 0,
                                fullReadback=copy.deepcopy(full['response']) if observed else None,parent=parents[j],family='thumbsup',
                                selected=SELECTED,planned=copy.deepcopy(plans[j]),issued=None if code in 'sq' else code!='n',
                                failure='transport' if not success and j==2 else 'rate-limited' if not success and j==4 else None))
        m['reactionActions']=dict(attempts=actions,pending=sum(code in 'squ' for code in codes))
        entries=[dict(id=100+j,user='U0',request=dict(method='reactions.remove',args=dict(channel='CCOMMENT01',timestamp='1781524801.000001',name=groupnames[j]))) for j in indices]
        outcomes=[]
        for j in indices[:done]:
            outcome=dict(id=100+j,disposition='response',raw={'ok':True})
            if not success and j==1:outcome['raw']={'ok':False,'error':'invalid_name'}
            if not success and j in (2,4):outcome.update(disposition='unconfirmed',raw=None)
            if not success and j==4:outcome.update(failure='rate-limited',retryAfterSecs=2)
            outcomes.append(outcome)
        m['reactionLedger']=dict(prepared=entries,issued=copy.deepcopy(entries),outcomes=outcomes,completed=done,held=issued>done,networkCalls=0,fullReads=[copy.deepcopy(full)] if not success and i>=3 else [])
        raw=original if success or i<3 else future
        typed=copy.deepcopy(first['model']['comments']['pageMessages']);typed[1].update(reactions=members(removed),family_fixture_opaque={'keep':'raw bases are distinct'})
        c.update(page=copy.deepcopy(raw),pageMessages=typed,read=serial,loading=success and i==7,error=V['NOTICE'] if success and i>=8 else None,
                 retryAt=9999999999 if not success and i==4 else 0,retryAfter=2 if not success and i==4 else 0)
        rx=[dict(name=r['name'],count=r['count']) for r in members(removed)]
        c['reactions']=[dict(ts=V['FIRST'],reactions=rx)];g['rows'][0]['reactions']=copy.deepcopy(rx)
        template=first['model']['ledger']['requests'];m['ledger'].update(requests=[dict(copy.deepcopy(r),serial=batch*4+j+1) for batch in range(reads//4) for j,r in enumerate(template)],completed=reads-int(success and i==7),replies=reads//4,cancelled=int(success and i>=8),held=success and i in (7,8))
        groups=0 if not i else 1 if success or i<4 else 2 if i<7 else 3
        m['composeLedger']['preflights']=[dict(method='conversations.genericInfo',args={'updated_channels':'{"CCOMMENT01":0}'})]*groups
        global_block=success and i==7 or not success and i==4;family_block=any(code in 'squ' for code in codes)
        probe=[]
        for r in members(removed):
            family=r['name'] in OWN+[SELECTED]
            enabled=not global_block and (not family_block if family else True)
            probe.append(dict(name=r['name'],count=r['count'],own='U0' in r['users'],glyph=GLYPHS[r['name']],available=enabled))
        m.update(familyProbe=probe,actionProbe=next(x for x in probe if x['name']==SELECTED),familyStatus=status)
        box=copy.deepcopy(g['editor']['rect'])
        pills=[dict(name=q['name'],count=q['count'],own=q['own'],glyph=q['glyph'],tag='BUTTON',type='button',disabled=not q['available'],ariaDisabled=str(not q['available']).lower(),pressed=str(q['own']).lower(),visible=True,hit=True,glyphVisible=True,glyphHit=True,rect=copy.deepcopy(box),glyphRect=copy.deepcopy(box)) for q in probe]
        g['family']=dict(pills=pills,tools=0,status=None if status is None else dict(text=status,role='status',visible=True,rect=copy.deepcopy(box)))
        g['action']=dict(editorText=g['action']['editorText']);g['notices']=[] if status is None else [status]
        if success and i>=8:g['notices'].append(V['NOTICE'])
        for control in g['controls']:
            control['disabled']=global_block if control['id'] in ('slack-list-comments-next','slack-list-comments-refresh') else not success and i==4 if control['id']=='slack-list-comment-send' else False
        if i in ((1,2) if success else (1,2,4,7)):inputs.append(dict(stage=stage,value=dict(ok=True,name=SELECTED,disabled=i==2,pressed='false')))
        if i in ((4,5,6,8) if success else (8,9)):releases.append(dict(stage=stage,value=dict(delivered=True,cancelledBeforeReply=False)))
        if not success and i in (3,6):updates.append(dict(stage=stage,value=copy.deepcopy(future)))
        p.update(initialResponse=copy.deepcopy(original),inputs=copy.deepcopy(inputs),releases=copy.deepcopy(releases),serverUpdates=copy.deepcopy(updates),socket=None,
                 heldResponse=copy.deepcopy(original) if success and i>=7 else None,release=dict(delivered=False,cancelledBeforeReply=True) if success and i==9 else None)
        rows.append(row)
    if success:
        event=dict(team='T1',generation=7,parsed=True,frame=dict(type='reaction_removed',user='U0',reaction='thumbsup',item=dict(type='message',channel='CCOMMENT01',ts='1781524801.000001')),
                   before=copy.deepcopy(rows[2]['model']['familyProbe']),after=copy.deepcopy(rows[3]['model']['familyProbe']))
        for row in rows[3:]:row['proof']['socket']=copy.deepcopy(event)
    base.update(case=case,stages=rows,final=rows[-1]['model'])
    return fixture,base


class FamilyReceipts(unittest.TestCase):
    def validate(self,r=None,case='reaction-family',narrow=False):
        f,b=sample(case,narrow);return V['validate'](f,b if r is None else r,DIR,png=False)
    def reject(self,fn,i=1,case='reaction-family'):
        _,r=sample(case);fn(r['stages'][i])
        with self.assertRaises(RuntimeError):self.validate(r,case)
    def test_both_cases_widths(self):
        for case in V['STAGES']:
            for narrow in (False,True):self.validate(case=case,narrow=narrow)
    def test_registration_and_legacy_shape(self):
        for case in V['STAGES']:
            stages,validate=V['C']['case_contract'](case);self.assertEqual(tuple(stages),V['STAGES'][case]);f,r=sample(case);validate(f,r,DIR,png=False)
        for case in ('reaction-actions','reaction-failures'):
            f,r=BASE['sample'](case);V['X']['validate'](f,r,BASE['DIR'],expected_case=case,png=False)
    def test_alias_and_nonowner_not_requested(self):
        for name in ('+1::skin-tone-3',SELECTED,'wave'):
            self.reject(lambda r:r['model']['reactionLedger']['prepared'][0]['request']['args'].update(name=name))
    def test_raw_order_and_selected_original(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][0].update(planned=list(reversed(OWN))))
        self.reject(lambda r:r['model']['reactionActions']['attempts'][0].update(selected=OWN[1]))
    def test_child_parent_and_scope(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][1].update(parent=101))
        self.reject(lambda r:r['model']['reactionActions']['attempts'][0]['target'].update(channel='COTHER'))
    def test_duplicate_dispatch_refused(self):
        self.reject(lambda r:r['model']['reactionLedger']['issued'].append(copy.deepcopy(r['model']['reactionLedger']['issued'][0])),2)
    def test_queued_is_not_issued(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][1].update(issued=True))
    def test_wire_proves_held_dispatch_separately(self):
        self.reject(lambda r:r['model']['reactionLedger'].update(issued=[]))
        self.reject(lambda r:r['model']['reactionActions']['attempts'][0].update(issued=True))
    def test_socket_does_not_apply_ack(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][0].update(raw={'ok':True},phase='confirmed'),3)
    def test_socket_cannot_rewrite_original_page(self):
        self.reject(lambda r:r['model']['comments']['page']['messages'][1]['reactions'][0].update(count=1),3)
    def test_remaining_children_not_abandoned_after_socket(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][1].update(phase='not-sent',issued=False),4)
    def test_mixed_original_ack_retained(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][0].update(published=False),1,'reaction-family-failures')
        self.reject(lambda r:r['model']['reactionActions']['attempts'][0].update(raw=None),3,'reaction-family-failures')
    def test_rejected_unknown_unsent_not_conflated(self):
        for n,phase in ((1,'not-sent'),(2,'rejected'),(3,'unknown')):
            self.reject(lambda r:r['model']['reactionActions']['attempts'][n].update(phase=phase),1,'reaction-family-failures')
    def test_issued429_is_rejected_not_uncertain_or_unsent(self):
        for update in (dict(phase='unknown'),dict(issued=False),dict(raw={'ok':True}),dict(retryAt=0),dict(failure='transport')):
            self.reject(lambda r:r['model']['reactionActions']['attempts'][4].update(update),4,'reaction-family-failures')
    def test_retryafter_is_original_failure_not_positive_response(self):
        self.reject(lambda r:r['model']['reactionLedger']['outcomes'][3].update(disposition='response',raw={'ok':True}),4,'reaction-family-failures')
        self.reject(lambda r:r['model']['reactionLedger']['outcomes'][3].update(retryAfterSecs=20),4,'reaction-family-failures')
    def test_fullread_required_for_removed_unknown(self):
        self.reject(lambda r:r['model']['reactionLedger'].update(fullReads=[]),3,'reaction-family-failures')
        self.reject(lambda r:r['model']['reactionLedger']['fullReads'][0]['request']['args'].update(full='false'),3,'reaction-family-failures')
        self.reject(lambda r:r['model']['reactionActions']['attempts'][2].update(fullReadback=None),3,'reaction-family-failures')
    def test_resolved_does_not_rewrite_unknown_raw(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][2].update(raw={'ok':True}),3,'reaction-family-failures')
    def test_fresh_retry_only_remaining_members_new_parent(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][6].update(parent=100),7,'reaction-family-failures')
        self.reject(lambda r:r['model']['reactionLedger']['prepared'][-1]['request']['args'].update(name=OWN[2]),7,'reaction-family-failures')
    def test_cooldown_not_derived_enabled_early(self):
        self.reject(lambda r:r['model']['comments'].update(retryAt=0),4,'reaction-family-failures')
        self.reject(lambda r:r['model']['comments'].update(retryAt=9999999999),5,'reaction-family-failures')
    def test_stale_held_read_cannot_deliver_or_rewrite(self):
        self.reject(lambda r:r['proof']['release'].update(delivered=True,cancelledBeforeReply=False),9)
        self.reject(lambda r:r['proof']['heldResponse']['messages'][1].update(reactions=[]),8)
    def test_final_unowned_pill_can_add_alias_stays_owned(self):
        for case,indices in (('reaction-family',(8,9)),('reaction-family-failures',(9,))):
            f,r=sample(case);V['validate'](f,r,DIR,png=False)
            for i in indices:
                row=r['stages'][i]
                self.assertIs(row['model']['actionProbe']['available'],True)
                pill=next(p for p in row['geometry']['family']['pills'] if p['name']==SELECTED)
                self.assertIs(pill['disabled'],False)
                self.assertEqual(pill['ariaDisabled'],'false')
                self.reject(lambda r:r['model']['actionProbe'].update(available=False),i,case)
                self.reject(lambda r:next(p for p in r['geometry']['family']['pills'] if p['name']==SELECTED).update(disabled=True,ariaDisabled='true'),i,case)
        self.reject(lambda r:r['model']['familyProbe'][-2].update(own=False),9)
    def test_pending_unknown_and_cooldown_remain_disabled(self):
        for case,indices in (('reaction-family',(1,2,3,4,5,6,7)),('reaction-family-failures',(1,2,4,7,8))):
            for i in indices:
                self.reject(lambda r:r['model']['actionProbe'].update(available=True),i,case)
                self.reject(lambda r:next(p for p in r['geometry']['family']['pills'] if p['name']==SELECTED).update(disabled=False,ariaDisabled='false'),i,case)

    def test_exact_partial_guidance_and_real_editor_line(self):
        self.reject(lambda r:r['geometry']['family']['status'].update(text='All reactions removed.'),1,'reaction-family-failures')
        self.reject(lambda r:r['geometry']['action']['editorText'].update(ok=False,visibleLines=0))
    def test_journal_native_rich_and_no_comment(self):
        self.reject(lambda r:r['proof']['journal'].update(blocks=[]))
        self.reject(lambda r:r['geometry']['editor'].update(sameNode=False))
        self.reject(lambda r:r['model']['composeLedger']['posts'].append({'method':'chat.postMessage'}))


if __name__=='__main__':unittest.main()
