"""Synthetic protocol mutations only; these tests do not launch the native app."""
import copy,json,runpy,unittest
from pathlib import Path
import slack_list_own_edit_plain_emoji as V
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_edit_storage.py')))
D=Path('/private/plain')


def sample(storage=False,narrow=False):
    fixture,base,_=B['sample'](narrow);first=copy.deepcopy(base['stages'][0]);initial=first['model']['comments'];scope=first['journal']['scope'];journal=first['journal'];case=V.CASE
    initial['comments']['page']=copy.deepcopy(V.INITIAL);initial['comments']['pageMessages']=copy.deepcopy(V.INITIAL['messages']);ds=V.seed_value(initial);rect=B['B']['rect']()
    def native_button(id):return dict(ok=True,proof=dict(id=id,text=id,disabled=False,visible=True,hit=True,rect=rect))
    def ticket(ops,generation=78,rev=0):return dict(ops=copy.deepcopy(ops),mode='PlainText',attachmentEffects='none-v1',revision=rev,generation=str(generation),composing=False,pending=False)
    def inv(i,active=False,ops=None,pending=False):
        v=dict(scope=copy.deepcopy(scope),ts=V.O['TS'],revision=i,capture=f'00000000-0000-4000-8000-{i:012d}' if i else '',seed=copy.deepcopy(ds) if active else None,blocks=[],attempt=copy.deepcopy(attempt) if pending else None,retry_at=0)
        if active:v['plain_delta']=dict(version=1,ops=copy.deepcopy(ops))
        return v
    attempt=dict(id='00000000-0000-4000-8000-000000000088',capture='00000000-0000-4000-8000-000000000003',phase='Pending',raw=None,readback=None,retry_at=0,record=dict(version=2,seed=copy.deepcopy(ds),blocks=V.FUTURE['messages'][1]['blocks'],text=V.TEXT,plain_delta=dict(version=1,ops=V.CHANGED_OPS)))
    original=dict(path='/private/nonce/comments.sqlite',account=initial['account'],scope=scope,reply=journal,edit=inv(0))
    p=dict(source=dict(received=copy.deepcopy(V.INITIAL['messages'][1]),boot=V.O['boot']()),replySeed=first['proof']['replySeed'],replyBlocks=first['proof']['replyBlocks'],opens=[],inputs=[],actions=[],storageControls=[],originalDisk=original)
    user=dict(id=V.O['SELF'],name='fixture-self',team_id='T1',is_admin=True,profile={})
    boot=dict(ok=True,team=dict(id='T1',name='List completion'),self=user,users=[user],channels=[dict(id='CLISTLOCAL',name='project-local',team_id='T1',is_channel=True,is_member=True,updated=100,previous_names=[])],prefs=V.PREFS['prefs'],subteams={'self':[]})
    init=dict(ok=True,self=user,team=dict(id='T1',name='List completion'),workspaces=[{'id':'T1'},{'id':'TOTHER001'}],onboarding={'user_data':{'onboarding_complete':True}},user_date_created=123)
    plain=dict(enabled=True,storage=storage,noLiveNetwork=True,preferences=V.PREFS,canonical=dict(locale='en-US',membership=[],rows=[],current=True),completion=dict(enabled=True,named=True,protectedFixture=True,identityCurrent=True,account=initial['account'],epoch=initial['epoch'],team='T1',user=V.O['SELF'],requests=[],responses=[],releases=[],held=0,completed=0,cancelled=0,networkCalls=0,httpCapability=False,seed=dict(boot=boot,init=init,accepted=True)))
    rows=[];reqs=[];pages=[];exposed=dict(visible=True,hit=True,rect=rect);readset=initial['ledger']['requests']
    for i,name in enumerate(V.STAGES[case]):
        unavailable=i==3;editing=i in (1,3,4);saving=i==4;accepted=i>=5;locked=False;current_ops=V.UNKNOWN_OPS if unavailable else V.CHANGED_OPS;current_text=V.UNKNOWN if unavailable else V.TEXT;n=V.READS[case][i];open_count=0 if i==0 else 1 if storage or i<3 else 2;gen=77+open_count
        row=copy.deepcopy(first);row.update(stage=name,capture=str(D/f'{fixture}-{name}.png'),plain=copy.deepcopy(plain));
        count=int(i>=3);row['plain']['customCatalog']=dict(started=count,finished=count,active=0,cancelled=0,pending=[],contextsCurrent=[True]*count,networkCalls=0,httpCapability=False,requests=[dict(serial=1,request=dict(method='POST',path='/api/emoji.list',args={}),owner=dict(account=initial['account'],epoch=initial['epoch'],team='T1',user=V.O['SELF']),response={'error':'offline'},settled=True,cancelled=False,contextCurrentAtReturn=True,release=None)] if count else [])
        m=row['model'];c=m['comments'];c['draftStore']=copy.deepcopy(initial['draftStore'])
        c['comments'].update(loading=saving,page=copy.deepcopy(V.FUTURE if i==6 else V.INITIAL),pageMessages=copy.deepcopy((V.FUTURE if accepted else V.INITIAL)['messages']))
        c['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(readset*(n//4))],completed=n,cancelled=0,held=False,replies=n//4)
        f=m['fixture'];f.update(received=copy.deepcopy(V.INITIAL['messages'][1]),policy=[dict(request=q,response=r) for _ in range(V.POLICIES[case][i]) for q,r in V.policy_cycle()],prepared=[V.O['wire'](V.BODY)] if saving or accepted else [],updates=[V.O['wire'](V.BODY)] if saving or accepted else [],responses=[dict(disposition='accepted',raw=V.ACK)] if saving or accepted else [],futureRows=[V.FUTURE['messages'][1]] if saving or accepted else [],held=saving,releases=[V.O['RELEASE']] if accepted else [],published=False,publishedRow=None)
        e=m['edit'];e.update(active=None,opening=False,events=0,retired=[],attempts=[dict(id=123,ts=V.O['TS'],blocks=V.FUTURE['messages'][1]['blocks'],phase='sent' if saving else 'confirmed',receipt=None if saving else V.ACK,retryAt=0)] if saving or accepted else [])
        mount=dict(ok=True,scope=f'list-edit:100:{gen}',generation=str(gen),mount=f'plain-mount-{gen}',delta=ticket(V.INITIAL_OPS,gen))
        a=dict(scope=mount['scope'],generation=gen,ts=V.O['TS'],current=True,blocks=[],plainDelta=dict(version=1,ops=copy.deepcopy(current_ops)),nativePending=False,revision=i+1,busy=saving,notice=V.REFUSAL if unavailable else V.T['NOTICES'][4] if locked else V.T['NOTICES'][9] if saving else None,composing=False,nativeReady=not locked,captureFailed=False,source=V.INITIAL['messages'][1],preserved=V.INITIAL['messages'][1]['blocks'][1:],attempt=123 if saving else None)
        e['active']=copy.deepcopy(a) if editing else None;m['capture']=dict(scope=mount['scope'],generation=gen,mount=None if locked else mount['mount'],failed=False,canSave=not saving and not locked,canCancel=not saving,canCheck=False,canRetry=False,busy=saving,finishing=locked) if editing else None
        row['editInventory']=inv(0 if i==0 else i+1,editing,current_ops,saving)
        if len(p['opens'])<open_count:
            original_active=copy.deepcopy(a);original_active.update(plainDelta=dict(version=1,ops=V.INITIAL_OPS),busy=False,notice=None,attempt=None)
            p['opens'].append(dict(action=native_button(''),active=original_active,mount=mount,inventory=inv(i+1,True,V.INITIAL_OPS)))
            p['inputs'].append(dict(native=dict(ok=True,action='native-select-all-insertText',text=current_text,sameNode=True,before=ticket(V.INITIAL_OPS,gen),after=ticket(current_ops,gen,1),exposed=exposed),inventory=inv(i+1,True,current_ops)))
        if i==4:p['inputs'].append(dict(native=dict(ok=True,action='native-select-all-insertText',text=V.TEXT,sameNode=True,before=ticket(V.UNKNOWN_OPS,gen),after=ticket(V.CHANGED_OPS,gen,2),exposed=exposed),inventory=inv(i+1,True,V.CHANGED_OPS)))
        while len(reqs)<n//4:
            j=len(reqs);future=i==6 and j==5;text=V.FUTURE_BYTES if future else V.INITIAL_BYTES;raw=V.W['decoded'](text)
            reqs.append(dict(serial=(j+1)*4,readSerial=j+1,method='conversations.replies',args=copy.deepcopy(readset[3]['args']),variant='future' if future else 'initial',bytes=text,raw=raw,owner=dict(serial=100,account=initial['account'],epoch=initial['epoch'])))
            pages.append(dict(raw=raw,messages=raw['messages'],witness=dict(messageIndex=1,raw=raw['messages'][1],styleKeys=None if future else ['italic','bold']),current=True,requestSerial=(j+1)*4,readSerial=j+1))
        entry=dict(raw=c['comments']['page'],messages=c['comments']['pageMessages'],witness=dict(messageIndex=1,raw=c['comments']['page']['messages'][1],styleKeys=None if i==6 else ['italic','bold']))
        row['ordered']=dict(initialBytes=V.INITIAL_BYTES,futureBytes=V.FUTURE_BYTES,requests=copy.deepcopy(reqs),pages=copy.deepcopy(pages),entryPage=copy.deepcopy(entry),completed=n//4,cancelled=0,held=False,heldOwnerCurrent=None,releases=[],plainModeEnabled=True)
        g=row['geometry'];g.update(editing=editing,captureFault=None,inertTextPaint=None,replyNotice=None,controls=[dict(id=id,text='Retry return' if locked and id.endswith('cancel') else id,disabled=disabled,visible=True,hit=True,rect=rect) for id,disabled in ([('slack-list-comment-edit-cancel',saving),('slack-list-comment-edit-save',saving or locked)] if editing else [('slack-list-comments-back',False),('slack-list-comment-send',False)])],edit=dict(delta=ticket(current_ops,gen,1),text=current_text,inputSameNode=True,sameNode=True,editable=not locked and not saving,rect=rect) if editing else None,notice=dict(text=a['notice'],visible=True,hit=True,rect=rect) if a['notice'] else None)
        g['reply'].update(sameNode=True,hidden=editing,inert=editing,visible=not editing);g['row']['text']=(V.FUTURE if accepted else V.INITIAL)['messages'][1]['text']+' Preserved original context'
        if locked or saving:
            source=base['stages'][4 if locked else 9]['geometry'];g['text']=copy.deepcopy(source['text']);g['inertTextPaint']=copy.deepcopy(source['inertTextPaint'])
        if locked:
            row['journal']=None;row['editInventory']=None
            g['edit']['delta']=None
            g['releasedPaint']=dict(originalNode=True,originalInput=True,scope=mount['scope'],generation=mount['generation'],liveDeltaAbsent=True,readOnly=True)
        if not storage:
            if i==2:p['escape']=dict(ok=True,action='original-keydown',key='Escape',keyCode=27,defaultPrevented=True,isTrusted=False,sameNode=True,before=ticket(V.CHANGED_OPS,78,1),exposed=exposed)
            if i==4:p['enter']=dict(ok=True,action='original-keydown',key='Enter',keyCode=13,defaultPrevented=True,isTrusted=False,sameNode=True,before=ticket(V.CHANGED_OPS,79,1),exposed=exposed);p['doubleSave']=dict(ok=True,disabled=True)
            if i==5:p['release']=V.O['RELEASE']
            if i==6:p['actions'].append(native_button('slack-list-comments-refresh'))
        elif i>=2:
            p['actions'].append(native_button('slack-list-comment-edit-cancel'))
            if i==2:p['cancelBefore']=copy.deepcopy(rows[1]['model']);p['cancelAfter']=copy.deepcopy(m)
            ack=dict(base['readiness'],sequence=i-1,action='hold' if i==2 else 'release',ok=True,account=initial['account'],inventories=[journal],editInventories=[rows[1]['editInventory']],editHistory=[],contentSha256='a'*64)
            if i==3:ack['unchanged']=True
            p['storageControls'].append(ack);p['storage']=dict(hold=p['storageControls'][0],release=p['storageControls'][1] if i==3 else None)
        if i==3:
            p['unavailableAfter']=copy.deepcopy(m)
            p['unavailableBefore']=copy.deepcopy(m)
            p['unavailableBefore']['edit']['active']['notice']=None
            p['unavailableBefore']['comments']['ledger']=dict(requests=copy.deepcopy(c['ledger']['requests'][:12]),completed=12,cancelled=0,held=False,replies=3)
            p['unavailableBefore']['fixture']['policy']=copy.deepcopy(f['policy'][:8])
            p['unknownSave']=native_button('slack-list-comment-edit-save')
        row['proof']=copy.deepcopy(p);rows.append(row)
    p['closedDisk']=dict(copy.deepcopy(original),edit=copy.deepcopy(rows[-1]['editInventory']))
    r=dict(ok=True,case=case,fixture=fixture,readiness=base['readiness'],stages=rows,proof=p,final=copy.deepcopy(rows[-1]['model']),initialRecords=base['initialRecords'],noLiveNetwork=True)
    closed=dict(account=initial['account'],inventories=[journal],editInventories=[rows[-1]['editInventory']],editHistory=[] if storage else [dict(copy.deepcopy(attempt),phase='Confirmed',raw=V.ACK)],contentSha256='a'*64)
    return copy.deepcopy((fixture,r,closed))


class EmojiTests(unittest.TestCase):
    def reject(self,change):
        f,r,_=sample();change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):
            V.validate(f,r,D,expected_case=V.CASE,png=False)
    def test_complete_both_widths_and_closed_private_history(self):
        for narrow in (False,True):
            f,r,c=sample(narrow=narrow);V.validate(f,r,D,expected_case=V.CASE,png=False);V.validate_closed(c,r)
    def test_original_independent_future_bytes(self):
        p=Path(__file__).parents[1]/'src/desktop/slack_lists/comments/fixture/plain/emoji-future.json'
        self.assertEqual(p.read_bytes().decode('utf-8'),V.FUTURE_BYTES)
    def test_lf_cannot_replace_authored_crlf(self):self.reject(lambda r:r['stages'][0]['ordered'].update(futureBytes=V.FUTURE_BYTES[:-2]+'\n'))
    def test_source_reference_named_conversion(self):
        p=Path(__file__).parents[1]/'crates/super-platinum-core/src/slack_lists/comments/edit_seed/prepare'
        emoji=json.loads((p/'emoji/vectors.json').read_text(encoding='utf-8'))['results'][7]
        encode=json.loads((p/'encode/vectors.json').read_text(encoding='utf-8'))['results'][148]
        self.assertEqual(emoji['input'],[{'insert':':smile:'}]);self.assertIs(emoji['plain'],True)
        self.assertEqual(encode['input'][0]['insert'],emoji['output'][0]['insert'])
        self.assertEqual(encode['output']['blocks'][0]['elements'][0]['elements'][0],V.BODY[0]['elements'][0]['elements'][1])
    def test_unicode_not_semantic_conversion(self):
        self.assertEqual(V.BODY[0]['elements'][0]['elements'][2],{'type':'text','text':' Unicode \U0001f604'})
    def test_explicit_catalog_failure_required(self):self.reject(lambda r:r['stages'][3]['plain']['customCatalog'].update(requests=[]))
    def test_catalog_failed_request_owner_refused(self):self.reject(lambda r:r['stages'][3]['plain']['customCatalog']['requests'][0]['owner'].update(epoch=99))
    def test_catalog_failure_not_empty_success(self):self.reject(lambda r:r['stages'][3]['plain']['customCatalog']['requests'][0].update(response={'raw':{'ok':True,'emoji':{}}}))
    def test_unknown_input_replaced_refused(self):self.reject(lambda r:r['stages'][3]['geometry']['edit']['delta'].update(ops=V.CHANGED_OPS))
    def test_unknown_update_refused(self):self.reject(lambda r:r['stages'][3]['model']['fixture']['updates'].append(V.O['wire'](V.BODY)))
    def test_unknown_attempt_refused(self):self.reject(lambda r:r['stages'][3]['model']['edit']['attempts'].append({}))
    def test_unknown_durable_change_refused(self):self.reject(lambda r:r['stages'][3]['editInventory']['plain_delta'].update(ops=V.CHANGED_OPS))
    def test_unknown_notice_missing_refused(self):self.reject(lambda r:r['stages'][3]['geometry'].update(notice=None))
    def test_unknown_before_revision_changed_refused(self):self.reject(lambda r:r['stages'][3]['proof']['unavailableBefore']['edit']['active'].update(revision=999))
    def test_unknown_preflight_prefix_refused(self):self.reject(lambda r:r['stages'][3]['proof']['unavailableBefore']['comments']['ledger'].update(completed=8))
    def test_unknown_policy_prefix_refused(self):self.reject(lambda r:r['stages'][3]['proof']['unavailableBefore']['fixture'].update(policy=[]))
    def test_unknown_source_changed_refused(self):self.reject(lambda r:r['stages'][3]['model']['edit']['active']['source'].update(text='new'))
    def test_recovery_replacement_node_refused(self):self.reject(lambda r:r['stages'][4]['proof']['inputs'][2]['native'].update(sameNode=False))
    def test_recovery_wrong_before_delta_refused(self):self.reject(lambda r:r['stages'][4]['proof']['inputs'][2]['native']['before'].update(ops=V.INITIAL_OPS))
    def test_semantic_emoji_name_refused(self):self.reject(lambda r:r['stages'][4]['model']['edit']['attempts'][0]['blocks'][0]['elements'][0]['elements'][1].update(name='wave'))
    def test_unicode_coercion_refused(self):self.reject(lambda r:r['stages'][4]['model']['edit']['attempts'][0]['blocks'][0]['elements'][0]['elements'][2].update(type='emoji'))
    def test_reply_changed_refused(self):self.reject(lambda r:r['stages'][3]['geometry']['reply'].update(blocks=[]))
    def test_policy_locale_missing_refused(self):self.reject(lambda r:r['stages'][4]['model']['fixture']['policy'][-3]['response']['prefs'].pop('locale'))
    def test_private_preflight_removed_refused(self):self.reject(lambda r:r['stages'][4]['ordered']['pages'].pop())
    def test_native_pending_refused(self):self.reject(lambda r:r['stages'][4]['geometry']['edit']['delta'].update(pending=True))
    def test_update_text_fallback_refused(self):self.reject(lambda r:r['stages'][4]['model']['fixture']['updates'][0]['args'].update(text=V.TEXT))
    def test_readback_source_rewritten_refused(self):self.reject(lambda r:r['stages'][6]['ordered']['entryPage']['raw']['messages'][1].update(text='other'))
    def test_closed_confirmed_history_required(self):
        f,r,c=sample();c['editHistory']=[]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_wrapper_original_harness_importable(self):
        h=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-emoji-native.py')))['harness']()
        self.assertEqual(h['case_contract'](V.CASE)[0],V.STAGES[V.CASE])
    def test_wrapper_storage_exact_case_only(self):
        h=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-emoji-native.py')))['harness']()
        with self.assertRaises(RuntimeError):h['_plain_storage_controller']('foreign')

if __name__=='__main__':unittest.main()
