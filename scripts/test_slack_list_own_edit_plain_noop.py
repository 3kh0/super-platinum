"""Synthetic protocol mutations; no native execution or comparator result fabrication."""
import copy
from pathlib import Path
import runpy
import unittest
import slack_list_own_edit_plain_noop as V
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_own_edit_plain.py')))
D=B['D']


def sample(storage=False,narrow=False):
    fixture,base,_=B['sample'](storage=storage,narrow=narrow)
    first=base['stages'][0];initial=copy.deepcopy(first['model']['comments']);journal=first['journal'];scope=journal['scope'];seed=V.seed_value(initial)
    rect=B['B']['B']['rect']();case=V.CASES[int(storage)];rows=[];reqs=[];pages=[]
    def button(id):return dict(ok=True,proof=dict(id=id,text=id,disabled=False,visible=True,hit=True,rect=rect))
    def ticket(ops,gen):return dict(ops=copy.deepcopy(ops),mode='PlainText',revision=1,generation=str(gen),composing=False,pending=False,attachmentEffects='none-v1')
    def inv(active=False,ops=None):
        v=dict(scope=copy.deepcopy(scope),ts=V.O['TS'],revision=1 if active else 0,capture='00000000-0000-4000-8000-000000000003' if active else '',seed=copy.deepcopy(seed) if active else None,blocks=[],attempt=None,retry_at=0)
        if active:v['plain_delta']=dict(version=1,ops=copy.deepcopy(ops))
        return v
    original=dict(copy.deepcopy(first['proof']['originalDisk']),edit=inv());p=dict(source=first['proof']['source'],replySeed=first['proof']['replySeed'],replyBlocks=first['proof']['replyBlocks'],opens=[],inputs=[],actions=[],storageControls=[],originalDisk=original)
    exposed=dict(visible=True,hit=True,rect=rect)
    for i,name in enumerate(V.STAGES[case]):
        failed=storage and i==2;editing=i in ((1,2) if storage else (1,3,4,7));ops=(V.TEMP_OPS if name=='modified' else V.INITIAL_OPS) if editing else None
        opens=int(i>0) if storage else 0 if i==0 else 1 if i<3 else 2 if i<7 else 3;gen=77+opens
        row=copy.deepcopy(first);row.update(stage=name,capture=str(D/f'{fixture}-{name}.png'));m=row['model'];c=m['comments'];n=V.READS[case][i]
        c['comments'].update(loading=False,page=copy.deepcopy(V.INITIAL),pageMessages=copy.deepcopy(V.INITIAL['messages']))
        c['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(initial['ledger']['requests']*(n//4))],completed=n,cancelled=0,held=False,replies=n//4)
        f=m['fixture'];f.update(received=copy.deepcopy(V.INITIAL['messages'][1]),policy=[dict(request=q,response=r) for _ in range(V.POLICIES[case][i]) for q,r in V.policy_cycle()],prepared=[],updates=[],responses=[],futureRows=[],held=False,releases=[],published=False,publishedRow=None)
        e=m['edit'];e.update(active=None,opening=False,events=0,retired=[],attempts=[])
        mount=dict(ok=True,scope=f'list-edit:100:{gen}',generation=str(gen),mount=f'plain-mount-{gen}',delta=ticket(V.INITIAL_OPS,gen))
        a=dict(scope=mount['scope'],generation=gen,ts=V.O['TS'],current=True,blocks=[],plainDelta=dict(version=1,ops=copy.deepcopy(ops)),nativePending=False,revision=0 if i==1 else 1,busy=False,
            notice=V.T['NOTICES'][4] if failed else None,composing=False,nativeReady=not failed,captureFailed=False,source=V.INITIAL['messages'][1],preserved=V.INITIAL['messages'][1]['blocks'][1:],attempt=None)
        e['active']=copy.deepcopy(a) if editing else None
        m['capture']=dict(scope=mount['scope'],generation=gen,mount=None if failed else mount['mount'],failed=False,canSave=not failed,canCancel=True,canCheck=False,canRetry=False,busy=False,finishing=failed) if editing else None
        row['editInventory']=inv(editing,ops)
        if len(p['opens'])<opens:
            original_a=dict(copy.deepcopy(a),plainDelta=dict(version=1,ops=V.INITIAL_OPS))
            p['opens'].append(dict(action=button(''),active=original_a,mount=mount,inventory=inv(True,V.INITIAL_OPS)))
            p['originalOps']=V.INITIAL_OPS
        if not storage and i in (3,4):
            before,after,text=(V.INITIAL_OPS,V.TEMP_OPS,V.TEMP_TEXT) if i==3 else (V.TEMP_OPS,V.INITIAL_OPS,V.ORIGINAL_TEXT)
            p['inputs'].append(dict(native=dict(ok=True,action='native-select-all-insertText',text=text,sameNode=True,before=ticket(before,gen),after=ticket(after,gen),exposed=exposed),inventory=inv(True,after)))
        while len(reqs)<n//4:
            j=len(reqs);raw=copy.deepcopy(V.INITIAL)
            reqs.append(dict(serial=(j+1)*4,readSerial=j+1,method='conversations.replies',args=copy.deepcopy(initial['ledger']['requests'][3]['args']),variant='initial',bytes=V.INITIAL_BYTES,raw=raw,owner=dict(serial=initial['comments']['generation'],account=initial['account'],epoch=initial['epoch'])))
            pages.append(dict(raw=raw,messages=raw['messages'],witness=dict(messageIndex=1,raw=raw['messages'][1],styleKeys=['italic','bold']),current=True,requestSerial=(j+1)*4,readSerial=j+1))
        entry=dict(raw=c['comments']['page'],messages=c['comments']['pageMessages'],witness=dict(messageIndex=1,raw=c['comments']['page']['messages'][1],styleKeys=['italic','bold']))
        row['ordered']=dict(initialBytes=V.INITIAL_BYTES,futureBytes=V.FUTURE_BYTES,requests=copy.deepcopy(reqs),pages=copy.deepcopy(pages),entryPage=entry,completed=n//4,cancelled=0,held=False,heldOwnerCurrent=None,releases=[],plainModeEnabled=True)
        g=row['geometry'];g.update(editing=editing,captureFault=None,inertTextPaint=None,replyNotice=None,controls=[dict(id=id,text='Retry return' if failed and id.endswith('cancel') else id,disabled=disabled,visible=True,hit=True,rect=rect) for id,disabled in
            ([('slack-list-comment-edit-cancel',False),('slack-list-comment-edit-save',failed)] if editing else [('slack-list-comments-back',False),('slack-list-comment-send',False)])],
            edit=dict(delta=ticket(ops,gen),text=ops[0]['insert'].rstrip('\n'),inputSameNode=True,sameNode=True,editable=not failed,rect=rect) if editing else None,
            notice=dict(text=a['notice'],visible=True,hit=True,rect=rect) if failed else None)
        g['reply'].update(sameNode=True,hidden=editing,inert=editing,visible=not editing);g['row']['text']=V.INITIAL['messages'][1]['text']+' Preserved original context'
        if i==2:
            p['saveBefore']=copy.deepcopy(rows[1]['model']);p['saveAfter']=copy.deepcopy(m);p['actions'].append(button('slack-list-comment-edit-save'))
        if failed:
            source=base['stages'][2]['geometry'];g['text']=copy.deepcopy(source['text']);g['inertTextPaint']=copy.deepcopy(source['inertTextPaint']);g['edit']['delta']=None
            g['releasedPaint']=dict(originalNode=True,originalInput=True,scope=mount['scope'],generation=mount['generation'],liveDeltaAbsent=True,readOnly=True)
            row['journal']=None;row['editInventory']=None
        if storage and i>=2:
            ack=dict(base['readiness'],sequence=i-1,action='hold' if i==2 else 'release',ok=True,account=initial['account'],inventories=[journal],editInventories=[rows[1]['editInventory']],editHistory=[],contentSha256='a'*64)
            if i==3:ack['unchanged']=True;p['actions'].append(button('slack-list-comment-edit-cancel'))
            p['storageControls'].append(ack);p['storage']=dict(hold=p['storageControls'][0],release=p['storageControls'][1] if i==3 else None)
        if not storage and i==5:
            p['revertSaveBefore']=copy.deepcopy(rows[4]['model']);p['revertSaveAfter']=copy.deepcopy(m)
            p['enter']=dict(ok=True,action='original-keydown',key='Enter',keyCode=13,defaultPrevented=True,isTrusted=False,sameNode=True,before=ticket(V.INITIAL_OPS,79),exposed=exposed)
        if name=='readback':p['actions'].append(button('slack-list-comments-refresh'))
        row['proof']=copy.deepcopy(p);rows.append(row)
    final=copy.deepcopy(rows[-1]['model']);cleared=inv()
    if not storage:
        final['edit']['active']=None;final['capture']=None;p['finalCancel']=button('slack-list-comment-edit-cancel')
        p['finalReturn']=dict(native=copy.deepcopy(rows[6]['geometry']),model=copy.deepcopy(final),disk=dict(copy.deepcopy(original),edit=cleared))
    p['closedDisk']=dict(copy.deepcopy(original),edit=cleared)
    receipt=dict(ok=True,case=case,fixture=fixture,readiness=base['readiness'],stages=rows,proof=p,final=final,initialRecords=base['initialRecords'],noLiveNetwork=True)
    closed=dict(account=initial['account'],inventories=[journal],editInventories=[cleared],editHistory=[],contentSha256='a'*64)
    return copy.deepcopy((fixture,receipt,closed))


class NoopTests(unittest.TestCase):
    def reject(self,change,storage=False):
        f,r,_=sample(storage);change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):V.validate(f,r,D,expected_case=r['case'],png=False)
    def test_full_both_cases_both_widths_closed_original_sqlite(self):
        for storage in (False,True):
            for narrow in (False,True):
                f,r,c=sample(storage,narrow);V.validate(f,r,D,expected_case=r['case'],png=False);V.validate_closed(c,r)
    def test_missing_native_capability_refused(self):self.reject(lambda r:r['stages'][1]['geometry']['edit']['delta'].pop('attachmentEffects'))
    def test_wrong_native_capability_refused(self):self.reject(lambda r:r['stages'][1]['geometry']['edit']['delta'].update(attachmentEffects='unknown'))
    def test_mount_missing_capability_refused(self):self.reject(lambda r:r['stages'][1]['proof']['opens'][0]['mount']['delta'].pop('attachmentEffects'))
    def test_revert_input_missing_capability_refused(self):self.reject(lambda r:r['stages'][4]['proof']['inputs'][1]['native']['after'].pop('attachmentEffects'))
    def test_initial_delta_not_display_fallback(self):self.reject(lambda r:r['stages'][1]['geometry']['edit']['delta'].update(ops=[{'insert':'Original own comment.\n'}]))
    def test_actual_modified_stage_required(self):self.reject(lambda r:r['stages'][3]['geometry']['edit']['delta'].update(ops=V.INITIAL_OPS))
    def test_exact_revert_required(self):self.reject(lambda r:r['stages'][4]['geometry']['edit']['delta'].update(ops=V.TEMP_OPS))
    def test_changed_then_revert_must_use_original_input(self):self.reject(lambda r:r['stages'][4]['proof']['inputs'][1]['native'].update(sameNode=False))
    def test_revert_before_must_be_actual_changed_delta(self):self.reject(lambda r:r['stages'][4]['proof']['inputs'][1]['native']['before'].update(ops=V.INITIAL_OPS))
    def test_fresh_preflight_not_suppressed(self):self.reject(lambda r:r['stages'][2]['model']['comments']['ledger'].update(completed=8))
    def test_fresh_policy_read_required(self):self.reject(lambda r:r['stages'][2]['model']['fixture']['policy'].pop())
    def test_private_ordered_source_required(self):self.reject(lambda r:r['stages'][5]['ordered']['pages'].pop())
    def test_no_update_even_if_same_text(self):self.reject(lambda r:r['stages'][2]['model']['fixture']['updates'].append({}))
    def test_no_attempt_even_if_no_http(self):self.reject(lambda r:r['stages'][2]['model']['edit']['attempts'].append({}))
    def test_no_prepared_request(self):self.reject(lambda r:r['stages'][5]['model']['fixture']['prepared'].append({}))
    def test_no_fabricated_ack(self):self.reject(lambda r:r['stages'][5]['model']['fixture']['responses'].append({'ok':True}))
    def test_independent_reply_node_preserved(self):self.reject(lambda r:r['stages'][2]['geometry']['reply'].update(sameNode=False))
    def test_independent_reply_blocks_preserved(self):self.reject(lambda r:r['stages'][5]['journal'].update(blocks=[]))
    def test_original_transcript_opaque_preserved(self):self.reject(lambda r:r['stages'][6]['model']['comments']['comments']['pageMessages'][1]['edit_fixture_opaque'].update(keep='changed'))
    def test_original_context_preserved(self):self.reject(lambda r:r['stages'][6]['ordered']['entryPage']['raw']['messages'][1]['blocks'].pop())
    def test_return_must_close_original_edit(self):self.reject(lambda r:r['stages'][2]['model']['edit'].update(active=r['stages'][1]['model']['edit']['active']))
    def test_final_cancel_is_original_native_action(self):self.reject(lambda r:r['proof']['finalCancel']['proof'].update(disabled=True))
    def test_final_return_no_new_request(self):self.reject(lambda r:r['proof']['finalReturn']['model']['comments']['ledger'].update(completed=32))
    def test_final_cleared_inventory_required(self):self.reject(lambda r:r['proof']['closedDisk']['edit'].update(seed={'changed':True}))
    def test_storage_no_locked_sqlite_snapshot(self):self.reject(lambda r:r['stages'][2].update(journal=r['stages'][1]['journal']),True)
    def test_storage_no_live_ticket_after_release(self):self.reject(lambda r:r['stages'][2]['geometry']['edit'].update(delta=r['stages'][1]['geometry']['edit']['delta']),True)
    def test_storage_wrong_scope_paint_refused(self):self.reject(lambda r:r['stages'][2]['geometry']['releasedPaint'].update(scope='foreign'),True)
    def test_storage_retry_return_control_required(self):self.reject(lambda r:r['stages'][2]['geometry']['controls'][0].update(disabled=True),True)
    def test_storage_original_save_before_must_be_unchanged(self):self.reject(lambda r:r['stages'][2]['proof']['saveBefore']['edit']['active']['plainDelta'].update(ops=V.TEMP_OPS),True)
    def test_storage_lock_release_must_remain_unchanged(self):self.reject(lambda r:r['stages'][3]['proof']['storageControls'][1].update(unchanged=False),True)
    def test_closed_send_history_forbidden(self):
        _,r,c=sample();c['editHistory']=[{}]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_wrapper_original_guards_and_two_lock_counts(self):
        h=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-noop-native.py')))['harness']()
        for case in V.CASES:self.assertEqual(h['case_contract'](case)[0],V.STAGES[case])
        f,r,_=sample(True);controller=h['_plain_storage_controller'](V.CASES[1])['StorageControl']
        a=controller(D,r['readiness'],case=V.CASES[0]);b=controller(D,r['readiness'],case=V.CASES[1])
        self.assertEqual((a.command_count,b.command_count),(0,2))


if __name__=='__main__':unittest.main()
