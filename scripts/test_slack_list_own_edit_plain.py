"""Synthetic protocol mutations only; these tests do not launch the native app."""
import copy,json,runpy,unittest
from pathlib import Path
import slack_list_own_edit_plain as V
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_edit_storage.py')))
D=Path('/private/plain')


def sample(storage=False,narrow=False):
    fixture,base,_=B['sample'](narrow);first=copy.deepcopy(base['stages'][0]);initial=first['model']['comments'];scope=first['journal']['scope'];journal=first['journal'];case='own-edit-plain-storage' if storage else 'own-edit-plain'
    initial['comments']['page']=copy.deepcopy(V.INITIAL);initial['comments']['pageMessages']=copy.deepcopy(V.INITIAL['messages']);ds=V.seed_value(initial);rect=B['B']['rect']()
    def native_button(id):return dict(ok=True,proof=dict(id=id,text=id,disabled=False,visible=True,hit=True,rect=rect))
    def ticket(ops,generation=78,rev=0):return dict(ops=copy.deepcopy(ops),mode='PlainText',attachmentEffects='none-v1',revision=rev,generation=str(generation),composing=False,pending=False)
    def inv(i,active=False,ops=None,pending=False):
        v=dict(scope=copy.deepcopy(scope),ts=V.O['TS'],revision=i,capture=f'00000000-0000-4000-8000-{i:012d}' if i else '',seed=copy.deepcopy(ds) if active else None,blocks=[],attempt=copy.deepcopy(attempt) if pending else None,retry_at=0)
        if active:v['plain_delta']=dict(version=1,ops=copy.deepcopy(ops))
        return v
    attempt=dict(id='00000000-0000-4000-8000-000000000088',capture='00000000-0000-4000-8000-000000000003',phase='Pending',raw=None,readback=None,retry_at=0,record=dict(version=2,seed=copy.deepcopy(ds),blocks=V.FUTURE['messages'][1]['blocks'],text='Changed plain comment',plain_delta=dict(version=1,ops=V.CHANGED_OPS)))
    original=dict(path='/private/nonce/comments.sqlite',account=initial['account'],scope=scope,reply=journal,edit=inv(0))
    p=dict(source=dict(received=copy.deepcopy(V.INITIAL['messages'][1]),boot=V.O['boot']()),replySeed=first['proof']['replySeed'],replyBlocks=first['proof']['replyBlocks'],opens=[],inputs=[],actions=[],storageControls=[],originalDisk=original)
    user=dict(id=V.O['SELF'],name='fixture-self',team_id='T1',is_admin=True,profile={})
    boot=dict(ok=True,team=dict(id='T1',name='List completion'),self=user,users=[user],channels=[dict(id='CLISTLOCAL',name='project-local',team_id='T1',is_channel=True,is_member=True,updated=100,previous_names=[])],prefs=V.PREFS['prefs'],subteams={'self':[]})
    init=dict(ok=True,self=user,team=dict(id='T1',name='List completion'),workspaces=[{'id':'T1'},{'id':'TOTHER001'}],onboarding={'user_data':{'onboarding_complete':True}},user_date_created=123)
    plain=dict(enabled=True,storage=storage,noLiveNetwork=True,preferences=V.PREFS,canonical=dict(locale='en-US',membership=[],rows=[],current=True),completion=dict(enabled=True,named=True,protectedFixture=True,identityCurrent=True,account=initial['account'],epoch=initial['epoch'],team='T1',user=V.O['SELF'],requests=[],responses=[],releases=[],held=0,completed=0,cancelled=0,networkCalls=0,httpCapability=False,seed=dict(boot=boot,init=init,accepted=True)))
    rows=[];reqs=[];pages=[];exposed=dict(visible=True,hit=True,rect=rect);readset=initial['ledger']['requests']
    for i,name in enumerate(V.STAGES[case]):
        editing=i==1 or (storage and i==2) or (not storage and i==3);saving=not storage and i==3;accepted=not storage and i>=4;locked=storage and i==2;n=V.READS[case][i];open_count=0 if i==0 else 1 if storage or i<3 else 2;gen=77+open_count
        row=copy.deepcopy(first);row.update(stage=name,capture=str(D/f'{fixture}-{name}.png'),plain=copy.deepcopy(plain));m=row['model'];c=m['comments'];c['draftStore']=copy.deepcopy(initial['draftStore'])
        c['comments'].update(loading=saving,page=copy.deepcopy(V.FUTURE if not storage and i==5 else V.INITIAL),pageMessages=copy.deepcopy((V.FUTURE if accepted else V.INITIAL)['messages']))
        c['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(readset*(n//4))],completed=n,cancelled=0,held=False,replies=n//4)
        f=m['fixture'];f.update(received=copy.deepcopy(V.INITIAL['messages'][1]),policy=[dict(request=q,response=r) for _ in range(V.POLICIES[case][i]) for q,r in V.policy_cycle()],prepared=[V.O['wire'](V.BODY)] if saving or accepted else [],updates=[V.O['wire'](V.BODY)] if saving or accepted else [],responses=[dict(disposition='accepted',raw=V.ACK)] if saving or accepted else [],futureRows=[V.FUTURE['messages'][1]] if saving or accepted else [],held=saving,releases=[V.O['RELEASE']] if accepted else [],published=False,publishedRow=None)
        e=m['edit'];e.update(active=None,opening=False,events=0,retired=[],attempts=[dict(id=123,ts=V.O['TS'],blocks=V.FUTURE['messages'][1]['blocks'],phase='sent' if saving else 'confirmed',receipt=None if saving else V.ACK,retryAt=0)] if saving or accepted else [])
        mount=dict(ok=True,scope=f'list-edit:100:{gen}',generation=str(gen),mount=f'plain-mount-{gen}',delta=ticket(V.INITIAL_OPS,gen))
        a=dict(scope=mount['scope'],generation=gen,ts=V.O['TS'],current=True,blocks=[],plainDelta=dict(version=1,ops=copy.deepcopy(V.CHANGED_OPS)),nativePending=False,revision=i+1,busy=saving,notice=V.T['NOTICES'][4] if locked else V.T['NOTICES'][9] if saving else None,composing=False,nativeReady=not locked,captureFailed=False,source=V.INITIAL['messages'][1],preserved=V.INITIAL['messages'][1]['blocks'][1:],attempt=123 if saving else None)
        e['active']=copy.deepcopy(a) if editing else None;m['capture']=dict(scope=mount['scope'],generation=gen,mount=None if locked else mount['mount'],failed=False,canSave=not saving and not locked,canCancel=not saving,canCheck=False,canRetry=False,busy=saving,finishing=locked) if editing else None
        row['editInventory']=inv(0 if i==0 else i+1,editing,V.CHANGED_OPS,saving)
        if len(p['opens'])<open_count:
            original_active=copy.deepcopy(a);original_active.update(plainDelta=dict(version=1,ops=V.INITIAL_OPS),busy=False,notice=None,attempt=None)
            p['opens'].append(dict(action=native_button(''),active=original_active,mount=mount,inventory=inv(i+1,True,V.INITIAL_OPS)))
            p['inputs'].append(dict(native=dict(ok=True,action='native-select-all-insertText',text='Changed plain comment',sameNode=True,before=ticket(V.INITIAL_OPS,gen),after=ticket(V.CHANGED_OPS,gen,1),exposed=exposed),inventory=inv(i+2,True,V.CHANGED_OPS)))
        while len(reqs)<n//4:
            j=len(reqs);future=not storage and i==5 and j==4;text=V.FUTURE_BYTES if future else V.INITIAL_BYTES;raw=V.W['decoded'](text)
            reqs.append(dict(serial=(j+1)*4,readSerial=j+1,method='conversations.replies',args=copy.deepcopy(readset[3]['args']),variant='future' if future else 'initial',bytes=text,raw=raw,owner=dict(serial=100,account=initial['account'],epoch=initial['epoch'])))
            pages.append(dict(raw=raw,messages=raw['messages'],witness=dict(messageIndex=1,raw=raw['messages'][1],styleKeys=None if future else ['italic','bold']),current=True,requestSerial=(j+1)*4,readSerial=j+1))
        entry=dict(raw=c['comments']['page'],messages=c['comments']['pageMessages'],witness=dict(messageIndex=1,raw=c['comments']['page']['messages'][1],styleKeys=None if not storage and i==5 else ['italic','bold']))
        row['ordered']=dict(initialBytes=V.INITIAL_BYTES,futureBytes=V.FUTURE_BYTES,requests=copy.deepcopy(reqs),pages=copy.deepcopy(pages),entryPage=copy.deepcopy(entry),completed=n//4,cancelled=0,held=False,heldOwnerCurrent=None,releases=[],plainModeEnabled=True)
        g=row['geometry'];g.update(editing=editing,captureFault=None,inertTextPaint=None,replyNotice=None,controls=[dict(id=id,text='Retry return' if locked and id.endswith('cancel') else id,disabled=disabled,visible=True,hit=True,rect=rect) for id,disabled in ([('slack-list-comment-edit-cancel',saving),('slack-list-comment-edit-save',saving or locked)] if editing else [('slack-list-comments-back',False),('slack-list-comment-send',False)])],edit=dict(delta=ticket(V.CHANGED_OPS,gen,1),text='Changed plain comment',inputSameNode=True,sameNode=True,editable=not locked and not saving,rect=rect) if editing else None,notice=dict(text=a['notice'],visible=True,hit=True,rect=rect) if a['notice'] else None)
        g['reply'].update(sameNode=True,hidden=editing,inert=editing,visible=not editing);g['row']['text']=(V.FUTURE if accepted else V.INITIAL)['messages'][1]['text']+' Preserved original context'
        if locked or saving:
            source=base['stages'][4 if locked else 9]['geometry'];g['text']=copy.deepcopy(source['text']);g['inertTextPaint']=copy.deepcopy(source['inertTextPaint'])
        if locked:
            row['journal']=None;row['editInventory']=None
            g['edit']['delta']=None
            g['releasedPaint']=dict(originalNode=True,originalInput=True,scope=mount['scope'],generation=mount['generation'],liveDeltaAbsent=True,readOnly=True)
        if not storage:
            if i==2:p['escape']=dict(ok=True,action='original-keydown',key='Escape',keyCode=27,defaultPrevented=True,isTrusted=False,sameNode=True,before=ticket(V.CHANGED_OPS,78,1),exposed=exposed)
            if i==3:p['enter']=dict(ok=True,action='original-keydown',key='Enter',keyCode=13,defaultPrevented=True,isTrusted=False,sameNode=True,before=ticket(V.CHANGED_OPS,79,1),exposed=exposed);p['doubleSave']=dict(ok=True,disabled=True)
            if i==4:p['release']=V.O['RELEASE']
            if i==5:p['actions'].append(native_button('slack-list-comments-refresh'))
        elif i>=2:
            p['actions'].append(native_button('slack-list-comment-edit-cancel'))
            if i==2:p['cancelBefore']=copy.deepcopy(rows[1]['model']);p['cancelAfter']=copy.deepcopy(m)
            ack=dict(base['readiness'],sequence=i-1,action='hold' if i==2 else 'release',ok=True,account=initial['account'],inventories=[journal],editInventories=[rows[1]['editInventory']],editHistory=[],contentSha256='a'*64)
            if i==3:ack['unchanged']=True
            p['storageControls'].append(ack);p['storage']=dict(hold=p['storageControls'][0],release=p['storageControls'][1] if i==3 else None)
        row['proof']=copy.deepcopy(p);rows.append(row)
    p['closedDisk']=dict(copy.deepcopy(original),edit=copy.deepcopy(rows[-1]['editInventory']))
    r=dict(ok=True,case=case,fixture=fixture,readiness=base['readiness'],stages=rows,proof=p,final=copy.deepcopy(rows[-1]['model']),initialRecords=base['initialRecords'],noLiveNetwork=True)
    closed=dict(account=initial['account'],inventories=[journal],editInventories=[rows[-1]['editInventory']],editHistory=[] if storage else [dict(copy.deepcopy(attempt),phase='Confirmed',raw=V.ACK)],contentSha256='a'*64)
    return fixture,r,closed


class PlainTests(unittest.TestCase):
    def reject(self,change,storage=False):
        f,r,_=sample(storage);change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):V.validate(f,r,D,expected_case=r['case'],png=False)
    def test_full_cases_both_widths_and_closed_sqlite(self):
        for storage in (False,True):
            for narrow in (False,True):
                f,r,c=sample(storage,narrow);V.validate(f,r,D,expected_case=r['case'],png=False);V.validate_closed(c,r)
    def test_fixture_original_bytes_pinned(self):
        root=Path(__file__).parents[1]/'src/desktop/slack_lists/comments/fixture'
        self.assertEqual((root/'ordered/initial.json').read_text(),V.INITIAL_BYTES)
        self.assertEqual((root/'plain/future.json').read_text(),V.FUTURE_BYTES)
    def test_native_rich_coercion_refused(self):self.reject(lambda r:r['stages'][1]['geometry']['edit']['delta'].update(mode='Rich'))
    def test_native_wrong_generation_refused(self):self.reject(lambda r:r['stages'][1]['proof']['inputs'][0]['native']['after'].update(generation='99'))
    def test_native_pending_refused(self):self.reject(lambda r:r['stages'][1]['geometry']['edit']['delta'].update(pending=True))
    def test_native_composing_refused(self):self.reject(lambda r:r['stages'][1]['geometry']['edit']['delta'].update(composing=True))
    def test_replacement_input_refused(self):self.reject(lambda r:r['stages'][1]['geometry']['edit'].update(inputSameNode=False))
    def test_covered_insert_refused(self):self.reject(lambda r:r['stages'][1]['proof']['inputs'][0]['native']['exposed'].update(hit=False))
    def test_unconsumed_enter_refused(self):self.reject(lambda r:r['stages'][3]['proof']['enter'].update(defaultPrevented=False))
    def test_escape_wrong_key_refused(self):self.reject(lambda r:r['stages'][2]['proof']['escape'].update(key='Tab'))
    def test_reply_changed_refused(self):self.reject(lambda r:r['stages'][1]['geometry']['reply'].update(blocks=[]))
    def test_original_order_refused(self):self.reject(lambda r:r['stages'][3]['ordered']['pages'][-1]['witness'].update(styleKeys=['bold','italic']))
    def test_private_preflight_missing_refused(self):self.reject(lambda r:r['stages'][3]['ordered']['pages'].pop())
    def test_original_page_rewritten_refused(self):self.reject(lambda r:r['stages'][4]['ordered']['entryPage'].update(raw=V.FUTURE))
    def test_policy_absence_not_default(self):self.reject(lambda r:r['stages'][1]['plain']['preferences']['prefs'].pop('plain_text_mode'))
    def test_notification_carrier_missing_refused(self):self.reject(lambda r:r['stages'][1]['plain']['preferences']['prefs'].pop('all_notifications_prefs'))
    def test_unobserved_membership_refused(self):self.reject(lambda r:r['stages'][1]['plain']['canonical'].update(membership=None))
    def test_locale_missing_refused(self):self.reject(lambda r:r['stages'][1]['plain']['canonical'].update(locale=None))
    def test_additional_semantic_read_refused(self):self.reject(lambda r:r['stages'][1]['plain']['completion']['requests'].append({'method':'users.search'}))
    def test_wire_context_change_refused(self):self.reject(lambda r:r['stages'][3]['model']['fixture']['prepared'][0]['args'].update(blocks='[]'))
    def test_wire_text_fallback_refused(self):self.reject(lambda r:r['stages'][3]['model']['fixture']['prepared'][0]['args'].update(text='Changed plain comment'))
    def test_durable_delta_lost_refused(self):self.reject(lambda r:r['stages'][1]['editInventory'].pop('plain_delta'))
    def test_durable_original_order_lost_refused(self):self.reject(lambda r:r['stages'][1]['editInventory']['seed']['plain'].update(ordered_response=json.dumps(V.INITIAL,sort_keys=True)))
    def test_cancel_does_not_send(self):self.reject(lambda r:r['stages'][2]['model']['fixture']['prepared'].append({}))
    def test_storage_no_disk_read_while_locked(self):self.reject(lambda r:r['stages'][2].update(journal=r['stages'][0]['journal']),True)
    def test_storage_lock_not_fabricated(self):self.reject(lambda r:r['stages'][2]['proof']['storageControls'][0].update(account='wrong'),True)
    def test_closed_storage_attempt_refused(self):
        _,r,c=sample(True);c['editHistory']=[{}]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_closed_body_mutation_refused(self):
        _,r,c=sample();c['editHistory'][0]['record']['plain_delta']['ops']=[]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_foreign_initial_disk_refused(self):self.reject(lambda r:r['stages'][0]['proof']['originalDisk'].update(account='wrong'))
    def test_foreign_record_read_refused(self):self.reject(lambda r:r['stages'][1]['model']['comments']['ledger']['requests'][1]['args'].update(id='R_LIST_001'))
    def test_held_save_retains_read_lease(self):self.reject(lambda r:r['stages'][3]['model']['comments']['comments'].update(loading=False))
    def test_other_stage_loading_refused(self):self.reject(lambda r:r['stages'][4]['model']['comments']['comments'].update(loading=True))
    def test_rich_version_record_refused(self):self.reject(lambda r:r['stages'][3]['editInventory']['attempt']['record'].update(version=1))
    def test_finished_delta_must_be_absent(self):self.reject(lambda r:r['stages'][2]['geometry']['edit'].update(delta=r['stages'][1]['geometry']['edit']['delta']),True)
    def test_finished_dom_original_owner_required(self):self.reject(lambda r:r['stages'][2]['geometry']['releasedPaint'].update(scope='foreign'),True)
    def test_finished_dom_replacement_refused(self):self.reject(lambda r:r['stages'][2]['geometry']['releasedPaint'].update(originalInput=False),True)
    def test_finished_dom_writable_refused(self):self.reject(lambda r:r['stages'][2]['geometry']['releasedPaint'].update(readOnly=False),True)
    def test_real_sqlite_plain_inventory_reader(self):
        import sqlite3
        from contextlib import closing
        wrapper=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-native.py')))
        _,receipt,_=sample(True);row=receipt['stages'][1];value=row['editInventory']
        cls=wrapper['storage_controller']('own-edit-plain-storage')['StorageControl']
        controller=cls(Path('/private'),{},case='own-edit-plain-storage')
        with closing(sqlite3.connect(':memory:')) as db:
            # SQLite memory mode cannot have DELETE journals; the same reader's
            # read-only query result is adapted only for its mode precondition.
            class Reads:
                def execute(self,query):
                    if query=='PRAGMA journal_mode':
                        return type('Row',(),{'fetchone':lambda _:('delete',)})()
                    return db.execute(query)
            db.executescript('PRAGMA user_version=1;CREATE TABLE metadata(account TEXT);CREATE TABLE drafts(owner TEXT,body TEXT);CREATE TABLE history(id TEXT,owner TEXT,body TEXT);CREATE TABLE edit_drafts(owner TEXT,body TEXT);CREATE TABLE edit_history(id TEXT,owner TEXT,body TEXT);')
            db.execute('INSERT INTO metadata VALUES (?)',(receipt['stages'][0]['model']['comments']['account'],))
            owner=json.dumps([value['scope'],value['ts']]);db.execute('INSERT INTO edit_drafts VALUES (?,?)',(owner,json.dumps(value)))
            controller.connection=Reads();self.assertEqual(controller._snapshot()['editInventories'],[value])
            for change in [lambda v:v.update(extra=True),lambda v:v['plain_delta'].update(version=True),lambda v:v.update(blocks=[{}]),lambda v:v.update(seed=None),lambda v:v['plain_delta'].update(ops=[{'retain':1}])]:
                broken=copy.deepcopy(value);change(broken);db.execute('UPDATE edit_drafts SET body=?',(json.dumps(broken),))
                with self.assertRaises(RuntimeError):controller._snapshot()
    def test_original_storage_parser_remains_strict(self):
        wrapper=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-native.py')))
        old=wrapper['storage_controller']('own-edit-storage')['StorageControl']
        import inspect
        self.assertIn("set(value) == {'scope','ts','revision','capture','seed','blocks','attempt','retry_at'}",inspect.getsource(old._snapshot))
    def test_wrapper_original_harness_importable(self):
        wrapper=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-native.py')));h=wrapper['harness']()
        self.assertEqual(h['case_contract']('own-edit-plain')[0],V.STAGES['own-edit-plain'])
    def test_wrapper_rejects_foreign_case(self):
        wrapper=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-native.py')));h=wrapper['harness']()
        with self.assertRaises(RuntimeError):h['NativeFixture']('slack-list',Path('/private/log'),checks={'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':'own-edit'})

if __name__=='__main__':unittest.main()
