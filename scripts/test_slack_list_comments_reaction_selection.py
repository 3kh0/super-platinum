"""Portable selected-reaction receipts; never dispatches native or Slack work."""
import copy
import json
from pathlib import Path
import runpy
import unittest
V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_reaction_selection.py')))
BASE=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_reaction_actions.py')))
CACHE={n:BASE['sample'](narrow=n) for n in (False,True)}
DIR=Path('/private/list-reaction-selection')
TONE='+1::skin-tone-4';CUSTOM='fixture_selection_custom';ALIAS='fixture_selection_alias'
INITIAL=['+1::skin-tone-2','thumbsup::skin-tone-4','eyes']
GLYPHS=dict(zip(INITIAL+[TONE,CUSTOM],['\U0001f44d\U0001f3fb','\U0001f44d\U0001f3fd','\U0001f440','\U0001f44d\U0001f3fd',None]))
def members(names):return [dict(name=n,count=1,users=['U0']) for n in names]


def sample(case='reaction-selection',narrow=False):
    fixture,receipt=copy.deepcopy(CACHE[narrow]);first=receipt['stages'][0];initial=first['model'];success=case=='reaction-selection';failure=case=='reaction-selection-failures';custody=not success and not failure
    raw=copy.deepcopy(initial['comments']['page']);raw['messages'][1].update(reactions=members(INITIAL),selection_fixture_opaque={'keep':'exact selection differs from raw-base family'})
    future=copy.deepcopy(raw);future['messages'][1]['reactions']=members(INITIAL[:2])
    target=dict(document=initial['document'],record=initial['records']['R_LIST_000'],channel='CCOMMENT01',root='1781524800.000001')
    full=dict(request=dict(method='reactions.get',args=dict(channel='CCOMMENT01',timestamp='1781524801.000001',full='true')),
              response=dict(ok=True,type='message',channel='CCOMMENT01',message=dict(ts='1781524801.000001',thread_ts='1781524800.000001',reactions=members(INITIAL[:2]))))
    prefs=dict(preferred_skin_tone=4,emoji_mode='native',emoji_use={},frecency='{}')
    original=dict(raw=dict(ok=True,prefs=copy.deepcopy(prefs),fixture_opaque='original complete prefs'),parsed=copy.deepcopy(prefs),preferences=dict(loaded=True,revision=1,raw=copy.deepcopy(prefs)),customRevision=2,
                  custom={CUSTOM:dict(name=CUSTOM,value='https://fixture.invalid/selection-custom.png'),ALIAS:dict(name=ALIAS,value='alias:'+CUSTOM)})
    rows=[];inputs=[];releases=[];server=[];socket=[];changes=[]
    for i,stage in enumerate(V['STAGES'][case]):
        row=copy.deepcopy(first);row.update(stage=stage,capture=str(DIR/f'{fixture}-{stage}.png'));m,g,p=row['model'],row['geometry'],row['proof'];c=m['comments']
        if success:
            count=[0,0,0,0,1,1,1,2,3,4][i];done=[0,0,0,0,0,0,1,2,3,4][i];reads=[4,4,4,4,8,8,8,12,16,20][i]
            codes=['','','','','s','s','c','cc','ccc','cccc'][i];names=[TONE,TONE,CUSTOM,CUSTOM];adding=[True,False,True,False]
            current=INITIAL+([TONE] if i in (5,6) else [CUSTOM] if i==8 else []);serial=[10,10,10,10,20,20,20,30,40,50][i]
        elif failure:
            count=[0,1,2,2,3,3,4,4][i];done=[0,1,2,2,3,3,3,4][i];reads=[4,8,12,16,20,20,24,24][i]
            codes=['','r','ru','ro','ror','ror','rors','rorc'][i];names=['eyes']*4;adding=[False,False,True,True]
            current=INITIAL[:2] if 3<=i<7 else INITIAL;serial=[10,20,30,40,50,50,60,60][i]
        else:
            count=done=0;reads=8 if i>=5 else 4;codes='p' if i==5 else 'n' if i==6 else '';names=['eyes'];adding=[False];current=INITIAL;serial=20 if i>=5 else 10
        loading=custody and i==5;rate=failure and i==4;opened=success and i in (1,2) or custody and i in (1,2,3,4);ready=not(custody and i in (2,4))
        phases={'s':'sent','c':'confirmed','r':'rejected','u':'unknown','o':'observed','p':'preparing','n':'not-sent'}
        actions=[]
        for j,code in enumerate(codes):
            observed=failure and j==1 and i>=3;sock=success and j==0 and i>=5
            actions.append(dict(id=100+j,target=copy.deepcopy(target),ts='1781524801.000001',name=names[j],user='U0',adding=adding[j],phase=phases[code],
                raw={'ok':True} if code=='c' else {'ok':False,'error':'invalid_name'} if failure and j==0 else None,
                socketObserved=sock,socketRevision=int(sock),canonicalObserved=observed,readbackResolved=observed,published=code=='c' and not sock,
                retryAt=9999999999 if failure and j==2 else 0,fullReadback=copy.deepcopy(full['response']) if observed else None))
        m['reactionActions']=dict(attempts=actions,pending=sum(code in 'sup' for code in codes))
        wire=[dict(id=100+j,user='U0',request=dict(method='reactions.add' if adding[j] else 'reactions.remove',args=dict(channel='CCOMMENT01',timestamp='1781524801.000001',name=names[j]))) for j in range(count)]
        outcomes=[]
        for j in range(done):
            out=dict(id=100+j,disposition='response',raw={'ok':True})
            if failure and j==0:out['raw']={'ok':False,'error':'invalid_name'}
            if failure and j in (1,2):out.update(disposition='unconfirmed',raw=None)
            if failure and j==2:out.update(failure='rate-limited',retryAfterSecs=2)
            outcomes.append(out)
        m['reactionLedger']=dict(prepared=wire,issued=copy.deepcopy(wire),outcomes=outcomes,completed=done,held=count>done,networkCalls=0,fullReads=[copy.deepcopy(full)] if failure and i>=3 else [])
        current_page=future if failure and i>=3 else raw
        typed=copy.deepcopy(initial['comments']['pageMessages']);typed[1].update(reactions=members(current),selection_fixture_opaque={'keep':'exact selection differs from raw-base family'})
        rx=[dict(name=n,count=1) for n in current]
        c.update(page=copy.deepcopy(current_page),pageMessages=typed,reactions=[dict(ts='1781524801.000001',reactions=rx)],read=serial,loading=loading,error=None,retryAt=9999999999 if rate else 0,retryAfter=2 if rate else 0)
        g['rows'][0]['reactions']=copy.deepcopy(rx)
        template=initial['ledger']['requests'];m['ledger'].update(requests=[dict(copy.deepcopy(r),serial=batch*4+j+1) for batch in range(reads//4) for j,r in enumerate(template)],completed=reads,cancelled=0,held=False,replies=reads//4)
        m['composeLedger']['preflights']=[dict(method='conversations.genericInfo',args={'updated_channels':'{"CCOMMENT01":0}'})]*(int(i>=5) if custody else count)
        m['composeLedger']['held']=loading
        phase=phases[codes[-1]] if codes else None
        status={'sent':'Updating the reaction. Your draft is unchanged.','confirmed':'Reaction updated.','rejected':'Slack rejected the reaction. Your draft is unchanged.',
                'unknown':'Reaction delivery is unconfirmed. Refresh comments to check before trying again.','observed':'The requested reaction state was observed in refreshed comments.',
                'preparing':'Checking the current reaction. Your draft is unchanged.','not-sent':'The reaction was not sent. Refresh comments before trying again.'}.get(phase)
        if success and i==6:status='Reaction response retained. Newer comment updates are shown.'
        available=not opened and not rate and not any(x in 'sp' for x in codes)
        m['actionProbe']=dict(available=available,status=status);m['selectionProbe']=[dict(name=n,count=1,own=True,glyph=GLYPHS[n]) for n in current]
        source=dict(original=copy.deepcopy(original),preferences=copy.deepcopy(original['preferences']),customRevision=2+int(custody and i>=2),catalog=None if not i else dict(token='201/local' if custody and i>=2 else '200/local',current=True))
        source['custom']=copy.deepcopy(original['custom'])
        if custody and i>=2:source['custom'][ALIAS]['value']='alias:eyes'
        if custody and i>=4:source['preferences']['revision']=2;source['preferences']['raw']['preferred_skin_tone']=5
        if custody and i>=6:source['preferences']['revision']=3;source['preferences']['raw']['preferred_skin_tone']=6
        m['selectionSource']=source
        picker_token=301 if custody and i>=3 else 300
        m['picker']=dict(generation=c['generation'],token=picker_token,ready=ready,notice=None if ready else 'The comment or emoji preferences changed. Close and reopen the picker.') if opened else None
        rect=copy.deepcopy(g['editor']['rect']);box=lambda:dict(visible=True,hit=True,rect=copy.deepcopy(rect))
        pills=[]
        for n in current:
            disabled=opened or rate or loading or success and i in (4,5) and n.startswith('+1::') or failure and i==2 and n=='eyes'
            pills.append(dict(name=n,count=1,own=True,pressed='true',disabled=disabled,ariaDisabled=str(disabled).lower(),tag='BUTTON',type='button',**box()))
        picker=None
        if opened:
            picker=dict(token=str(picker_token),ready=str(ready).lower(),query='' if success and i==1 else '+1',queryBox=box(),close=box(),dialog=box(),options=[dict(name='+1',disabled=not ready,selected='true',glyph='\U0001f44d',media=False,**box())],notice=None if ready else dict(text='The comment or emoji preferences changed. Close and reopen the picker.',**box()))
        if opened and ready:
            if success and i==1:
                picker['options'] += [dict(name=f'fixture-option-{n}',disabled=False,selected='false',glyph='x',media=False,**box()) for n in range(119)]
            picker['notice']=dict(text='120 of 1974 emoji' if success and i==1 else '1 of 1 emoji',**box())
        g['selection']=dict(picker=picker,pills=pills,add=dict(disabled=not available,type='button',**box()),status=None if status is None else dict(text=status,role='status',**box()))
        g['action']=dict(editorText=None if opened else g['action']['editorText']);g['notices']=[] if status is None else [status]
        for control in g['controls']:control['disabled']=(loading or rate) if control['id'] in ('slack-list-comments-next','slack-list-comments-refresh') else rate if control['id']=='slack-list-comment-send' else False
        picked=None
        if success:
            if i==1:inputs.append(dict(stage=stage,value=dict(ok=True,ts='1781524801.000001')))
            if i==2:inputs.append(dict(stage=stage,value=dict(ok=True,query='+1',names=['+1'])))
            if i==3:inputs.append(dict(stage=stage,value=dict(ok=True)))
            picked={4:'+1',7:'+1',8:CUSTOM,9:ALIAS}.get(i)
            if i==5:
                event_keys=['read','loading','error','page','pageMessages','live','accepted','editCustody','visibleTs','liveTs','acceptedTs','liveReactions','deleted','reactions','reveal']
                socket=[dict(stage=stage,team='T1',generation=7,parsed=True,frame=dict(type='reaction_added',user='U0',reaction=TONE,item=dict(type='message',channel='CCOMMENT01',ts='1781524801.000001')),
                             before={k:copy.deepcopy(rows[4]['model']['comments'][k]) for k in event_keys},after={k:copy.deepcopy(c[k]) for k in event_keys})]
                update=copy.deepcopy(raw);update['messages'][1]['reactions']=members(INITIAL+[TONE]);server.append(dict(stage=stage,raw=update))
            if i==6:releases.append(dict(stage=stage,value=dict(delivered=True,cancelledBeforeReply=False)))
            if i==8:server.append(dict(stage='exact-removed',raw=copy.deepcopy(raw)))
            if i==9:
                update=copy.deepcopy(raw);update['messages'][1]['reactions']=members(INITIAL+[CUSTOM]);server.append(dict(stage='custom-added',raw=update))
        elif failure:
            picked='eyes' if i in (1,2,4,6) else None
            if i==3:server.append(dict(stage=stage,raw=copy.deepcopy(future)))
            if i==7:releases.append(dict(stage=stage,value=dict(delivered=True,cancelledBeforeReply=False)))
        else:
            picked='+1' if i in (2,4) else 'eyes' if i==5 else None
            if i in (2,4,6):
                before=copy.deepcopy(rows[i-1]['model']['selectionSource']);after=copy.deepcopy(source)
                if i==2:
                    after['catalog']=None;changes.append(dict(stage=stage,kind='accepted-custom-metadata',name=ALIAS,value='alias:eyes',before=before,after=after))
                else:changes.append(dict(stage=stage,kind='pref_change',frame=dict(type='pref_change',name='preferred_skin_tone',value=5 if i==4 else 6),team='T1',generation=7,before=before,after=after))
            if i==6:releases.append(dict(stage=stage,value=dict(delivered=True,cancelledBeforeReply=False)))
        if picked:
            stale=custody and i in (2,4);token=str(picker_token if stale else 400+i)
            inputs.append(dict(stage=stage,value=dict(ok=True,name=picked,disabled=stale,token=token),**({'unchanged':True} if stale else {})))
        p.update(initialResponse=copy.deepcopy(raw),inputs=copy.deepcopy(inputs),releases=copy.deepcopy(releases),serverUpdates=copy.deepcopy(server),socket=copy.deepcopy(socket),sourceChanges=copy.deepcopy(changes))
        rows.append(row)
    receipt.update(case=case,stages=rows,final=rows[-1]['model'])
    return fixture,receipt


class SelectionReceipts(unittest.TestCase):
    def valid(self,case='reaction-selection',narrow=False):
        f,r=sample(case,narrow);return V['validate'](f,r,DIR,png=False)
    def reject(self,fn,i=4,case='reaction-selection'):
        f,r=sample(case);fn(r['stages'][i])
        with self.assertRaises(RuntimeError):V['validate'](f,r,DIR,png=False)
    def test_all_three_cases_both_widths(self):
        for c in V['STAGES']:
            for n in (False,True):self.valid(c,n)
    def test_runner_registration(self):
        for c in V['STAGES']:
            stages,validate=V['C']['case_contract'](c);self.assertEqual(tuple(stages),V['STAGES'][c]);f,r=sample(c);validate(f,r,DIR,png=False)
    def test_exact_preference_owned_name_not_raw_picker_base(self):
        for name in ('+1','thumbsup::skin-tone-4','+1::skin-tone-2'):
            self.reject(lambda r:r['model']['reactionLedger']['prepared'][0]['request']['args'].update(name=name))
    def test_choice_remove_is_not_family_remove(self):
        self.reject(lambda r:r['model']['reactionLedger']['issued'].append(copy.deepcopy(r['model']['reactionLedger']['issued'][-1])),7)
        self.reject(lambda r:r['model']['comments']['pageMessages'][1]['reactions'].pop(0),7)
    def test_custom_alias_removes_exact_custom_target(self):
        self.reject(lambda r:r['model']['reactionLedger']['prepared'][-1]['request']['args'].update(name=ALIAS),9)
        self.reject(lambda r:r['model']['reactionLedger']['prepared'][-1]['request'].update(method='reactions.add'),9)
    def test_original_socket_provenance_and_raw_page(self):
        self.reject(lambda r:r['proof']['socket'][0].update(generation=6),5)
        self.reject(lambda r:r['model']['comments']['page']['messages'][1].update(reactions=[]),5)
    def test_ack_does_not_erase_socket(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][0].update(socketObserved=False,published=True),6)
    def test_independent_complete_preferences_required(self):
        self.reject(lambda r:r['model']['selectionSource']['original']['raw'].update(prefs={}))
        self.reject(lambda r:r['model']['selectionSource']['preferences'].update(loaded=False))
    def test_catalog_uses_local_ticket_string_and_strict_lineage(self):
        for token in (200, True, '', '0/local', '01/local', '200/remote', '200/local/extra', '18446744073709551616/local'):
            self.reject(lambda r:r['model']['selectionSource']['catalog'].update(token=token),1)
        self.reject(lambda r:r['model']['selectionSource']['catalog'].update(current=False),1)
        self.reject(lambda r:r['model']['selectionSource'].update(catalog=None),1)
        self.reject(lambda r:r['model']['selectionSource']['catalog'].update(token='199/local'),2,'reaction-selection-custody')
    def test_retained_native_normal_b_complete_receipt(self):
        path=Path(__file__).resolve().parents[1]/'tmp/list-comments-reaction-selection-20261019/selection-normal-b/slack-list/slack-list.json'
        if not path.is_file():self.skipTest('Local retained native receipt is absent')
        outer=json.loads(path.read_text(encoding='utf-8'));receipt=outer['terminalReceipts'][0]
        V['validate'](outer['fixture'],receipt,outer['readyDirectory'],dimensions=outer['expectedPngDimensions'])
    def test_ready_status_is_exact_result_count_not_stale_warning(self):
        self.reject(lambda r:r['geometry']['selection']['picker'].update(notice=None),1)
        self.reject(lambda r:r['geometry']['selection']['picker']['notice'].update(text='1 of 1 emoji'),1)
        self.reject(lambda r:r['geometry']['selection']['picker']['notice'].update(text='The comment or emoji preferences changed. Close and reopen the picker.'),2)
    def test_cancel_has_no_dispatch(self):
        self.reject(lambda r:r['model']['reactionLedger']['prepared'].append({'method':'reactions.add'}),3)
    def test_same_list_draft_node_journal_even_under_picker(self):
        self.reject(lambda r:r['geometry']['editor'].update(sameNode=False),1)
        self.reject(lambda r:r['proof']['journal'].update(blocks=[]),2)
        self.reject(lambda r:r['model']['detail'].update(record='OTHER'),1)
    def test_closed_picker_requires_real_text_line(self):
        self.reject(lambda r:r['geometry']['action']['editorText'].update(visibleLines=0,ok=False),3)
    def test_closed_controls_must_be_hit_testable(self):
        self.reject(lambda r:r['geometry']['selection']['add'].update(hit=False),3)
        self.reject(lambda r:r['geometry']['selection']['pills'][0].update(hit=False),3)
    def test_search_and_cancel_original_token(self):
        self.reject(lambda r:r['model']['picker'].update(token=999),2)
        self.reject(lambda r:r['geometry']['selection']['picker'].update(query='eyes'),2)
    def test_unknown_not_rejected_or_replayed(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][1].update(phase='rejected'),2,'reaction-selection-failures')
        self.reject(lambda r:r['model']['reactionLedger']['issued'].append(copy.deepcopy(r['model']['reactionLedger']['issued'][-1])),3,'reaction-selection-failures')
    def test_absence_needs_full_true_response(self):
        self.reject(lambda r:r['model']['reactionLedger'].update(fullReads=[]),3,'reaction-selection-failures')
        self.reject(lambda r:r['model']['reactionLedger']['fullReads'][0]['request']['args'].update(full='false'),3,'reaction-selection-failures')
        self.reject(lambda r:r['model']['reactionActions']['attempts'][1].update(raw={'ok':True}),3,'reaction-selection-failures')
    def test_issued429_preserves_typed_disposition(self):
        self.reject(lambda r:r['model']['reactionLedger']['outcomes'][2].update(disposition='not-requested'),4,'reaction-selection-failures')
        self.reject(lambda r:r['model']['reactionActions']['attempts'][2].update(phase='unknown'),4,'reaction-selection-failures')
    def test_actual_cooldown_retired_before_retry(self):
        self.reject(lambda r:r['model']['comments'].update(retryAt=9999999999),5,'reaction-selection-failures')
    def test_catalog_stale_options_disabled_and_original_token(self):
        self.reject(lambda r:r['geometry']['selection']['picker']['options'][0].update(disabled=False),2,'reaction-selection-custody')
        self.reject(lambda r:r['model']['picker'].update(ready=True),2,'reaction-selection-custody')
    def test_actual_custom_alias_revision_custody(self):
        self.reject(lambda r:r['model']['selectionSource']['custom'][ALIAS].update(value='alias:wave'),2,'reaction-selection-custody')
        self.reject(lambda r:r['model']['selectionSource'].update(customRevision=2),2,'reaction-selection-custody')
    def test_pref_frame_has_original_team_socket_and_payload(self):
        self.reject(lambda r:r['proof']['sourceChanges'][-1].update(team='OTHER'),4,'reaction-selection-custody')
        self.reject(lambda r:r['proof']['sourceChanges'][-1]['frame'].update(value=6),4,'reaction-selection-custody')
    def test_preflight_retirement_never_dispatches(self):
        self.reject(lambda r:r['model']['reactionLedger']['issued'].append({'method':'reactions.remove'}),6,'reaction-selection-custody')
        self.reject(lambda r:r['model']['reactionActions']['attempts'][0].update(phase='confirmed',raw={'ok':True}),6,'reaction-selection-custody')
    def test_delivered_preflight_not_falsely_called_cancelled(self):
        self.reject(lambda r:r['proof']['releases'][-1]['value'].update(delivered=False,cancelledBeforeReply=True),6,'reaction-selection-custody')
    def test_no_comment_post_or_live_network(self):
        self.reject(lambda r:r['model']['composeLedger']['posts'].append({}))
        self.reject(lambda r:r['model'].update(networkCalls=1))


if __name__=='__main__':unittest.main()
