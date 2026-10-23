"""Portable synthetic mutations; no native launch and no runtime-derived expectations."""
import copy
import json
from pathlib import Path
import runpy
import unittest
import slack_list_plain_channel_save as V

B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_own_edit_plain_emoji.py')))
D=B['D']


def options(query):
    return dict(rawQuery=query,formattedQuery=query,locale='en-US',query=dict(normalized=query,matcherNormalized=query),
        options=dict(ticket='fixture-original',key='ml_fuzzy_match_beyond_word_boundaries_v2',search='',forced=None,storage=None,storageFailed=False,group='noGroup',enabled=False),eligibility=None,
        fields=[dict(text=v,lowered=v,localeLowered=v) for v in sorted({'',query,'project-harbor'})],classes={'project-harbor':0})

def selectors(matched):
    return dict(selfId=V.O['SELF'],teamId='T1',enterprise=False,metadata={'CLISTLOCAL':dict(lastRead=None,latest=None,starred=False,unread=False,draft=False,localDraft=False,priority=None)} if matched else {},
        navigationHistory={},experiments=dict(autocomplete_affinity_score=None,autocomplete_affinity_ml_model=None),offline={'channel':{'CLISTLOCAL':{'team_ctr_90dd':0.148,'user_ctr_30dd':0.148}} if matched else {}},entityCtr={'channel':0.148},channelPriority={'CLISTLOCAL':0.0} if matched else {})

def sample(narrow=False):
    retired=False;cancelled=False
    fixture,base,_=B['sample'](narrow=narrow)
    initial=copy.deepcopy(base['stages'][0]['model']['comments']);first=base['stages'][0]
    journal=first['journal'];scope=journal['scope'];seed=V.seed_value(initial);rect=B['B']['B']['rect']()
    case=V.CASES[int(retired)];rows=[];requests=[];pages=[];owner=dict(account=initial['account'],epoch=initial['epoch'],team='T1',user=V.O['SELF'])
    def button(id):return dict(ok=True,proof=dict(id=id,text=id,disabled=False,visible=True,hit=True,rect=rect))
    def ticket(ops):return dict(ops=copy.deepcopy(ops),mode='PlainText',attachmentEffects='none-v1',revision=1,generation='78',composing=False,pending=False)
    att=dict(id='00000000-0000-4000-8000-000000000088',capture='00000000-0000-4000-8000-000000000003',phase='Pending',raw=None,readback=None,retry_at=0,
        record=dict(version=2,seed=seed,blocks=V.FUTURE['messages'][1]['blocks'],text=V.WIRE_TEXT,plain_delta=dict(version=1,ops=V.CHANGED_OPS)))
    def inv(active=False,ops=None,pending=False):
        v=dict(scope=copy.deepcopy(scope),ts=V.O['TS'],revision=2 if active else 0,capture='00000000-0000-4000-8000-000000000003' if active else '',seed=copy.deepcopy(seed) if active else None,
            blocks=[],attempt=copy.deepcopy(att) if pending else None,retry_at=0)
        if active:v['plain_delta']=dict(version=1,ops=copy.deepcopy(ops))
        return v
    original=dict(copy.deepcopy(first['proof']['originalDisk']),edit=inv())
    p=dict(source=dict(received=V.INITIAL['messages'][1],boot=V.O['boot']()),replySeed=first['proof']['replySeed'],replyBlocks=first['proof']['replyBlocks'],opens=[],inputs=[],actions=[],storageControls=[],originalDisk=original)
    mount=dict(ok=True,scope='list-edit:100:78',generation='78',mount='plain-mount-78',delta=ticket(V.INITIAL_OPS))
    exposed=dict(visible=True,hit=True,rect=rect)
    for i,name in enumerate(V.STAGES[case]):
        failed=name in ('save-refused','source-ready');held=False;saving=name=='saving';accepted=name in ('saved','readback');replaced=name=='replaced';editing=name in ('editing','source-pending','save-refused','source-ready','saving');busy=held or saving
        notice=V.REFUSAL if failed else V.CHECKING if held else V.T['NOTICES'][9] if saving else None
        row=copy.deepcopy(first);row.update(stage=name,capture=str(D/f'{fixture}-{name}.png'),plain=copy.deepcopy(first['plain']))
        m=row['model'];c=m['comments'];n=V.READS[case][i]
        c['comments'].update(loading=busy,page=copy.deepcopy(V.FUTURE if name=='readback' else V.INITIAL),pageMessages=copy.deepcopy((V.FUTURE if accepted else V.INITIAL)['messages']))
        c['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(initial['ledger']['requests']*(n//4))],completed=n,cancelled=0,held=False,replies=n//4)
        f=m['fixture'];sent=saving or accepted
        f.update(received=copy.deepcopy(V.INITIAL['messages'][1]),policy=[dict(request=q,response=r) for _ in range(V.POLICIES[case][i]) for q,r in V.policy_cycle()],
            prepared=[V.O['wire'](V.BODY)] if sent else [],updates=[V.O['wire'](V.BODY)] if sent else [],responses=[dict(disposition='accepted',raw=V.ACK)] if sent else [],futureRows=[V.FUTURE['messages'][1]] if sent else [],held=saving,releases=[V.O['RELEASE']] if accepted else [],published=False,publishedRow=None)
        e=m['edit'];e.update(active=None,opening=False,events=0,retired=[],attempts=[dict(id=123,ts=V.O['TS'],blocks=V.FUTURE['messages'][1]['blocks'],phase='sent' if saving else 'confirmed',receipt=None if saving else V.ACK,retryAt=0)] if sent else [])
        a=dict(scope=mount['scope'],generation=78,ts=V.O['TS'],current=True,blocks=[],plainDelta=dict(version=1,ops=copy.deepcopy(V.CHANGED_OPS)),nativePending=False,revision=2,
            busy=busy,notice=notice,composing=False,nativeReady=True,captureFailed=False,source=V.INITIAL['messages'][1],preserved=V.INITIAL['messages'][1]['blocks'][1:],attempt=123 if saving else None)
        e['active']=copy.deepcopy(a) if editing else None
        m['capture']=dict(scope=mount['scope'],generation=78,mount=mount['mount'],failed=False,canSave=not busy,canCancel=not busy,canCheck=False,canRetry=False,busy=busy,finishing=False) if editing else None
        row['editInventory']=inv(editing,V.CHANGED_OPS,saving)
        if i==1:
            original_active=dict(copy.deepcopy(a),plainDelta=dict(version=1,ops=V.INITIAL_OPS))
            p['opens'].append(dict(action=button(''),active=original_active,mount=mount,inventory=inv(True,V.INITIAL_OPS)))
            p['inputs'].append(dict(native=dict(ok=True,action='native-select-all-insertText',text=V.TEXT,sameNode=True,before=ticket(V.INITIAL_OPS),after=ticket(V.CHANGED_OPS),exposed=exposed),inventory=inv(True,V.CHANGED_OPS)))
        while len(requests)<n//4:
            j=len(requests);future=name=='readback' and j==n//4-1;text=V.FUTURE_BYTES if future else V.INITIAL_BYTES;raw=json.loads(text)
            requests.append(dict(serial=(j+1)*4,readSerial=j+1,method='conversations.replies',args=copy.deepcopy(initial['ledger']['requests'][3]['args']),variant='future' if future else 'initial',bytes=text,raw=raw,owner=dict(serial=initial['comments']['generation'],account=initial['account'],epoch=initial['epoch'])))
            pages.append(dict(raw=raw,messages=raw['messages'],witness=dict(messageIndex=1,raw=raw['messages'][1],styleKeys=None if future else ['italic','bold']),current=True,requestSerial=(j+1)*4,readSerial=j+1))
        entry=dict(raw=c['comments']['page'],messages=c['comments']['pageMessages'],witness=dict(messageIndex=1,raw=c['comments']['page']['messages'][1],styleKeys=None if name=='readback' else ['italic','bold']))
        row['ordered']=dict(initialBytes=V.INITIAL_BYTES,futureBytes=V.FUTURE_BYTES,requests=copy.deepcopy(requests),pages=copy.deepcopy(pages),entryPage=entry,completed=n//4,cancelled=0,held=False,heldOwnerCurrent=None,releases=[],plainModeEnabled=True)
        source_row={'id':'CLISTLOCAL','name':'project-harbor','is_channel':True,'is_member':True,'fromAnotherTeam':False}
        source=dict(account=initial['account'],epoch=initial['epoch'],team='T1',user=V.O['SELF'],socket=8,
            current=i not in (2,3),pending=i in (2,3),rows=None if i in (2,3) else [source_row],
            conditional=dict(installed=True,drainHeld=i in (2,3),requestCount=int(i>=5),held=int(i>=5),managers=[dict(accountCurrent=True,active=True,worker=True,freshIds=[],pendingIds=[],nextWake=1792725218590)] if i>=5 else []))
        row['plain']['channelSource']=source
        lookup=[]
        if i>=5:
            for serial,query in enumerate(('project-harbor','not-found'),1):
                lookup.append(dict(serial=serial,observation=dict(lookup=dict(kind='channel',query=query),
                    requested={'query':query,'options':{'entities':{'channels':{'includePrivate':False,'includeFromAnotherTeam':True}},'limit':25,'sort':{'source':'texty-autocomplete'}}},
                    source=[source_row],options=options(query),selectors=selectors(serial==1),
                    results=[source_row] if serial==1 else [],sourceCurrent=True,rankCurrent=True)))
        row['plain']['channels']=dict(enabled=True,lookups=copy.deepcopy(lookup))
        row['plain']['completion']['seed']['boot']['channels'][0]['name']='project-harbor'
        if i>=5:
            row['plain']['completion']['requests']=[dict(serial=1,method='search.autocomplete.model',args={'_x_reason':'AutocompleteModelCache'})]
            row['plain']['completion']['responses']=[dict(serial=1,disposition='response',raw={'ok':True,'model':None})]
        if i==2:p['channelEvent']=dict(raw={'type':'channel_rename','channel':{'id':'CLISTLOCAL','name':'project-harbor'},'event_ts':'2000000001.0'},before=copy.deepcopy(rows[1]['plain']['channelSource']),originalCurrent=False,source=copy.deepcopy(source))
        if i==4:p['channelRelease']=copy.deepcopy(source)
        g=row['geometry'];g.update(editing=editing,captureFault=None,inertTextPaint=None,replyNotice=None,controls=[dict(id=id,text=id,disabled=disabled,visible=True,hit=True,rect=rect) for id,disabled in
            ([('slack-list-comment-edit-cancel',busy),('slack-list-comment-edit-save',busy)] if editing else [('slack-list-comments-back',False),('slack-list-comment-send',False)])],
            edit=dict(delta=ticket(V.CHANGED_OPS),text=V.TEXT,inputSameNode=True,sameNode=True,editable=not busy,rect=rect) if editing else None,notice=dict(text=notice,visible=True,hit=True,rect=rect) if notice else None)
        g['reply'].update(sameNode=True,hidden=editing,inert=editing,visible=not editing);g['row']['text']=(V.TEXT if accepted else V.INITIAL['messages'][1]['text'])+' Preserved original context'
        if busy:
            g['text']=copy.deepcopy(base['stages'][4]['geometry']['text']);g['inertTextPaint']=copy.deepcopy(base['stages'][4]['geometry']['inertTextPaint'])
        if name=='save-refused':p.update(failedSaveBefore=copy.deepcopy(rows[2]['model']),failedSaveAfter=copy.deepcopy(m),failedSave=dict(ok=True,action='original-keydown',key='Enter',keyCode=13,defaultPrevented=True,isTrusted=False,sameNode=True,before=ticket(V.CHANGED_OPS),exposed=exposed))
        if saving:
            p['enter']=dict(ok=True,action='original-keydown',key='Enter',keyCode=13,defaultPrevented=True,isTrusted=False,sameNode=True,before=ticket(V.CHANGED_OPS),exposed=exposed)
            p['busyControls']=dict(ok=True,saveDisabled=True,cancelDisabled=True)
        if saving:p.update(doubleSave=dict(ok=True,disabled=True))
        if accepted:p['release']=V.O['RELEASE']
        if name=='readback':p['actions'].append(button('slack-list-comments-refresh'))
        row['proof']=copy.deepcopy(p);rows.append(row)
    p['closedDisk']=dict(copy.deepcopy(original),edit=copy.deepcopy(rows[-1]['editInventory']))
    receipt=dict(ok=True,case=case,fixture=fixture,readiness=base['readiness'],stages=rows,proof=p,final=copy.deepcopy(rows[-1]['model']),initialRecords=base['initialRecords'],noLiveNetwork=True)
    closed=dict(account=initial['account'],inventories=[journal],editInventories=[rows[-1]['editInventory']],editHistory=[] if retired else [dict(copy.deepcopy(att),phase='Confirmed',raw=V.ACK)],contentSha256='a'*64)
    return copy.deepcopy((fixture,receipt,closed))


class ChannelTests(unittest.TestCase):
    def reject(self,change):
        f,r,_=sample();change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):
            V.validate(f,r,D,png=False)
    def test_full_both_widths_and_closed_database(self):
        for narrow in (False,True):
            f,r,c=sample(narrow);V.validate(f,r,D,png=False);V.validate_closed(c,r)
    def test_future_original_bytes(self):
        p=Path(__file__).parents[1]/'src/desktop/slack_lists/comments/fixture/plain/channel-future.json'
        self.assertEqual(p.read_bytes().decode('utf-8'),V.FUTURE_BYTES)
    def test_old_canonical_handle_refused(self):self.reject(lambda r:r['stages'][2]['proof']['channelEvent'].update(originalCurrent=True))
    def test_pending_ready_refused(self):self.reject(lambda r:r['stages'][3]['plain']['channelSource'].update(current=True))
    def test_socket_reseed_refused(self):self.reject(lambda r:r['stages'][4]['plain']['channelSource'].update(socket=9))
    def test_foreign_team_refused(self):self.reject(lambda r:r['stages'][4]['plain']['channelSource'].update(team='T2'))
    def test_no_match_not_missing(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][1]['observation'].pop('results'))
    def test_no_match_not_channel(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][1]['observation'].update(results=[{'id':'CLISTLOCAL'}]))
    def test_stale_lookup_source(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation'].update(sourceCurrent=False))
    def test_stale_rank(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation'].update(rankCurrent=False))
    def test_foreign_raw_source(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation']['source'][0].update(name='foreign'))
    def test_requested_private_admission(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation']['requested']['options']['entities']['channels'].update(includePrivate=True))
    def test_requested_limit_changed(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation']['requested']['options'].update(limit=50))
    def test_native_locale_changed(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation']['options'].update(locale='tr'))
    def test_fabricated_option_group(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation']['options']['options'].update(group='on'))
    def test_rank_metadata_changed(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation']['selectors']['metadata']['CLISTLOCAL'].update(draft=True))
    def test_no_empty_negative_before_source(self):self.reject(lambda r:r['stages'][3]['plain']['channels']['lookups'].append({}))
    def test_unexpected_network_factory(self):self.reject(lambda r:r['stages'][5]['plain']['completion']['requests'][0].update(method='channels.search'))
    def test_model_raw_changed(self):self.reject(lambda r:r['stages'][5]['plain']['completion']['responses'][0].update(raw={'ok':False}))
    def test_failure_no_attempt(self):self.reject(lambda r:r['stages'][3]['model']['edit']['attempts'].append({}))
    def test_failure_native_same_node(self):self.reject(lambda r:r['stages'][3]['geometry']['edit'].update(sameNode=False))
    def test_failure_reply_journal(self):self.reject(lambda r:r['stages'][3]['journal'].update(blocks=[]))
    def test_failure_original_delta(self):self.reject(lambda r:r['stages'][3]['editInventory']['plain_delta'].update(ops=[]))
    def test_pending_double_save(self):self.reject(lambda r:r['stages'][5]['proof']['doubleSave'].update(disabled=False))
    def test_channel_wire_identity(self):self.reject(lambda r:r['stages'][5]['model']['edit']['attempts'][0]['blocks'][0]['elements'][0]['elements'][1].update(channel_id='CFOREIGN1'))
    def test_context_suffix_preserved(self):self.reject(lambda r:r['stages'][5]['model']['edit']['attempts'][0]['blocks'].pop())
    def test_closed_history_required(self):
        _,r,c=sample();c['editHistory']=[]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_premature_first_hit_refresh(self):self.reject(lambda r:r['stages'][1]['plain']['channelSource']['conditional'].update(requestCount=1,held=1))
    def test_extra_conditional_request(self):self.reject(lambda r:r['stages'][5]['plain']['channelSource']['conditional'].update(requestCount=2))
    def test_first_hit_must_remain_held(self):self.reject(lambda r:r['stages'][6]['plain']['channelSource']['conditional'].update(held=0))
    def test_late_manager_replaced(self):self.reject(lambda r:r['stages'][7]['plain']['channelSource']['conditional']['managers'][0].update(nextWake=1792725218591))
    def test_foreign_manager_account(self):self.reject(lambda r:r['stages'][5]['plain']['channelSource']['conditional']['managers'][0].update(accountCurrent=False))
    def test_missing_name_collation(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation']['options'].update(classes={}))
    def test_query_is_not_collation_name(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][1]['observation']['options']['classes'].update({'not-found':1}))
    def test_navigation_storage_array_is_not_projection(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation']['selectors'].update(navigationHistory=[]))
    def test_navigation_foreign_across_queries(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation']['selectors'].update(navigationHistory={'C12345678':{'ts':1,'position':1}}))
    def test_navigation_projection_fields(self):self.reject(lambda r:r['stages'][5]['plain']['channels']['lookups'][0]['observation']['selectors'].update(navigationHistory={'C12345678':{'timestamp':1,'position':1}}))
    def test_actual_nonempty_navigation_retained(self):
        f,r,_=sample()
        for row in r['stages'][5:]:
            for item in row['plain']['channels']['lookups']:
                item['observation']['selectors']['navigationHistory']={'C12345678':{'ts':123.5,'position':2}}
        V.validate(f,r,D,png=False)
    def test_original_closed_runner(self):
        h=runpy.run_path(str(Path(__file__).with_name('check-list-plain-channel-save-native.py')))['harness']()
        self.assertEqual(h['case_contract'](V.CASES[0])[0],V.STAGES[V.CASES[0]])

if __name__=='__main__':unittest.main()
