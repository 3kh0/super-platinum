"""Pure member receipt regressions. Never launches a native process."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest

R=runpy.run_path(str(Path(__file__).with_name('check-profile-member-completion-native.py')))
C=runpy.run_path(str(Path(__file__).with_name('test_profile_channel_completion_native.py')))
TEXT={'#project-harbor':R['LOCAL'],'#project-completion':R['SAME'],'#project-cove':R['SAME'],
      'project':'alex','project-co':'harbor','project-held':'alex-held','project-cancel':'alex-cancel',
      'zz-no-matching-channel':'zz-no-matching-person','No matching channels':'No matching people',
      'Channel suggestions could not load. Edit your query to try again.':R['FAILURE']}
IDS={'CMENTION01':'UMENTION01','CCOMPLETE1':R['FIRST'],'CCOMPLETE2':R['SECOND']}
def member(value):
    if isinstance(value,list):return [member(v) for v in value]
    if isinstance(value,str):return TEXT.get(value,value)
    if not isinstance(value,dict):return value
    result={k:member(v) for k,v in value.items()}
    if result.get('type')=='channel':
        result['type']='user';result['user_id']=IDS[result.pop('channel_id')]
        if 'fixture_occurrence' in result:result['opaque']=dict(keep=[result['fixture_occurrence'],'original'])
    if 'channel' in result:result['user']=IDS[result.pop('channel')]
    if result.get('method')=='channels/search':
        q=result['args']['query'];result['method']='users/search'
        result['args']=dict(query=q,count=25,fuzz=1,enable_workspace_ranking=True,top_users=[],default_workspace='TINIT0001')
    if 'searches' in result:result.update(channelSearches=0,memberSearches=result['searches'],groupSearches=0)
    if isinstance(result.get('blocks'),str):result['blocks']=json.dumps(member(json.loads(result['blocks'])))
    return result
def sample(directory,narrow=False):
    fixture,value=C['sample'](directory,narrow);value=member(value)
    refreshed=value['stages'][2];item=copy.deepcopy(refreshed['geometry']['rows'][0])
    for label in ('@Alex Retired','@Alex Self'):
        extra=copy.deepcopy(item);extra['text']=label;refreshed['geometry']['rows'].append(extra)
    refreshed['proof']['acceptedMembers']=[dict(id=uid,deleted=uid in ('UCOMPLETE3','UCOMPLETE4'),is_bot=uid=='UCOMPLETE4',
        fixture_opaque=dict(preserve=uid,ordered=['first','second'])) for uid in (R['FIRST'],R['SECOND'],'UCOMPLETE3','UCOMPLETE4')]
    for index,keyboard in ((3,True),(4,False)):
        value['stages'][index]['proof']['choice']=dict(label=R['SAME'],matches=2,occurrence=0 if keyboard else 1,keyboard=keyboard,index=0 if keyboard else 1)
    extra=[]
    for stage in ('refreshed-middle','refreshed-bottom'):
        page=copy.deepcopy(refreshed);page['stage']=stage;extra.append(page)
        value['captures'][stage]=str(directory.resolve()/f'{fixture}-{stage}.png')
    value['stages'][3:3]=extra
    for page in value['stages']:
        g=page['geometry'];rows=g['rows']
        if not rows:continue
        target={'refreshed':0,'refreshed-middle':2,'refreshed-bottom':4}.get(page['stage'])
        total=len(rows)*32+8;height=min(total,72 if narrow else 128)
        top=0 if target is None else min(target*32,total-height)
        for i,row in enumerate(rows):
            y=104+i*32-top
            row.update(id=f'option-1-{i}',index=i,rect=dict(left=154,right=414,top=y,bottom=y+32,width=260,height=32),visible=y>=100 and y+32<=100+height)
        g['resultRows']=copy.deepcopy(rows);g['rows']=[r for r in rows if r['visible']]
        g['popup']=dict(id='popup-1',sequence=1,scrollTop=top,clientHeight=height,scrollHeight=total,targetIndex=target,
                        rect=dict(left=150,right=420,top=100,bottom=100+height,width=270,height=height))
    return fixture,value

class Validator(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.directory=Path(self.tmp.name);self.fixture,self.value=sample(self.directory)
    def validate(self):R['validate'](self.fixture,self.value,self.directory,platform='win32',png=False)
    def reject(self,fn):
        fn(self.value)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):self.validate()
    def test_normal(self):self.validate()
    def test_narrow(self):self.fixture,self.value=sample(self.directory,True);self.validate()
    def test_positive_empty_group_read_does_not_relax_member_only_tokens(self):
        source=self.value['source']
        source['requests'].append(dict(serial=len(source['requests'])+1,method='usergroups/search',
            args=dict(query='alex',count=25,org_wide=True)))
        source['searches']+=1;source['groupSearches']+=1;source['completed']+=1
        self.validate()
        self.value['stages'][6]['geometry']['native']['tokens'][3]['value']['type']='usergroup'
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):self.validate()

    def test_duplicate_same_label_identity(self):self.reject(lambda r:r['stages'][6]['geometry']['native']['tokens'][3]['value'].__setitem__('user_id',R['FIRST']))
    def test_initial_opaque(self):self.reject(lambda r:r['stages'][0]['geometry']['native']['snapshot']['blocks'][0]['elements'][0]['elements'][1].pop('opaque'))
    def test_initial_style(self):self.reject(lambda r:r['stages'][0]['geometry']['native']['snapshot']['blocks'][0]['elements'][0]['elements'][1].pop('style'))
    def test_wrong_ordinal(self):self.reject(lambda r:r['stages'][6]['proof']['choice'].__setitem__('occurrence',0))
    def test_missing_remote_source_fields(self):self.reject(lambda r:r['stages'][2]['proof']['acceptedMembers'][0].pop('fixture_opaque'))
    def test_deleted_bot(self):self.reject(lambda r:r['stages'][2]['geometry']['rows'].append({**r['stages'][2]['geometry']['rows'][0],'text':'@Alex Deleted Bot'}))
    def test_remote_query_replaced(self):self.reject(lambda r:r['stages'][2]['proof'].__setitem__('sequence',9))
    def test_count20(self):self.reject(lambda r:r['stages'][1]['source']['requests'][0]['args'].__setitem__('count',20))
    def test_added_wire_filter(self):self.reject(lambda r:r['stages'][1]['source']['requests'][0]['args'].__setitem__('filter','NOT deactivated'))
    def test_channel_request(self):self.reject(lambda r:r['stages'][1]['source']['requests'][0].__setitem__('method','channels/search'))
    def test_cancel_delivered(self):self.reject(lambda r:r['stages'][12]['proof']['release'].__setitem__('delivered',True))
    def test_error_hidden(self):self.reject(lambda r:r['stages'][9]['geometry']['notices'][0].__setitem__('visible',False))
    def test_stale_selected(self):self.reject(lambda r:r['stages'][13]['proof'].__setitem__('nextSequence',20))
    def test_saved_wire(self):self.reject(lambda r:r['stages'][7]['model']['attempts'][0][0]['long_text'].__setitem__('blocks','[]'))
    def test_missing_scroll_page(self):self.reject(lambda r:r['stages'].pop(3))
    def test_scrolled_identity_changed(self):self.reject(lambda r:r['stages'][3]['geometry']['resultRows'][4].__setitem__('id','new-result'))
    def test_scrolled_query_changed(self):self.reject(lambda r:r['stages'][3]['geometry']['popup'].__setitem__('sequence',2))
    def test_scroll_did_not_reach_bottom(self):self.reject(lambda r:r['stages'][4]['geometry']['popup'].__setitem__('scrollTop',0))
    def test_scroll_preserves_native(self):self.reject(lambda r:r['stages'][3]['geometry']['native']['snapshot'].__setitem__('revision',5))
    def test_visible_row_clipped_by_list(self):self.reject(lambda r:r['stages'][2]['geometry']['rows'][0]['rect'].update(top=90))
    def test_hidden_row_cannot_be_counted_visible(self):self.reject(lambda r:r['stages'][2]['geometry']['rows'].append(r['stages'][2]['geometry']['resultRows'][-1]))
    def test_no_launch_on_import(self):self.assertEqual(R['B']['PREFIX'],'profile-member-completion-check: ')
    def test_shared_scope_is_member(self):self.assertEqual(R['B']['SCOPE'],'ordinary-member-completion')

class SourceLedger(unittest.TestCase):
    def source(self):
        return dict(networkCalls=0, requests=[
            dict(serial=1,method='users/search',args=dict(query='alex',count=25,fuzz=1,
                enable_workspace_ranking=True,top_users=[],default_workspace='TINIT0001')),
            dict(serial=2,method='usergroups/search',args=dict(query='alex',count=25,org_wide=True))],
            searches=2,memberSearches=1,groupSearches=1,channelSearches=0,
            active=0,completed=2,cancelled=0,heldQueries=[],held=False)
    def reject(self,change):
        source=self.source();change(source)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):R['validate_source'](source)
    def test_exact_group_factory_and_total_lifecycle(self):
        source=self.source();self.assertEqual(R['validate_source'](source),source['requests'])
    def test_empty_group_query_allowed(self):
        source=self.source();source['requests'][1]['args']['query']='';R['validate_source'](source)
    def test_held_group_is_in_total_active_count(self):
        source=self.source();source.update(active=1,completed=1,held=True,heldQueries=['alex']);R['validate_source'](source)
    def test_no_implicit_legacy_group_counter(self):self.reject(lambda s:s.pop('groupSearches'))
    def test_wrong_orgwide(self):self.reject(lambda s:s['requests'][1]['args'].__setitem__('org_wide',False))
    def test_numeric_orgwide_is_not_observed_true(self):self.reject(lambda s:s['requests'][1]['args'].__setitem__('org_wide',1))
    def test_wrong_group_count(self):self.reject(lambda s:s['requests'][1]['args'].__setitem__('count',20))
    def test_float_group_count(self):self.reject(lambda s:s['requests'][1]['args'].__setitem__('count',25.0))
    def test_unobserved_group_filter(self):self.reject(lambda s:s['requests'][1]['args'].__setitem__('include_disabled',True))
    def test_missing_group_argument(self):self.reject(lambda s:s['requests'][1]['args'].pop('org_wide'))
    def test_unformatted_group_query(self):self.reject(lambda s:s['requests'][1]['args'].__setitem__('query','@alex'))
    def test_nonstring_group_query(self):self.reject(lambda s:s['requests'][1]['args'].__setitem__('query',None))
    def test_group_accounted_as_member(self):self.reject(lambda s:s.update(memberSearches=2,groupSearches=0))
    def test_group_missing_from_total(self):self.reject(lambda s:s.update(searches=1))
    def test_group_missing_from_lifecycle(self):self.reject(lambda s:s.update(completed=1))
    def test_boolean_count_not_integer(self):self.reject(lambda s:s.update(groupSearches=True))
    def test_group_request_secret_rejected(self):self.reject(lambda s:s['requests'][1]['args'].__setitem__('token','synthetic'))

if __name__=='__main__':unittest.main()
