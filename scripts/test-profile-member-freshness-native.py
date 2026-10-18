"""Closed receipt contract tests; no native process or network."""
import copy
from pathlib import Path
import runpy
import unittest

H=runpy.run_path(str(Path(__file__).with_name('check-profile-member-freshness-native.py')))
ROOT=Path('tmp/freshness-unit')

def rect():
    return dict(left=10,top=10,right=110,bottom=30,width=100,height=20)

def sample(case='renamed',narrow=False):
    fixture=H['FIXTURES'][int(narrow)]; viewport=list(H['expected_viewport'](fixture,'win32'))
    out=dict(ok=True,noLiveNetwork=True,fixture=fixture,case=case,viewport=viewport,stages=[],captures={})
    counts={'initial':0,'opened-held':1,'native-edited':1,'read-before-ack':1,'renamed':1,'malformed':1,'offline':2,'rejected':3,'recovered':4,'reopened':4 if case=='failure' else 1,'account-replaced':1}
    done={'renamed','malformed','offline','rejected','recovered','reopened'}
    for stage in H['STAGES'][case]:
        count=counts[stage]; replaced=stage=='account-replaced'; published=stage in ('renamed','recovered','reopened'); editor=stage not in ('initial','account-replaced')
        formtext=H['EDITED'] if stage in ('offline','rejected','recovered','reopened') else H['DRAFT']
        label=H['RENAMED'] if stage=='reopened' else H['LABEL']
        raw=dict(id=H['MEMBER'],updated=102 if published else 101,profile=dict(display_name=H['RENAMED'] if published else H['LABEL']))
        requests=[dict(kind='member',method='users/info',account=H['ACCOUNT'],team='TINIT0001',epoch=0,body=dict(updated_ids={H['MEMBER']:101},check_interaction=True,include_profile_only_users=True)) for _ in range(count)]
        frames=[]
        operations=max(0,2*count-1) if case=='failure' else count
        for i in range(operations):
            unsubscribe=case=='failure' and i%2==1
            answered=stage in done or i<operations-1
            frames.append(dict(account=H['ACCOUNT'],accepted=answered,request=dict(type='subscription_update',subtype='user_change_unsubscribe_request' if unsubscribe else 'user_change_subscribe_request',ids=[H['MEMBER']],id=i+1),reply=dict(reply_to=i+1,ok=not(case=='failure' and i==4)) if answered else None))
        failures={'malformed':1,'offline':2,'rejected':3,'recovered':3,'reopened':3 if case=='failure' else 0}.get(stage,0)
        model=dict(networkCalls=0,account=H['FOREIGN'] if replaced else H['ACCOUNT'],epoch=int(replaced),requests=requests,subscriptions=frames,profile=dict(enabled=False) if replaced else dict(enabled=True,dirty=True,attempts=[],reads=0,blocks=H['blocks'](formtext)),sourcePresent=not replaced,source=None if replaced else raw,native=None if replaced else copy.deepcopy(raw),automatic=dict(started=operations,finished=operations if stage in done else 0,http_finished=count if stage in done or stage=='read-before-ack' else 0,acknowledgments=max(0,count-(1 if case=='failure' and count>=3 else 0)) if stage in done else 0,failures=failures,worker=stage not in done and count>0))
        controls=[] if replaced else ['Back','Save section']+(['Use description','Cancel description edit'] if editor else ['Edit description'])
        text=H['DRAFT'] if stage=='opened-held' else H['EDITED']
        captured=dict(snapshot=dict(blocks=H['blocks'](text)),tokens=[dict(value=t,label='@'+label) for t in H['blocks'](text)[0]['elements'][0]['elements'][1:]]) if editor else None
        geometry=dict(ok=True,noOverflow=True,viewport=viewport,dialog=not replaced,dialogRect=rect(),controls=[dict(text=t,disabled=editor and t=='Save section',visible=True,rect=rect()) for t in controls],notices=[],tokens=[dict(text='@'+label,editable=False,visible=True,rect=rect()) for _ in range(2)] if editor else [],richEditable=True if editor else None,captured=captured,globalToast=None,pngDimensions=[1280,780])
        proof=dict(native=dict(ok=True,sameNode=True,sameSelection=True,noToast=True),replacement=dict(actualInstallLoaded=True,before=dict(profile=dict(enabled=True,attempts=[],reads=0))))
        out['stages'].append(dict(stage=stage,model=model,geometry=geometry,proof=proof));out['captures'][stage]=str(ROOT/f'{fixture}-{stage}.png')
    out['final']=copy.deepcopy(out['stages'][-1]['model'])
    return out

def lifetime(case='socket-successor',narrow=False):
    base=sample('renamed',narrow);fixture=base['fixture'];viewport=base['viewport'];old=base['stages']
    result={k:copy.deepcopy(v) for k,v in base.items() if k not in ('stages','captures','final')};result.update(case=case,stages=[],captures={})
    socket=case=='socket-successor';generation=2
    counts=({'initial':0,'held':1,'native-edited':1,'socket-replaced':2,'retired-replies':2,'successor-read':2,'successor-ack':2,'reopened':2}
            if socket else {'initial':0,'healthy':1,'changed-native':1,'opening-held':2,'read-before-ack':2,'continued':2})
    waiting=dict(identity=9,visit=3,serial=4,reading=True,mounted=False)
    for stage in H['STAGES'][case]:
        form=not socket and stage in ('healthy','changed-native');pending=not socket and stage in ('opening-held','read-before-ack')
        template=old[0] if stage=='initial' or form or pending else old[1] if stage=='held' else old[-1] if stage in ('continued','reopened') else old[2]
        row=copy.deepcopy(template);row['stage']=stage;row['proof']={};m=row['model'];g=row['geometry'];count=counts[stage]
        replacement=socket and stage in ('socket-replaced','retired-replies','successor-read','successor-ack','reopened')
        published=stage in ('continued','successor-ack','reopened');native_changed=not socket and stage in ('changed-native','opening-held','read-before-ack','continued')
        m['socketGeneration']=generation+int(replacement)
        m['opening']=dict(identity=10 if stage=='continued' else 9,visit=3,serial=4,reading=pending,mounted=not(form or pending or stage=='initial'))
        m['profile']['blocks']=H['blocks'](H['EDITED'] if (not socket and stage!='initial') or stage=='reopened' else H['DRAFT'])
        m['source']=dict(id=H['MEMBER'],updated=102 if published else 101,profile=dict(display_name=H['RENAMED'] if published else H['LABEL']))
        m['native']=copy.deepcopy(m['source'])
        if native_changed:m['native']['profile']['display_name']=H['RENAMED']
        m['requests']=[dict(serial=i+1,kind='member',method='users/info',account=H['ACCOUNT'],team='TINIT0001',epoch=0,body=dict(updated_ids={H['MEMBER']:101},check_interaction=True,include_profile_only_users=True)) for i in range(count)]
        operations=count if socket else max(0,2*count-1);frames=[]
        for i in range(operations):
            sock=i if socket else 0;id=1 if socket else i+1
            answered=(published or (not socket and i<2) or (socket and i==0 and stage in ('retired-replies','successor-read')))
            accepted=answered and not(socket and replacement and i==0)
            frames.append(dict(index=i,socket=sock,generation=generation+sock,account=H['ACCOUNT'],request=dict(type='subscription_update',subtype='user_change_unsubscribe_request' if not socket and i==1 else 'user_change_subscribe_request',ids=[H['MEMBER']],id=id),reply=dict(reply_to=id,ok=True) if answered else None,accepted=accepted if answered else None))
        m['subscriptions']=frames
        m['automatic']=dict(started=operations,finished=operations if published or form else 1 if replacement else 2 if pending else 0,http_finished=2 if not socket and stage in ('read-before-ack','continued') else 1 if published or form or stage=='successor-read' else 0,acknowledgments=(1 if socket else 3) if published else 2 if pending else 1 if form else 0,failures=0,worker=not(published or form or stage=='initial'))
        if pending:
            g.update(captured=None,tokens=[],richEditable=None,notices=[dict(text='Loading mention names. Your description is unchanged.',visible=True,rect=rect())],controls=[dict(text=t,disabled=t in ('Save section','Retry description'),visible=True,rect=rect()) for t in ('Back','Save section','Retry description','Cancel description edit')])
            row['proof']['opening']=copy.deepcopy(waiting)
        if socket and replacement:row['proof']['native']=dict(ok=True,sameNode=True,sameSelection=True,noToast=True)
        if stage=='socket-replaced':row['proof'].update(before=copy.deepcopy(result['stages'][2]['model']),connection=dict(before=generation,after=generation+1,socket=1))
        if stage=='retired-replies':row['proof'].update(oldHttpDelivered=False,oldAck=dict(routed=False))
        if stage=='changed-native':row['proof'].update(before=dict(source=copy.deepcopy(m['source'])),after=dict(source=copy.deepcopy(m['source'])),acceptedGeneration=generation)
        if stage=='continued':row['proof'].update(opening=copy.deepcopy(waiting),afterOpening=copy.deepcopy(m['opening']),noExtraOpen=True)
        result['stages'].append(row);result['captures'][stage]=str(ROOT/f'{fixture}-{stage}.png')
    result['final']=copy.deepcopy(result['stages'][-1]['model']);return result

class LifetimeTests(unittest.TestCase):
    def validate(self,r):H['validate'](r['fixture'],r,ROOT,platform='win32',png=False)
    def reject(self,change,case='socket-successor'):
        r=lifetime(case);change(r)
        with self.assertRaises(RuntimeError):self.validate(r)
    def test_cases_both_widths(self):
        for case in ('socket-successor','changed-opening'):
            for narrow in (False,True):
                with self.subTest(case=case,narrow=narrow):self.validate(lifetime(case,narrow))
    def test_same_integer_ack_different_socket_allowed(self):self.validate(lifetime())
    def test_wrong_successor_socket(self):self.reject(lambda r:r['stages'][3]['model']['subscriptions'][1].update(socket=0))
    def test_old_ack_routed_rejected(self):self.reject(lambda r:r['stages'][4]['proof']['oldAck'].update(routed=True))
    def test_old_http_delivered_rejected(self):self.reject(lambda r:r['stages'][4]['proof'].update(oldHttpDelivered=True))
    def test_old_source_poison_rejected(self):self.reject(lambda r:r['stages'][4]['model']['source'].update(updated=999))
    def test_successor_http_not_ack(self):self.reject(lambda r:r['stages'][5]['model']['automatic'].update(acknowledgments=1))
    def test_successor_native_custody(self):self.reject(lambda r:r['stages'][6]['proof']['native'].update(sameNode=False))
    def test_foreign_replacement_not_successor(self):self.reject(lambda r:r['stages'][3]['model'].update(account=H['FOREIGN']))
    def test_changed_event_not_raw_adoption(self):self.reject(lambda r:r['stages'][2]['model']['source'].update(updated=102),'changed-opening')
    def test_changed_waiting_has_no_stale_editor(self):self.reject(lambda r:r['stages'][3]['geometry'].update(richEditable=True),'changed-opening')
    def test_waiting_notice_visible(self):self.reject(lambda r:r['stages'][3]['geometry']['notices'][0].update(visible=False),'changed-opening')
    def test_original_opening_until_ack(self):self.reject(lambda r:r['stages'][4]['model']['opening'].update(identity=88),'changed-opening')
    def test_continued_original_blocks(self):self.reject(lambda r:r['stages'][5]['model']['profile'].update(blocks=H['blocks']('lost draft')),'changed-opening')
    def test_continued_same_visit(self):self.reject(lambda r:r['stages'][5]['model']['opening'].update(visit=77),'changed-opening')
    def test_continued_is_automatic(self):self.reject(lambda r:r['stages'][5]['proof'].update(noExtraOpen=False),'changed-opening')
    def test_continued_ends_loading(self):self.reject(lambda r:r['stages'][5]['model']['opening'].update(reading=True),'changed-opening')

def continuation(case='cross-socket-opening',narrow=False):
    base=lifetime('changed-opening',narrow);old=base['stages'];cross=case=='cross-socket-opening'
    out={k:copy.deepcopy(v) for k,v in base.items() if k not in ('stages','captures','final')};out.update(case=case,stages=[],captures={})
    counts=({'initial':0,'changed-native':1,'opening-held':2,'disconnected':2,'successor-held':3,'old-retired':3,'successor-read':3,'continued':3} if cross else
            {'initial':0,'opening-held':2,'failed':2,'retry-held':3,'canceled':3,'shared-finished':3,'reopened':3,'account-replaced':4})
    original=dict(identity=9,visit=3,serial=4,reading=True,mounted=False,lease=True);canceled=dict(identity=12,visit=3,serial=4,reading=False,mounted=False,lease=False)
    for stage in H['STAGES'][case]:
        waiting=stage in ('opening-held','disconnected','successor-held','old-retired','successor-read','retry-held');replaced=stage=='account-replaced';fresh=stage in ('continued','shared-finished','reopened')
        successor=cross and stage in ('successor-held','old-retired','successor-read','continued');count=counts[stage]
        template=old[3] if waiting or stage=='failed' else old[5] if stage in ('continued','reopened') else old[0]
        row=copy.deepcopy(template);row.update(stage=stage,proof={});m=row['model'];g=row['geometry'];proof=row['proof']
        m['account']=H['FOREIGN'] if replaced else H['ACCOUNT'];m['epoch']=int(replaced);m['socketGeneration']=2+int(successor)
        m['profile']=dict(enabled=False) if replaced else dict(enabled=True,dirty=True,attempts=[],reads=0,blocks=H['blocks'](H['DRAFT'] if stage=='initial' else H['EDITED']))
        m['sourcePresent']=not replaced;m['source']=None if replaced else dict(id=H['MEMBER'],updated=102 if fresh else 101,profile=dict(display_name=H['RENAMED'] if fresh else H['LABEL']))
        m['native']=None if replaced else dict(id=H['MEMBER'],updated=101 if stage=='initial' else 102,profile=dict(display_name=H['LABEL'] if stage=='initial' else H['RENAMED']))
        opening=copy.deepcopy(original if waiting else canceled if stage in ('canceled','shared-finished') else dict(identity=10,visit=3,serial=4,reading=False,mounted=stage in ('continued','reopened'),lease=False))
        if stage=='retry-held':opening['identity']=11
        m['opening']=None if replaced else opening
        m['requests']=[dict(serial=i+1,kind='member',method='users/info',account=H['ACCOUNT'],team='TINIT0001',epoch=0,body=dict(updated_ids={H['MEMBER']:102 if replaced and i==count-1 else 101},check_interaction=True,include_profile_only_users=True)) for i in range(count)]
        ops=(0 if count==0 else 1 if count==1 else 4 if successor else 3) if cross else max(0,2*count-1);frames=[]
        for i in range(ops):
            sock=1 if cross and i==3 else 0;unsub=i==1 if cross else i%2==1;id=1 if sock else i+1
            answered=(i<2 or (cross and i==2 and stage in ('old-retired','successor-read','continued')) or (cross and stage=='continued') or (not cross and (i<ops-1 or stage in ('failed','shared-finished','reopened'))))
            accepted=answered and not(cross and i==2 and successor)
            frames.append(dict(socket=sock,generation=2+sock,account=H['ACCOUNT'],request=dict(type='subscription_update',subtype='user_change_unsubscribe_request' if unsub else 'user_change_subscribe_request',ids=[H['MEMBER']],id=id),reply=dict(reply_to=id,ok=True) if answered else None,accepted=accepted if answered else None))
        m['subscriptions']=frames
        finished=4 if stage=='continued' else 5 if stage in ('shared-finished','reopened') else 3 if stage in ('failed','disconnected') or successor else max(0,ops-1)
        acks=3 if stage=='continued' else 5 if stage in ('shared-finished','reopened') else 2 if cross and count>=2 else max(0,ops-1)
        m['automatic']=dict(started=ops,finished=finished,http_finished=2 if stage in ('successor-read','continued') else 1,acknowledgments=acks,failures=0 if cross or stage in ('initial','opening-held') else 1,worker=waiting and stage!='disconnected',pending=[H['MEMBER']] if stage=='disconnected' else [])
        if waiting:proof['opening']=copy.deepcopy(opening)
        if stage=='disconnected':proof['acceptedGeneration']=2
        if stage=='successor-held':proof['connection']=dict(before=2,after=3,socket=1)
        if stage=='old-retired':proof.update(oldHttpDelivered=False,oldAck=dict(routed=False))
        if stage=='successor-read':proof['beforeHttp']=1
        if stage=='continued':proof.update(opening=copy.deepcopy(original),afterOpening=copy.deepcopy(opening),noExtraOpen=True)
        if stage=='failed':
            proof['quietFailure']=True
            g['notices'][0]['text']='Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details.'
            for c in g['controls']:c['disabled']=c['text']=='Save section'
        if stage=='retry-held':proof['ordinaryOpen']=True
        if stage=='canceled':proof.update(beforeOpening=dict(identity=11),afterOpening=copy.deepcopy(opening))
        if stage=='shared-finished':proof.update(canceledOpening=copy.deepcopy(canceled),noReopen=True)
        if replaced:
            proof['replacement']=dict(actualInstallLoaded=True,before=dict(profile=dict(enabled=True,attempts=[],reads=0,blocks=H['blocks'](H['EDITED'])),opening=dict(reading=True,lease=True)))
            g.update(dialog=False,controls=[],tokens=[],captured=None,richEditable=None,notices=[])
        out['stages'].append(row);out['captures'][stage]=str(ROOT/f"{out['fixture']}-{stage}.png")
    out['final']=copy.deepcopy(out['stages'][-1]['model']);return out

class ContinuationTests(unittest.TestCase):
    def validate(self,r):H['validate'](r['fixture'],r,ROOT,platform='win32',png=False)
    def reject(self,change,case='cross-socket-opening'):
        r=continuation(case);change(r)
        with self.assertRaises(RuntimeError):self.validate(r)
    def test_both_cases_both_widths(self):
        for case in ('cross-socket-opening','opening-failure-cancel'):
            for narrow in (False,True):
                with self.subTest(case=case,narrow=narrow):self.validate(continuation(case,narrow))
    def test_worker_none_still_pending(self):self.reject(lambda r:r['stages'][3]['model']['opening'].update(reading=False))
    def test_pending_gap_not_empty(self):self.reject(lambda r:r['stages'][3]['model']['automatic'].update(pending=[]))
    def test_successor_keeps_original_lease(self):self.reject(lambda r:r['stages'][4]['model']['opening'].update(identity=55))
    def test_original_poison_not_adopted(self):self.reject(lambda r:r['stages'][5]['model']['source'].update(updated=999))
    def test_original_ack_not_successor(self):self.reject(lambda r:r['stages'][5]['proof']['oldAck'].update(routed=True))
    def test_http_cannot_resume_original(self):self.reject(lambda r:r['stages'][6]['model']['opening'].update(mounted=True))
    def test_automatic_final_no_click(self):self.reject(lambda r:r['stages'][7]['proof'].update(noExtraOpen=False))
    def test_failed_clears_lease(self):self.reject(lambda r:r['stages'][2]['model']['opening'].update(lease=True),'opening-failure-cancel')
    def test_failed_retry_visible(self):self.reject(lambda r:r['stages'][2]['geometry']['notices'][0].update(visible=False),'opening-failure-cancel')
    def test_canceled_reading_cleared(self):self.reject(lambda r:r['stages'][4]['model']['opening'].update(reading=True),'opening-failure-cancel')
    def test_shared_result_cannot_reopen(self):self.reject(lambda r:r['stages'][5]['model']['opening'].update(mounted=True),'opening-failure-cancel')
    def test_cancel_preserves_raw_blocks(self):self.reject(lambda r:r['stages'][4]['model']['profile'].update(blocks=H['blocks']('changed')),'opening-failure-cancel')
    def test_ordinary_reopen_not_forced_retry(self):self.reject(lambda r:r['stages'][3]['proof'].update(ordinaryOpen=False),'opening-failure-cancel')
    def test_account_was_pending(self):self.reject(lambda r:r['stages'][7]['proof']['replacement']['before']['opening'].update(lease=False),'opening-failure-cancel')
    def test_account_does_not_adopt(self):self.reject(lambda r:r['stages'][7]['model'].update(sourcePresent=True),'opening-failure-cancel')

class FreshnessTests(unittest.TestCase):
    def validate(self,r): H['validate'](r['fixture'],r,ROOT,platform='win32',png=False)
    def reject(self,mutate,case='renamed'):
        r=sample(case);mutate(r)
        with self.assertRaises(RuntimeError):self.validate(r)
    def test_all_cases_both_widths(self):
        for case in ('renamed','failure','account'):
            for narrow in (False,True):
                with self.subTest(case=case,narrow=narrow):self.validate(sample(case,narrow))
    def test_forced_read_not_conditional(self):self.reject(lambda r:r['stages'][1]['model']['requests'][0]['body'].update(ids=[H['MEMBER']]))
    def test_foreign_read(self):self.reject(lambda r:r['stages'][1]['model']['requests'][0].update(account=H['FOREIGN']))
    def test_missing_socket(self):self.reject(lambda r:r['stages'][1]['model'].update(subscriptions=[]))
    def test_wrong_ack_identity(self):self.reject(lambda r:r['stages'][4]['model']['subscriptions'][0]['reply'].update(reply_to=999))
    def test_http_not_enough(self):self.reject(lambda r:r['stages'][3]['model']['automatic'].update(finished=1))
    def test_early_source_publication(self):self.reject(lambda r:r['stages'][3]['model']['source'].update(updated=102))
    def test_mounted_label_is_frozen(self):self.reject(lambda r:r['stages'][4]['geometry']['captured']['tokens'][0].update(label='@'+H['RENAMED']))
    def test_native_selection_custody(self):self.reject(lambda r:r['stages'][4]['proof']['native'].update(sameSelection=False))
    def test_no_auto_save(self):self.reject(lambda r:r['stages'][4]['model']['profile'].update(attempts=[{}]))
    def test_no_quiet_failure_toast(self):self.reject(lambda r:r['stages'][3]['geometry'].update(globalToast='Offline'),'failure')
    def test_real_failure_counter(self):self.reject(lambda r:r['stages'][3]['model']['automatic'].update(failures=0),'failure')
    def test_ack_rejection_is_not_success(self):self.reject(lambda r:r['stages'][5]['model']['subscriptions'][-1]['reply'].update(ok=True),'failure')
    def test_new_account_no_publication(self):self.reject(lambda r:r['stages'][-1]['model'].update(sourcePresent=True),'account')
    def test_new_account_actual_install(self):self.reject(lambda r:r['stages'][-1]['proof']['replacement'].update(actualInstallLoaded=False),'account')
    def test_raw_token_not_replaced(self):self.reject(lambda r:r['stages'][2]['geometry']['captured']['snapshot']['blocks'][0]['elements'][0]['elements'][1].update(opaque={}))
    def test_clipped_token(self):self.reject(lambda r:r['stages'][2]['geometry']['tokens'][0].update(visible=False))
    def test_missing_unsubscribe(self):self.reject(lambda r:r['stages'][4]['model']['subscriptions'].pop(1),'failure')
    def test_unsubscribe_wrong_waiter(self):self.reject(lambda r:r['stages'][4]['model']['subscriptions'][1].update(accepted=False),'failure')
    def test_unsubscribe_rejected(self):self.reject(lambda r:r['stages'][4]['model']['subscriptions'][1]['reply'].update(ok=False),'failure')
    def test_replaced_original_write_rejected(self):self.reject(lambda r:r['stages'][-1]['proof']['replacement']['before']['profile'].update(attempts=[{}]),'account')
    def test_replaced_original_ledger_required(self):self.reject(lambda r:r['stages'][-1]['proof']['replacement'].pop('before'),'account')
    def test_live_null_ledger_rejected(self):self.reject(lambda r:r['stages'][1]['model']['profile'].pop('attempts'))
    def test_retired_no_current_ledger(self):self.reject(lambda r:r['stages'][-1]['model']['profile'].update(attempts=[]),'account')
    def test_unicode_literal_independent(self):self.assertEqual([ord(c) for c in H['EDITED'].split(' ')[3]], [0x4e2d,0x1f642])

if __name__=='__main__':unittest.main()
