"""Focused no-op rich/custody mutations; synthetic receipts are not native proof."""
import copy
from pathlib import Path
import runpy
import unittest

V=runpy.run_path(str(Path(__file__).with_name('slack_list_own_edit_noop.py')))
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_edit_storage.py')))
O=V['O'];DIRECTORY=Path('/private/own-edit-noop')


def sample(case='own-edit-noop',narrow=False):
    fixture,base,_=B['sample'](narrow);first=base['stages'][0];scope=first['journal']['scope']
    seed=V['T']['RST']['seed_value'](first['model']);original=copy.deepcopy(V['ORIGINAL'])
    modified=copy.deepcopy(original);modified[0]['elements'][-1]['elements'][-1]['text']+=V['TEMP']
    storage=case.endswith('-storage');rows=[];opens=[];actions=[];p={k:copy.deepcopy(first['proof'][k]) for k in ('source','replySeed','replyBlocks')}
    p.update(actions=[],opens=[],storageControls=[])
    readset=first['model']['comments']['ledger']['requests']
    def inv(rev,blocks,active=False):
        return dict(scope=copy.deepcopy(scope),ts=O['TS'],revision=rev,capture=f'00000000-0000-4000-8000-{rev:012d}' if rev else '',
            seed=copy.deepcopy(seed) if active else None,blocks=copy.deepcopy(blocks),attempt=None,retry_at=0)
    def action(id):
        native=dict(ok=True,proof=dict(id='' if id=='open' else id,text='Edit' if id=='open' else id,disabled=False,visible=True,hit=True,rect=B['B']['rect']()))
        actions.append(native if id=='open' else dict(id=id,native=native))
    counts=(0,1,1,1) if storage else (0,1,1,2,2,2,2,3)
    for i,name in enumerate(V['STAGES'][case]):
        row=copy.deepcopy(first);row.update(stage=name,capture=str(DIRECTORY/f'{fixture}-{name}.png'))
        model=row['model'];m,f,e=(model[k] for k in ('comments','fixture','edit'))
        n=V['READS'][case][i];m['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(readset*(n//4))],completed=n,replies=n//4,cancelled=0,held=False)
        f['policy']=[dict(request=q,response=r) for _ in range(V['POLICIES'][case][i]) for q,r in O['policy_cycle']()]
        editing=i in ((1,2) if storage else (1,3,4,7));failed=storage and i==2
        blocks=modified if not storage and i==3 else original
        generation=77+counts[i]
        mount=dict(ok=True,scope=f'list-edit:100:{generation}',mount=f'actual-mount-{generation}',generation=str(generation),blocks=copy.deepcopy(original))
        e.update(opening=False,events=0,retired=[],attempts=[],active=None)
        if editing:
            e['active']=dict(scope=mount['scope'],generation=generation,ts=O['TS'],current=True,blocks=copy.deepcopy(blocks),revision=i,
                busy=False,notice=V['T']['NOTICES'][4] if failed else None,composing=False,source=O['received'](),preserved=O['received']()['blocks'][1:],
                nativeReady=not failed,captureFailed=False,attempt=None)
            model['capture']=dict(scope=mount['scope'],generation=generation,mount=None if failed else mount['mount'],failed=False,canSave=not failed,
                canCancel=True,canCheck=False,canRetry=False,busy=False,finishing=failed)
        else:model['capture']=None
        row['editInventory']=inv(i,blocks if editing else [],editing)
        if len(opens)<counts[i]:
            action('open');a=copy.deepcopy(e['active']);a['blocks']=copy.deepcopy(original)
            opens.append(dict(active=a,mount=copy.deepcopy(mount),inventory=inv(i,original,True)))
            p['originalBlocks']=copy.deepcopy(original)
        if i==2:
            action('slack-list-comment-edit-save');p['saveBefore']=copy.deepcopy(rows[1]['model']);p['saveAfter']=copy.deepcopy(model)
        if not storage and i==3:p['append']=dict(ok=True,blocks=copy.deepcopy(modified))
        if not storage and i==4:p['revert']=dict(ok=True,action='undo',undone=True,sameNode=True,before=dict(blocks=copy.deepcopy(modified)),after=dict(blocks=copy.deepcopy(original)))
        if not storage and i==5:
            action('slack-list-comment-edit-save');p['revertSaveBefore']=copy.deepcopy(rows[4]['model']);p['revertSaveAfter']=copy.deepcopy(model)
        if not storage and i==6:action('slack-list-comments-refresh')
        if storage and i==3:action('slack-list-comment-edit-cancel')
        if storage and i>=2:
            acks=[]
            for seq in range(1,i):
                ack=dict(base['readiness'],sequence=seq,action='hold' if seq==1 else 'release',ok=True,account=m['account'],
                    inventories=[copy.deepcopy(first['journal'])],editInventories=[copy.deepcopy(rows[1]['editInventory'])],editHistory=[],contentSha256='a'*64)
                if seq==2:ack['unchanged']=True
                acks.append(ack)
            p['storageControls']=acks;p['storage']=dict(hold=acks[0],release=acks[1] if len(acks)>1 else None)
        g=row['geometry']
        g.update(editing=editing,captureFault=None,replyNotice=None,inertTextPaint=None,
            controls=[dict(id=id,text='Retry return' if failed and id.endswith('cancel') else id,disabled=failed and id.endswith('save'),visible=True,hit=True,rect=B['B']['rect']())
                for id in (['slack-list-comment-edit-cancel','slack-list-comment-edit-save'] if editing else ['slack-list-comments-back','slack-list-comment-send'])],
            edit=dict(blocks=copy.deepcopy(blocks),editable=not failed,sameNode=True,rect=B['B']['rect']()) if editing else None,
            notice=dict(text=V['T']['NOTICES'][4],visible=True,hit=True,rect=B['B']['rect']()) if failed else None)
        g['reply'].update(sameNode=True,hidden=editing,inert=editing,visible=not editing)
        if failed:
            fg=copy.deepcopy(base['stages'][4]['geometry']);g['text']=fg['text'];g['inertTextPaint']=fg['inertTextPaint']
            row['journal']=None;row['editInventory']=None
        row['rowNative']=dict(ok=True,ts=O['TS'],text=g['row']['text'],editedMarkers=0)
        p.update(opens=copy.deepcopy(opens),actions=copy.deepcopy(actions));row['proof']=copy.deepcopy(p);rows.append(row)
    final=copy.deepcopy(rows[-1]['model'])
    if not storage:
        action('slack-list-comment-edit-cancel');p['actions']=copy.deepcopy(actions)
        final['edit']['active']=None;final['capture']=None
        p['finalReturn']=dict(native=copy.deepcopy(rows[0]['geometry']),inventory=inv(9,[]),journal=copy.deepcopy(first['journal']))
    result=dict(ok=True,case=case,fixture=fixture,readiness=base['readiness'],stages=rows,proof=p,final=final,initialRecords=base['initialRecords'],noLiveNetwork=True)
    return fixture,result


class NoopTests(unittest.TestCase):
    def check(self,case='own-edit-noop',narrow=False):
        fixture,r=sample(case,narrow);V['validator'](case)(fixture,r,DIRECTORY,png=False);return r

    def reject(self,mutation,case='own-edit-noop'):
        fixture,r=sample(case);mutation(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):
            V['validator'](case)(fixture,r,DIRECTORY,png=False)

    def test_both_cases_widths(self):
        for case in V['STAGES']:
            for narrow in (False,True):self.check(case,narrow)

    def test_exact_rich_not_plain_text(self):
        for key,value in (('style',{'italic':True}),('opaque',{'new':True}),('text','Changed')):
            with self.subTest(key=key):self.reject(lambda r:r['stages'][4]['model']['edit']['active']['blocks'][0]['elements'][0]['elements'][0].update({key:value}))

    def test_context_cannot_enter_editable_or_change_source(self):
        self.reject(lambda r:r['stages'][1]['model']['edit']['active']['blocks'].append(copy.deepcopy(O['received']()['blocks'][-1])))
        self.reject(lambda r:r['stages'][1]['model']['edit']['active']['source']['blocks'][-1].update(edit_context_opaque={}))

    def test_no_update_attempt_ack_or_reply_send(self):
        for key in ('prepared','updates','responses','futureRows','releases'):
            with self.subTest(key=key):self.reject(lambda r:r['stages'][2]['model']['fixture'][key].append({}))
        self.reject(lambda r:r['stages'][2]['model']['edit']['attempts'].append({}))
        self.reject(lambda r:r['stages'][2]['model']['comments']['composeLedger']['posts'].append({}))

    def test_no_noop_preflight(self):
        self.reject(lambda r:r['stages'][2]['proof']['saveAfter']['fixture']['policy'].append({}))
        self.reject(lambda r:r['stages'][5]['proof']['revertSaveAfter']['comments']['ledger'].update(completed=16))

    def test_undo_is_actual_original_node(self):
        for key in ('ok','undone','sameNode'):
            with self.subTest(key=key):self.reject(lambda r:r['stages'][4]['proof']['revert'].update({key:False}))
        self.reject(lambda r:r['stages'][4]['proof']['revert'].update(action='set-blocks'))

    def test_reply_and_edit_nodes_remain_original(self):
        self.reject(lambda r:r['stages'][4]['geometry']['edit'].update(sameNode=False))
        self.reject(lambda r:r['stages'][2]['geometry']['reply'].update(sameNode=False))
        self.reject(lambda r:r['stages'][2]['journal'].update(revision=99))

    def test_native_original_transcript_no_edited_marker(self):
        self.reject(lambda r:r['stages'][6]['rowNative'].update(editedMarkers=1))
        self.reject(lambda r:r['stages'][2]['model']['comments']['comments']['pageMessages'][1].update(edited={'ts':'1'}))

    def test_closure_and_final_cancel_required(self):
        self.reject(lambda r:r['final']['edit'].update(active=copy.deepcopy(r['stages'][7]['model']['edit']['active'])))
        self.reject(lambda r:r['proof']['actions'].pop())
        self.reject(lambda r:r['proof']['finalReturn']['inventory'].update(seed={}))

    def test_storage_locked_no_disk_read_and_retry_only(self):
        case='own-edit-noop-storage'
        self.reject(lambda r:r['stages'][2].update(journal=copy.deepcopy(r['stages'][0]['journal'])),case)
        self.reject(lambda r:r['stages'][2]['geometry']['controls'][1].update(disabled=False),case)
        self.reject(lambda r:r['stages'][2]['model']['capture'].update(canSave=True),case)
        self.reject(lambda r:r['stages'][3]['proof']['storageControls'][1].update(unchanged=False),case)

    def test_closed_snapshot_exact_and_no_history(self):
        for case in V['STAGES']:
            r=self.check(case);disk=r['stages'][-1]['editInventory'] if case.endswith('-storage') else r['proof']['finalReturn']['inventory']
            s=dict(account=r['stages'][0]['model']['comments']['account'],inventories=[r['stages'][0]['journal']],editInventories=[disk],editHistory=[],contentSha256='a'*64)
            V['validate_closed'](s,r)
            for key,value in (('editHistory',[{}]),('account','foreign'),('inventories',[]),('editInventories',[])):
                with self.subTest(case=case,key=key),self.assertRaises(RuntimeError):V['validate_closed'](dict(s,**{key:value}),r)

    def test_isolated_harness_and_storage_scope(self):
        w=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-noop-native.py')));h=w['harness']()
        self.assertEqual(h['case_contract']('own-edit-noop')[0],V['STAGES']['own-edit-noop'])
        self.assertEqual(h['case_contract']('own-edit')[0],O['STAGES']['own-edit'])
        self.assertEqual(w['storage_controller']('own-edit-noop')['StorageControl'].__name__,'NoopStorage')
        self.assertEqual(w['storage_controller']('other')['StorageControl'].__name__,'StorageControl')

    def test_exact_zero_or_two_storage_commands(self):
        w=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-noop-native.py')))
        for case,count in (('own-edit-noop',0),('own-edit-noop-storage',2)):
            cls=w['storage_controller'](case)['StorageControl']
            controller=cls(Path('/not-entered'),{},case=case)
            self.assertEqual(controller.command_count,count)
            self.assertEqual(controller.case,'own-edit-storage')
            self.assertEqual(controller.evidence['configuredCase'],case)
            with self.assertRaises(RuntimeError):cls(Path('/not-entered'),{},case='own-edit')

    def test_wrong_owner_or_native_capture_refused(self):
        self.reject(lambda r:r['stages'][1]['model']['comments'].update(account='00000000-0000-4000-8000-000000000099'))
        self.reject(lambda r:r['stages'][1]['model']['capture'].update(mount='replacement'))
        self.reject(lambda r:r['stages'][1]['model']['edit']['active'].update(current=False))

    def test_actual_save_action_and_readback_required(self):
        self.reject(lambda r:r['stages'][2]['proof']['actions'][1]['native']['proof'].update(hit=False))
        self.reject(lambda r:r['stages'][6]['proof']['actions'][-1].update(id='fabricated-refresh'))
        self.reject(lambda r:r['stages'][6]['model']['comments']['ledger']['requests'].pop())

    def test_clear_cannot_retain_send_record_or_original_seed(self):
        self.reject(lambda r:r['stages'][2]['editInventory'].update(attempt={'phase':'Pending'}))
        self.reject(lambda r:r['stages'][2]['editInventory'].update(seed=copy.deepcopy(r['stages'][1]['editInventory']['seed'])))
        self.reject(lambda r:r['stages'][3]['editInventory'].update(seed={}))


if __name__=='__main__':unittest.main()
