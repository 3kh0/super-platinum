"""Portable synthetic mutations; no native launch and no runtime-derived expectations."""
import copy
import json
from pathlib import Path
import runpy
import unittest
import slack_list_own_edit_plain_custom as V

B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_own_edit_plain_emoji.py')))
D=B['D']


def sample(retired=False,narrow=False,cancelled=False):
    fixture,base,_=B['sample'](narrow=narrow)
    initial=copy.deepcopy(base['stages'][0]['model']['comments']);first=base['stages'][0]
    journal=first['journal'];scope=journal['scope'];seed=V.seed_value(initial);rect=B['B']['B']['rect']()
    case=V.CASES[int(retired)];rows=[];requests=[];pages=[];owner=dict(account=initial['account'],epoch=initial['epoch'],team='T1',user=V.O['SELF'])
    def button(id):return dict(ok=True,proof=dict(id=id,text=id,disabled=False,visible=True,hit=True,rect=rect))
    def ticket(ops):return dict(ops=copy.deepcopy(ops),mode='PlainText',attachmentEffects='none-v1',revision=1,generation='78',composing=False,pending=False)
    att=dict(id='00000000-0000-4000-8000-000000000088',capture='00000000-0000-4000-8000-000000000003',phase='Pending',raw=None,readback=None,retry_at=0,
        record=dict(version=2,seed=seed,blocks=V.FUTURE['messages'][1]['blocks'],text=V.TEXT,plain_delta=dict(version=1,ops=V.CHANGED_OPS)))
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
        failed=name=='catalog-failed';held=name=='catalog-held';saving=name=='saving';accepted=name in ('saved','readback');replaced=name=='replaced';editing=name in ('editing','catalog-failed','catalog-held','saving');busy=held or saving
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
        count=0 if i<2 else 1 if retired or failed else 2
        cat=dict(started=count,finished=count-int(held),active=int(held),cancelled=int(replaced and cancelled),pending=[count] if held else [],contextsCurrent=[not replaced]*count,networkCalls=0,httpCapability=False,requests=[])
        for k in range(1,count+1):
            offline=not retired and k==1;complete=offline or sent or replaced
            release=None if offline or not complete else dict(serial=k,receiverRetired=replaced and cancelled,delivered=not (replaced and cancelled))
            cat['requests'].append(dict(serial=k,request=dict(method='POST',path='/api/emoji.list',args={}),owner=owner,response={'error':'offline'} if offline else {'raw':V.RAW,'bytes':V.RAW_BYTES} if complete else None,
                settled=complete,cancelled=replaced and cancelled,contextCurrentAtReturn=None if not complete or replaced and cancelled else not replaced,release=release))
        row['plain']['customCatalog']=cat
        g=row['geometry'];g.update(editing=editing,captureFault=None,inertTextPaint=None,replyNotice=None,controls=[dict(id=id,text=id,disabled=disabled,visible=True,hit=True,rect=rect) for id,disabled in
            ([('slack-list-comment-edit-cancel',busy),('slack-list-comment-edit-save',busy)] if editing else [('slack-list-comments-back',False),('slack-list-comment-send',False)])],
            edit=dict(delta=ticket(V.CHANGED_OPS),text=V.TEXT,inputSameNode=True,sameNode=True,editable=not busy,rect=rect) if editing else None,notice=dict(text=notice,visible=True,hit=True,rect=rect) if notice else None)
        g['reply'].update(sameNode=True,hidden=editing,inert=editing,visible=not editing);g['row']['text']=(V.FUTURE if accepted else V.INITIAL)['messages'][1]['text']+' Preserved original context'
        if busy:
            g['text']=copy.deepcopy(base['stages'][4]['geometry']['text']);g['inertTextPaint']=copy.deepcopy(base['stages'][4]['geometry']['inertTextPaint'])
        if failed:p.update(failedSaveBefore=copy.deepcopy(rows[1]['model']),failedSaveAfter=copy.deepcopy(m),firstSave=button('slack-list-comment-edit-save'))
        if held:
            p['enter']=dict(ok=True,action='original-keydown',key='Enter',keyCode=13,defaultPrevented=True,isTrusted=False,sameNode=True,before=ticket(V.CHANGED_OPS),exposed=exposed)
            p['busyControls']=dict(ok=True,saveDisabled=True,cancelDisabled=True)
        if saving:p.update(catalogRelease=cat['requests'][-1]['release'],doubleSave=dict(ok=True,disabled=True))
        if accepted:p['release']=V.O['RELEASE']
        if name=='readback':p['actions'].append(button('slack-list-comments-refresh'))
        if replaced:
            original_f=copy.deepcopy(rows[2]['model']['fixture']);current=copy.deepcopy(c)
            current.update(account='list-custom-emoji-successor',epoch=initial['epoch']+1,noSession=True,noTransport=True,comments=None,document=None,records=None,unrelated=None,detail=None)
            row.update(model=current,geometry={'ok':True},catalog=cat,editInventory=copy.deepcopy(rows[2]['editInventory']))
            row['ordered']['entryPage']=None;row.pop('plain')
            disk=dict(copy.deepcopy(original),edit=copy.deepcopy(row['editInventory']))
            p.update(beforeReplacementDisk=disk,afterReplacementDisk=copy.deepcopy(disk),originalFixtureBefore=original_f,originalFixtureAfter=copy.deepcopy(original_f),
                replacement=dict(originalAccount=initial['account'],originalEpoch=initial['epoch'],detached={'ok':True},syntheticAccountReplacement=True,catalogRelease=cat['requests'][-1]['release'],successorBefore=copy.deepcopy(current)))
        row['proof']=copy.deepcopy(p);rows.append(row)
    p['closedDisk']=dict(copy.deepcopy(original),edit=copy.deepcopy(rows[-1]['editInventory']))
    receipt=dict(ok=True,case=case,fixture=fixture,readiness=base['readiness'],stages=rows,proof=p,final=copy.deepcopy(rows[-1]['model']),initialRecords=base['initialRecords'],noLiveNetwork=True)
    closed=dict(account=initial['account'],inventories=[journal],editInventories=[rows[-1]['editInventory']],editHistory=[] if retired else [dict(copy.deepcopy(att),phase='Confirmed',raw=V.ACK)],contentSha256='a'*64)
    return copy.deepcopy((fixture,receipt,closed))


class CustomTests(unittest.TestCase):
    def reject(self,change,retired=False):
        f,r,_=sample(retired);change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):
            V.validate(f,r,D,expected_case=r['case'],png=False)
    def test_full_cases_both_widths_and_closed_original_database(self):
        for retired in (False,True):
            for narrow in (False,True):
                f,r,c=sample(retired,narrow);V.validate(f,r,D,expected_case=r['case'],png=False);V.validate_closed(c,r)
    def test_actual_cancelled_receiver_is_distinct_valid_retirement(self):
        f,r,c=sample(True,cancelled=True);V.validate(f,r,D,expected_case=r['case'],png=False);V.validate_closed(c,r)
    def test_authored_bytes_are_independent_utf8_and_exact_lf(self):
        p=Path(__file__).parents[1]/'src/desktop/slack_lists/comments/fixture/completion/plain_custom_emoji'
        self.assertEqual((p/'future.json').read_bytes().decode(),V.FUTURE_BYTES)
        self.assertEqual((p/'catalog.json').read_bytes().decode(),V.RAW_BYTES)
    def test_alias_requested_name_retained(self):
        self.assertEqual(V.BODY[0]['elements'][0]['elements'][3],{'type':'emoji','name':'parrot_alias'})
    def test_raw_catalog_byte_normalization_refused(self):self.reject(lambda r:r['stages'][4]['plain']['customCatalog']['requests'][1]['response'].update(bytes=V.RAW_BYTES.rstrip()))
    def test_raw_alias_target_changed_refused(self):self.reject(lambda r:r['stages'][4]['plain']['customCatalog']['requests'][1]['response']['raw']['emoji'].update(parrot_alias='alias:other'))
    def test_catalog_borrowed_owner_refused(self):self.reject(lambda r:r['stages'][3]['plain']['customCatalog']['requests'][1]['owner'].update(epoch=99))
    def test_catalog_wrong_factory_refused(self):self.reject(lambda r:r['stages'][3]['plain']['customCatalog']['requests'][1]['request'].update(path='/api/emojis.info'))
    def test_catalog_partial_instead_complete_refused(self):self.reject(lambda r:r['stages'][4]['plain']['customCatalog']['requests'][1]['response']['raw'].pop('emoji'))
    def test_failed_save_cannot_be_empty_positive(self):self.reject(lambda r:r['stages'][2]['plain']['customCatalog']['requests'][0].update(response={'raw':{'ok':True,'emoji':{}}}))
    def test_held_response_invented_refused(self):self.reject(lambda r:r['stages'][3]['plain']['customCatalog']['requests'][1].update(response={'raw':V.RAW,'bytes':V.RAW_BYTES}))
    def test_held_cancel_cannot_be_enabled(self):self.reject(lambda r:r['stages'][3]['model']['capture'].update(canCancel=True))
    def test_busy_cancel_native_guard_missing(self):self.reject(lambda r:r['stages'][3]['proof']['busyControls'].update(cancelDisabled=False))
    def test_held_send_attempt_refused(self):self.reject(lambda r:r['stages'][3]['model']['edit']['attempts'].append({}))
    def test_failure_durable_delta_change_refused(self):self.reject(lambda r:r['stages'][2]['editInventory']['plain_delta'].update(ops=V.INITIAL_OPS))
    def test_same_editor_replaced_refused(self):self.reject(lambda r:r['stages'][3]['geometry']['edit'].update(sameNode=False))
    def test_failed_reply_draft_changed(self):self.reject(lambda r:r['stages'][2]['journal'].update(blocks=[]))
    def test_alias_wire_target_substitution_refused(self):self.reject(lambda r:r['stages'][4]['model']['edit']['attempts'][0]['blocks'][0]['elements'][0]['elements'][3].update(name='party_parrot'))
    def test_custom_unicode_invention_refused(self):self.reject(lambda r:r['stages'][4]['model']['edit']['attempts'][0]['blocks'][0]['elements'][0]['elements'][1].update(unicode='1f99c'))
    def test_private_preflight_omitted_refused(self):self.reject(lambda r:r['stages'][3]['ordered']['pages'].pop())
    def test_ordered_future_newline_refused(self):self.reject(lambda r:r['stages'][0]['ordered'].update(futureBytes=V.FUTURE_BYTES[:-1]+'\r\n'))
    def test_future_readback_changed_refused(self):self.reject(lambda r:r['stages'][6]['ordered']['entryPage']['raw']['messages'][1].update(text='other'))
    def test_retired_current_context_refused(self):self.reject(lambda r:r['stages'][3]['catalog'].update(contextsCurrent=[True]),True)
    def test_retired_return_current_refused(self):self.reject(lambda r:r['stages'][3]['catalog']['requests'][0].update(contextCurrentAtReturn=True),True)
    def test_retired_update_refused(self):self.reject(lambda r:r['stages'][3]['proof']['originalFixtureAfter']['updates'].append({}),True)
    def test_retired_durable_edit_changed_refused(self):self.reject(lambda r:r['stages'][3]['editInventory']['plain_delta'].update(ops=V.INITIAL_OPS),True)
    def test_retired_successor_changed_refused(self):self.reject(lambda r:r['stages'][3]['model'].update(writes=1),True)
    def test_retired_old_ui_survived_refused(self):self.reject(lambda r:r['stages'][3]['model'].update(comments={}),True)
    def test_retired_detachment_missing_refused(self):self.reject(lambda r:r['stages'][3]['geometry'].update(ok=False),True)
    def test_closed_retired_attempt_refused(self):
        _,r,c=sample(True);c['editHistory']=[{}]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_closed_success_history_missing_refused(self):
        _,r,c=sample();c['editHistory']=[]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_wrapper_uses_original_closed_harness(self):
        h=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-custom-native.py')))['harness']()
        self.assertTrue(callable(h['run_fixture']))
        for case in V.CASES:self.assertEqual(h['case_contract'](case)[0],V.STAGES[case])


if __name__=='__main__':unittest.main()
