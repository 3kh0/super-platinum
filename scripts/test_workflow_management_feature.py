import copy
import json
import unittest
from urllib.parse import urlencode
import workflow_management_feature as V


def request():
    query={'_x_id':'1'*32,'_x_csid':'2'*32,'fp':'3'*32,'_x_desktop_ia':'true','_x_foreground':'true','_x_frontend_build_type':'current','_x_gantry':'true','_x_num_retries':'0','_x_version_ts':'capture-required','slack_route':'default'}
    return dict(method=V.METHOD,url='https://example.test/api/'+V.METHOD+'?'+urlencode(query),fields=[['trigger_ids','TR_REVIEW'],['channel_id',V.A.DEST],['_x_reason',V.REASON]],tokenPresent=True)


def sample(case,stage='added',held=False,pairs=1):
    sent=stage=='added' and case!='denied'
    methods=['api.getFlannelHttpUrl','permissions/info']*(0 if stage not in ('selected','added') else 2 if sent else 1)
    if stage=='added':
        if sent and case!='rejected' and not held:methods+=['conversations.info']
        methods+=['bookmarks.list','workflows.triggers.preview']*pairs
    reads=[]
    for i,method in enumerate(methods,1):
        body,result,endpoint,safe=V.read_literal(case,method)
        reads.append(dict(serial=i,method=method,body=body,result=result,endpoint=endpoint,retrySafe=safe,owner=copy.deepcopy(V.OWNER)))
    raw=V.acknowledgement(case)
    settled=sent and not held
    attempt=dict(number=1,status=200,outcome='rejected' if case=='rejected' else 'acknowledged',raw=raw,responseText=json.dumps(raw),retryAfterMillis=None,mayHaveApplied=case!='rejected')
    feature=dict(case=case,reads=reads,requests=[dict(serial=1,request=request(),owner=copy.deepcopy(V.OWNER))] if sent else [],responses=[dict(serial=1,raw=raw)] if sent else [],attempts=[attempt] if settled else [],settlements=[dict(serial=1,returned=True)] if settled else [],releases=[dict(serial=1,receiverRetired=False,delivered=True)] if case=='held' and settled else [],held=[1] if held else [],acknowledged=settled and case!='rejected',networkCalls=0)
    return {'confirmationAdd':{'feature':feature}}


def destination(case,held=False):
    featured=case in ('success','held') and not held
    return dict(ok=True,connected=True,modalAbsent=True,status='ready',notice='Featuring workflow\u2026' if held else V.NOTICE[case],
      readback=dict(current=True,status='ready',bookmarksRaw=V.read_literal(case,'bookmarks.list')[1]['raw'],previewsRaw=V.preview(),owner={'channel':V.A.DEST,'team':'TWORKFLOW'},featured=[{'id':'TR_REVIEW'}] if featured else [],bookmarked=[{'id':'TR_REVIEW'}],channelRaw=V.channel(case) if case in ('success','held','readback-missing') and not held else copy.deepcopy(V.A.CHANNELS[V.A.DEST])),
      rows=[dict(id='TR_REVIEW',status='available',title='Published review \u4e2d')]*(2 if featured else 1))


class Feature(unittest.TestCase):
    def reject(self,change,case='success',stage='added'):
        value=sample(case,stage,pairs=2 if case=='held' else 1);V.ledger(case,stage,value);change(value['confirmationAdd']['feature'])
        with self.assertRaises(RuntimeError):V.ledger(case,stage,value)
    def test_all_six_cases_and_phases(self):
        for case in V.CASES:
            for stage in ('about','checked','selected','added'):
                with self.subTest(case=case,stage=stage):V.ledger(case,stage,sample(case,stage,pairs=2 if case=='held' and stage=='added' else 1))
    def test_held_actual_delayed_response(self):V.ledger('held','added',sample('held',held=True),held=True)
    def test_success_coalesced_or_two_browse_cycles(self):
        for count in (1,2):V.ledger('success','added',sample('success',pairs=count))
    def test_readback_can_interleave_browse(self):
        value=sample('success');reads=value['confirmationAdd']['feature']['reads'];reads.append(reads.pop(4))
        for i,r in enumerate(reads,1):r['serial']=i
        V.ledger('success','added',value)
    def test_no_third_browse_replay(self):
        with self.assertRaises(RuntimeError):V.ledger('success','added',sample('success',pairs=3))
    def test_permission_aggregate_is_not_grant(self):self.reject(lambda v:v['reads'][1]['result']['raw'].update(allowed=[],allowed_per_team={'TWORKFLOW':['MANAGE_CHANNEL_WORKFLOWS']}))
    def test_wrong_permission_family(self):self.reject(lambda v:v['reads'][1]['body'].update(permissions=['MANAGE_CHANNEL_TABS']))
    def test_invented_reason_on_edge(self):self.reject(lambda v:v['reads'][1]['body'].update(_x_reason='invented'))
    def test_wrong_owner(self):self.reject(lambda v:v['reads'][0]['owner'].update(user='UOTHER'))
    def test_wrong_origin(self):self.reject(lambda v:v['requests'][0]['request'].update(url=v['requests'][0]['request']['url'].replace('example.test','evil.test')))
    def test_wrong_channel(self):self.reject(lambda v:v['requests'][0]['request']['fields'][1].__setitem__(1,V.A.PRIVATE))
    def test_duplicate_mutation(self):self.reject(lambda v:v['requests'].append(copy.deepcopy(v['requests'][0])))
    def test_http200_without_ack_is_not_success(self):self.reject(lambda v:v['attempts'][0]['raw'].update(ok=False))
    def test_raw_response_custody(self):self.reject(lambda v:v['responses'][0]['raw'].pop('opaque_receipt'))
    def test_response_text_custody(self):self.reject(lambda v:v['attempts'][0].update(responseText='{}'))
    def test_rejected_attempt_not_applied(self):self.reject(lambda v:v['attempts'][0].update(mayHaveApplied=True),'rejected')
    def test_denied_has_no_mutation(self):self.reject(lambda v:v['requests'].append({'serial':1}),'denied')
    def test_readback_failure_not_fabricated_raw(self):self.reject(lambda v:v['reads'][4].update(result={'raw':{'ok':True}}),'readback-failure')
    def test_missing_readback_not_fabricated_properties(self):self.reject(lambda v:v['reads'][4]['result']['raw']['channel'].update(properties=V.channel('success')['properties']),'readback-missing')
    def test_held_drop_not_delivery(self):self.reject(lambda v:v['releases'][0].update(receiverRetired=True,delivered=False),'held')
    def test_held_before_release_cannot_settle(self):
        value=sample('held',held=True);value['confirmationAdd']['feature']['settlements']=[{'serial':1,'returned':True}]
        with self.assertRaises(RuntimeError):V.ledger('held','added',value,held=True)
    def test_all_destination_outcomes(self):
        for case in V.CASES:V.destination_proof(case,destination(case))
        V.destination_proof('held',destination('held',True),held=True)
    def test_ack_not_substitute_for_canonical_feature(self):
        value=destination('readback-missing');value['readback']['featured']=[{'id':'TR_REVIEW'}]
        with self.assertRaises(RuntimeError):V.destination_proof('readback-missing',value)
    def test_bookmark_title_not_preview(self):
        value=destination('success');value['rows'][0]['title']=V.A.bookmark()['title']
        with self.assertRaises(RuntimeError):V.destination_proof('success',value)
    def test_missing_feedback(self):
        value=destination('rejected');value['notice']=None
        with self.assertRaises(RuntimeError):V.destination_proof('rejected',value)
    def test_checkbox_requires_original_exposed_exact_copy(self):
        value=dict(ok=True,present=True,nodeId=12,original=True,connected=True,type='checkbox',checked=False,disabled=False,indeterminate=False,label='Feature this workflow in channel',help='This adds it as a button to the message composer.',visible=True,hit=True,rect={'left':1,'top':2,'right':17,'bottom':18,'width':16,'height':16},hitPoint={'x':9,'y':10})
        check=lambda v:V.checkbox_proof(v,True,False,lambda *_:None,{})
        check(value)
        for key,wrong in [('original',False),('hit',False),('visible',False),('disabled',True),('checked',True),('label','wrong'),('connected',False)]:
            with self.subTest(key=key),self.assertRaises(RuntimeError):check(dict(value,**{key:wrong}))
    def test_product_outcomes_remain_independent(self):
        for case in V.CASES:
            server=sample(case,pairs=2 if case=='held' else 1);ledger=server['confirmationAdd']['feature'];allowed=case!='denied'
            op=None if not allowed else dict(generation=1,request=ledger['requests'][0]['request'],attempts=ledger['attempts'],outcome='rejected' if case=='rejected' else 'acknowledged',raw=None if case=='rejected' else V.acknowledgement(case),readbackStatus='not-started' if case=='rejected' else 'absent' if case=='readback-missing' else 'unavailable' if case=='readback-failure' else 'confirmed',readback=None if case in ('rejected','readback-failure') else V.read_literal(case,'conversations.info')[1]['raw'])
            product={'attempts':[{'confirmationAdd':{'feature':dict(attempted=True,loading=False,visible=allowed,checked=allowed,notice=None,count=0 if allowed else None,operation=op)}}]}
            V.product_proof(case,'added',product,server)
            if op:
                op['readbackStatus']='confirmed' if case!='success' else 'absent'
                if case not in ('held',):
                    with self.assertRaises(RuntimeError):V.product_proof(case,'added',product,server)
    def test_actual_selection_custody_and_single_click(self):
        box=dict(ok=True,present=True,nodeId=12,original=True,connected=True,type='checkbox',checked=False,disabled=False,indeterminate=False,label='Feature this workflow in channel',help='This adds it as a button to the message composer.',visible=True,hit=True,epoch='8',rect={'left':1,'top':2,'right':17,'bottom':18,'width':16,'height':16},hitPoint={'x':9,'y':10})
        fields={'title':{'nodeId':2,'value':'Unicode \u4e2d'},'description':{'nodeId':3,'value':'retained'}}
        native={'fields':fields,'modal':{'nodeId':1,'originalNodes':True,'fields':copy.deepcopy(fields),'upload':{'nodeId':4}}}
        before={'model':{'title':'parent','description':'draft','confirmationDestination':{'documents':[{'id':1}]}},'server':sample('success','selected'),'firstPublish':{'attempts':[{}]},'native':native}
        after=copy.deepcopy(before)
        row={'model':after['model'],'proof':{k:after[k] for k in ('firstPublish','server','native')}}
        value=dict(case='success',choiceAfter=copy.deepcopy(before),before=before,after=after,checkboxBefore=box,checkboxAfter=dict(box,checked=True,epoch='9'),activation=dict(ok=True,kind='original-dom-click',nodeId=12,checked=True,click={'isTrusted':False,'targetNodeId':12},rect=box['rect']))
        check=lambda v:V.selection_proof('success',{'feature':v},row,lambda *_:None,{})
        check(value)
        mutations=[lambda v:v['before']['model'].update(title='changed'),lambda v:v['before']['native']['fields']['title'].update(nodeId=99),lambda v:v['before']['native']['modal']['upload'].update(nodeId=99),lambda v:v['activation']['click'].update(isTrusted=True),lambda v:v['checkboxAfter'].update(nodeId=99),lambda v:v['before']['server']['confirmationAdd']['feature']['requests'].append({})]
        for mutate in mutations:
            candidate=copy.deepcopy(value);mutate(candidate)
            with self.assertRaises(RuntimeError):check(candidate)

    def test_hidden_feature_dispatch_is_rejected(self):
        server=sample('denied');product={'attempts':[{'confirmationAdd':{'feature':dict(attempted=True,loading=False,visible=False,checked=False,notice=None,count=None,operation={'outcome':'acknowledged'})}}]}
        with self.assertRaises(RuntimeError):V.product_proof('denied','added',product,server)
    def test_typed_channel_defaults_not_raw_http_fabrication(self):
        value=destination('success');value['readback']['channelRaw']['last_read']='0'
        V.destination_proof('success',value)
        value['readback']['channelRaw']['properties']['channel_workflows'][0]['title']='forged'
        with self.assertRaises(RuntimeError):V.destination_proof('success',value)
    def test_original_browse_raw_required(self):
        value=destination('success');value['readback']['previewsRaw']['triggers'][0]['name']='forged'
        with self.assertRaises(RuntimeError):V.destination_proof('success',value)

    def test_denied_checkbox_must_be_absent(self):
        V.checkbox_proof(dict(ok=True,present=False,originalAbsent=True),False,False,lambda *_:None,{})
        with self.assertRaises(RuntimeError):V.checkbox_proof(dict(ok=True,present=False,originalAbsent=False),False,False,lambda *_:None,{})

if __name__=='__main__':unittest.main()
