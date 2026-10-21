"""Portable receipts and adversarial checks for actual closed pill-addition schemas."""
import copy
import runpy
import unittest
from pathlib import Path
V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_pill_additions.py')))
BASE=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_reaction_selection.py')))
DIR=Path('/private/pill-additions')
RAW=['wave','wave::skin-tone-3','handshake','handshake::skin-tone-2-6','thumbsup','+1']
NAMES=['wave::skin-tone-4',RAW[1],'handshake::skin-tone-4',RAW[3],'thumbsup::skin-tone-4','+1::skin-tone-4']
PREF=dict(preferred_skin_tone=4,emoji_mode='native',emoji_use={},frecency='{}')
REPLY=dict(ok=True,prefs=PREF,fixture_opaque='original cold preference response')
GLYPHS=['\U0001f44b','\U0001f44b\U0001f3fc','\U0001f91d','\U0001faf1\U0001f3fb\u200d\U0001faf2\U0001f3ff','\U0001f44d','\U0001f44d']
GLYPHS=dict(zip(RAW+NAMES,GLYPHS+['\U0001f44b\U0001f3fd',GLYPHS[1],'\U0001f91d\U0001f3fd',GLYPHS[3],'\U0001f44d\U0001f3fd','\U0001f44d\U0001f3fd']))

def rx(owned=None):
    result=[dict(name=n,count=1,users=['U1']) for n in RAW]
    if owned in RAW:result[RAW.index(owned)].update(count=2,users=['U1','U0'])
    elif owned:result.append(dict(name=owned,count=1,users=['U0']))
    return result


def sample(case='pill-additions',narrow=False):
    f,r=BASE['sample'](narrow=narrow);first=copy.deepcopy(r['stages'][0]);base=first['model'];custody=case.endswith('custody')
    raw=copy.deepcopy(base['comments']['page']);raw['messages'][1].pop('selection_fixture_opaque');raw['messages'][1].update(reactions=rx(),pill_additions_opaque={'keep':'original raw names and independent membership'})
    typed=copy.deepcopy(base['comments']['pageMessages']);typed[1].pop('selection_fixture_opaque');typed[1].update(reactions=rx(),pill_additions_opaque={'keep':'original raw names and independent membership'})
    target=dict(document=base['initialDocument'],record=base['records']['R_LIST_000'],channel='CCOMMENT01',root='1781524800.000001')
    rows=[];inputs=[];releases=[];updates=[];events=[];changes=[]
    for i,stage in enumerate(V['STAGES'][case]):
        row=copy.deepcopy(first);row.update(stage=stage,capture=str(DIR/f'{f}-{stage}.png'));m,g,p=row['model'],row['geometry'],row['proof'];c=m['comments']
        for key in ('selectionSource','selectionProbe'):m.pop(key,None)
        if custody:
            reads=[4,4,4,4,8,8,12,12,16,20][i];posts=int(i>=8);phases=[[],[],[],[],['preparing'],['not-sent'],['not-sent','preparing'],['not-sent']*2,['not-sent','not-sent','unknown'],['not-sent','not-sent','observed']][i]
            prefcount=[0,1,2,2,3,3,3,3,3,3][i];prefdone=[0,1,1,2,3,3,3,3,3,3][i];preflight=[0,0,0,0,1,1,2,2,3,3][i];addition='failed' if i in (1,3) else 'loading' if i==2 else None
            owned=RAW[1] if i==9 else None;targetname=RAW[1];serial=[10,10,10,10,20,20,30,30,40,50][i]
        else:
            posts=max(0,i-1);reads=4+posts*4;phases=['confirmed']*posts;prefcount=int(i>0);prefdone=int(i>1);preflight=posts;addition='loading' if i==1 else None
            owned=NAMES[i-2] if i>1 else None;targetname=owned or RAW[0];serial=10 if i<2 else 20+10*(i-2)
        current=rx(owned);rawpage=copy.deepcopy(raw)
        if custody and i==9:rawpage['messages'][1]['reactions']=copy.deepcopy(current)
        nowtyped=copy.deepcopy(typed);nowtyped[1]['reactions']=copy.deepcopy(current);short=[dict(name=x['name'],count=x['count']) for x in current]
        c.update(page=rawpage,pageMessages=nowtyped,reactions=[dict(ts='1781524801.000001',reactions=short)],read=serial,loading=custody and i in (4,6),retryAt=0,retryAfter=0,error=None)
        g['rows'][0]['reactions']=copy.deepcopy(short);m['picker']=None
        actions=[]
        for j,phase in enumerate(phases):
            observed=phase=='observed';confirmed=phase=='confirmed'
            actions.append(dict(id=100+j,target=copy.deepcopy(target),ts='1781524801.000001',name=RAW[1] if custody else NAMES[j],user='U0',adding=True,phase=phase,raw={'ok':True} if confirmed else None,socketObserved=False,socketRevision=0,canonicalObserved=observed,readbackResolved=observed,published=confirmed,retryAt=0,fullReadback=None))
        m['reactionActions']=dict(attempts=actions,pending=sum(p in ('preparing','unknown') for p in phases))
        indices=([2] if posts else []) if custody else range(posts)
        wires=[dict(id=100+j,user='U0',request=dict(method='reactions.add',args=dict(channel='CCOMMENT01',timestamp='1781524801.000001',name=RAW[1] if custody else NAMES[j]))) for j in indices]
        m['reactionLedger']=dict(prepared=wires,issued=copy.deepcopy(wires),completed=posts,held=False,networkCalls=0,fullReads=[],outcomes=[dict(id=x['id'],disposition='unconfirmed' if custody else 'response',raw=None if custody else {'ok':True}) for x in wires])
        template=base['ledger']['requests'];m['ledger'].update(requests=[dict(copy.deepcopy(q),serial=b*4+j+1) for b in range(reads//4) for j,q in enumerate(template)],completed=reads,cancelled=0,held=False,replies=reads//4)
        m['composeLedger']['preflights']=[dict(method='conversations.genericInfo',args={'updated_channels':'{"CCOMMENT01":0}'})]*preflight;m['composeLedger']['held']=custody and i in (4,6)
        pref=dict(loaded=False,revision=0,raw={})
        if custody and i==3:pref=dict(loaded=False,revision=1,raw={'preferred_skin_tone':5})
        elif custody and i>=4:
            pref=dict(loaded=True,revision=2+int(i>=7),raw=copy.deepcopy(PREF))
            if i>=7:pref['raw']['preferred_skin_tone']=5
        elif not custody and i>=2:pref=dict(loaded=True,revision=1,raw=copy.deepcopy(PREF))
        modes=(['Failure','Held','Success'] if custody else ['Held'])[:prefcount]
        outcomes=[]
        for j in range(prefdone):
            failed=custody and j==0
            adopted=(dict(loaded=False,revision=0,raw={}) if j==0 else dict(loaded=False,revision=1,raw={'preferred_skin_tone':5}) if j==1 else dict(loaded=True,revision=2,raw=copy.deepcopy(PREF))) if custody else dict(loaded=True,revision=1,raw=copy.deepcopy(PREF))
            outcomes += [dict(kind='response',raw=None if failed else copy.deepcopy(REPLY),failed=failed),dict(kind='adoption',failed=failed,preferences=adopted)]
        m['additionSource']=dict(preferences=pref,customRevision=2+int(custody and i>=5),catalog=None if i==0 or custody and i==5 else dict(token='2/local' if custody and i>=6 else '1/local',current=True),requests=[dict(method='users.prefs.get',args={'prefs':'preferred_skin_tone,emoji_mode,emoji_use,frecency'},mode=mode) for mode in modes],outcomes=outcomes,held=(i==2 if custody else i==1),completed=prefdone,cancelled=0)
        status=None
        if addition:status='Loading emoji and preferences. Your draft is unchanged.' if addition=='loading' else 'Emoji details could not load. Click the reaction to retry. Your draft is unchanged.' if i==1 else 'The comment or emoji details changed. Click the reaction to retry. Your draft is unchanged.'
        elif phases:status={'confirmed':'Reaction updated.','preparing':'Checking the current reaction. Your draft is unchanged.','not-sent':'The reaction was not sent. Refresh comments before trying again.','unknown':'Reaction delivery is unconfirmed. Refresh comments to check before trying again.','observed':'The requested reaction state was observed in refreshed comments.'}[phases[-1]]
        m['addition']=dict(token=51 if custody and i>=2 else 50,generation=c['generation'],ts='1781524801.000001',requested=RAW[1] if custody else RAW[0],phase=addition,notice=status) if addition else None
        m['actionProbe']=dict(status=status);blocked=addition=='loading' or 'preparing' in phases
        probe=[dict(name=x['name'],count=x['count'],own='U0' in x['users'],glyph=GLYPHS[x['name']],available=not(blocked or custody and i==8 and x['name'].startswith('wave'))) for x in current]
        m['additionProbe']=probe;rect=copy.deepcopy(g['editor']['rect']);pills=[dict(name=x['name'],count=x['count'],own=x['own'],pressed=str(x['own']).lower(),disabled=not x['available'],ariaDisabled=str(not x['available']).lower(),tag='BUTTON',type='button',visible=True,hit=True,rect=copy.deepcopy(rect)) for x in probe]
        g.pop('selection',None);g['additions']=dict(target=targetname,pills=pills,picker=False,tools=0,status=None if status is None else dict(text=status,role='status',visible=True,rect=copy.deepcopy(rect)));g['notices']=[] if status is None else [status]
        for ctl in g['controls']:ctl['disabled']=custody and i in (4,6) and ctl['id'] in ('slack-list-comments-next','slack-list-comments-refresh')
        p['pillViews']=[dict(name=x['name'],ok=True,hit=True,rect=copy.deepcopy(rect)) for x in current]
        clicked=RAW[1] if custody and i in (1,2,4,6,8) else RAW[0] if not custody and i==1 else RAW[i-2] if not custody and i>=3 else None
        if clicked:inputs.append(dict(stage=stage,value=dict(ok=True,name=clicked,picker=False)))
        if not custody and i==2 or custody and i==3:releases.append(dict(stage=stage,kind='preferences',value=dict(delivered=True,cancelledBeforeReply=False,raw=copy.deepcopy(REPLY))))
        if custody and i in (5,7):releases.append(dict(stage=stage,kind='preflight',value=dict(delivered=True,cancelledBeforeReply=False)))
        if not custody and i>=3:
            updates.append(dict(stage=stage,raw=copy.deepcopy(raw)))
            before={k:copy.deepcopy(rows[i-1]['model']['comments'][k]) for k in V['S']['EVENT_KEYS']};after=copy.deepcopy(before);after['pageMessages'][1]['reactions']=rx();after['reactions']=[dict(ts='1781524801.000001',reactions=[dict(name=n,count=1) for n in RAW])]
            events.append(dict(stage=stage,team='T1',generation=7,parsed=True,frame=dict(type='reaction_removed',user='U0',reaction=NAMES[i-3],item=dict(type='message',channel='CCOMMENT01',ts='1781524801.000001')),before=before,after=after))
        if custody and i==9:updates.append(dict(stage=stage,raw=copy.deepcopy(rawpage)))
        if custody and i in (3,5,7):
            before=copy.deepcopy(rows[i-1]['model']['additionSource']);after=copy.deepcopy(before)
            if i==5:
                after['customRevision']+=1;after['catalog']=None;change=dict(stage=stage,kind='accepted-custom-metadata',name='fixture_addition_changed',value='alias:eyes',before=before,after=after)
            else:
                after['preferences']=copy.deepcopy(pref);change=dict(stage=stage,kind='pref_change',team='T1',generation=7,frame=dict(type='pref_change',name='preferred_skin_tone',value=5),before=before,after=after)
            changes.append(change)
        if custody and i>=2:p['throttleWait']=dict(ready=True,elapsedMillis=24010)
        p.update(initialResponse=copy.deepcopy(raw),inputs=copy.deepcopy(inputs),releases=copy.deepcopy(releases),serverUpdates=copy.deepcopy(updates),socket=copy.deepcopy(events),sourceChanges=copy.deepcopy(changes));rows.append(row)
    r.update(case=case,stages=rows,final=rows[-1]['model']);return f,r


class PillAdditions(unittest.TestCase):
    def reject(self,change,i=2,case='pill-additions'):
        f,r=sample(case);change(r['stages'][i])
        with self.assertRaises(RuntimeError):V['validate'](f,r,DIR,png=False)
    def test_cases_widths(self):
        for c in V['STAGES']:
            for n in (False,True):
                f,r=sample(c,n);V['validate'](f,r,DIR,png=False)
    def test_registration(self):
        for c in V['STAGES']:
            stages,validate=V['C']['case_contract'](c);self.assertEqual(tuple(stages),V['STAGES'][c]);f,r=sample(c);validate(f,r,DIR,png=False)
    def test_cold_acquisition_is_not_seeded(self):
        self.reject(lambda r:r['model']['additionSource']['preferences'].update(loaded=True),0)
        self.reject(lambda r:r['model']['additionSource'].update(catalog={'token':'1/local','current':True}),0)
    def test_loading_catalog_may_still_be_building(self):
        f,r=sample();r['stages'][1]['model']['additionSource']['catalog']=None;V['validate'](f,r,DIR,png=False)
    def test_preference_factory_and_original_response(self):
        self.reject(lambda r:r['model']['additionSource']['requests'][0].update(method='users.prefs.set'))
        self.reject(lambda r:r['model']['additionSource']['outcomes'][0]['raw']['prefs'].update(preferred_skin_tone=5))
    def test_preference_missing_and_partial_not_loaded(self):
        self.reject(lambda r:r['model']['additionSource'].update(outcomes=[]))
        self.reject(lambda r:r['model']['additionSource']['preferences'].update(loaded=True),3,'pill-additions-custody')
    def test_each_exact_wire_and_no_inverse(self):
        for i,bad in ((2,'wave'),(3,'wave::skin-tone-4'),(4,'handshake'),(5,'handshake::skin-tone-4'),(6,'+1::skin-tone-4'),(7,'thumbsup::skin-tone-4')):
            self.reject(lambda r:r['model']['reactionLedger']['issued'][-1]['request']['args'].update(name=bad),i)
        self.reject(lambda r:r['model']['reactionLedger']['issued'][0]['request'].update(method='reactions.remove'))
    def test_duplicate_wire_and_chat_forbidden(self):
        self.reject(lambda r:r['model']['reactionLedger']['issued'].append(copy.deepcopy(r['model']['reactionLedger']['issued'][0])))
        self.reject(lambda r:r['model']['composeLedger']['posts'].append({}))
    def test_socket_retains_ack_and_raw_page(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][0].update(raw=None),3)
        self.reject(lambda r:r['model']['comments']['page']['messages'][1].update(reactions=[]),3)
    def test_socket_frame_and_independent_future_provenance(self):
        self.reject(lambda r:r['proof']['socket'][0].update(generation=6),3)
        self.reject(lambda r:r['proof']['socket'][0]['frame'].update(user='U1'),3)
        self.reject(lambda r:r['proof']['serverUpdates'][0]['raw']['messages'][1].update(reactions=[]),3)
    def test_no_picker_and_generic_tools(self):
        self.reject(lambda r:r['geometry']['additions'].update(picker=True))
        self.reject(lambda r:r['geometry']['additions'].update(tools=1))
    def test_native_draft_and_journal_unchanged(self):
        self.reject(lambda r:r['geometry']['editor'].update(sameNode=False),1)
        self.reject(lambda r:r['proof']['journal'].update(blocks=[]))
        self.reject(lambda r:r['geometry']['action']['editorText'].update(ok=False,visibleLines=0))
    def test_every_pill_actual_scroll_hit_and_selected_visibility(self):
        self.reject(lambda r:r['proof']['pillViews'][0].update(hit=False))
        self.reject(lambda r:r['geometry']['additions']['pills'][-1].update(visible=False))
    def test_admission_loading_unknown_and_status(self):
        self.reject(lambda r:r['model']['additionProbe'][0].update(available=True),1)
        self.reject(lambda r:r['model']['additionProbe'][0].update(available=True),8,'pill-additions-custody')
        self.reject(lambda r:r['geometry']['additions']['status'].update(text='Done'),1)
    def test_retired_source_does_not_dispatch(self):
        for i in (3,5,7):self.reject(lambda r:r['model']['reactionLedger']['issued'].append({}),i,'pill-additions-custody')
    def test_changed_source_and_delivered_original_reply(self):
        self.reject(lambda r:r['proof']['sourceChanges'][0]['frame'].update(value=4),3,'pill-additions-custody')
        self.reject(lambda r:r['proof']['releases'][0]['value'].update(delivered=False),3,'pill-additions-custody')
        self.reject(lambda r:r['model']['additionSource']['catalog'].update(token='1/local'),6,'pill-additions-custody')
    def test_no_throttle_reset_or_auto_request(self):
        self.reject(lambda r:r['proof']['throttleWait'].update(ready=False),2,'pill-additions-custody')
        self.reject(lambda r:r['model']['additionSource']['requests'].append({}),3,'pill-additions-custody')
    def test_unknown_positive_page_keeps_original_transport_result(self):
        self.reject(lambda r:r['model']['reactionActions']['attempts'][2].update(raw={'ok':True}),9,'pill-additions-custody')
        self.reject(lambda r:r['model']['reactionLedger']['outcomes'][0].update(disposition='response'),9,'pill-additions-custody')
        self.reject(lambda r:r['model']['reactionLedger']['fullReads'].append({}),9,'pill-additions-custody')
        self.reject(lambda r:r['model']['comments']['page']['messages'][1]['reactions'][1].update(users=['U1']),9,'pill-additions-custody')

if __name__=='__main__':unittest.main()
