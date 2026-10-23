"""Independent combined-source receipt mutations; no native execution."""
import copy
from pathlib import Path
import runpy
import sys
import unittest
import rich_emoji_combined as V

BASE=runpy.run_path(str(Path(__file__).with_name('test_rich_emoji_display_source.py')))
DIRECTORY=Path('/private/combined-emoji')

def sample(narrow=False):
    fixture,old=BASE['sample'](narrow)
    blocks=[{'type':'rich_text','block_id':'original-combined-emoji','opaque':{'keep':True},'elements':[{'type':'rich_text_section','elements':[
        {'type':'text','text':'Known '},
        {'type':'emoji','name':'conflicting_original','unicode':'1f44d-1f3fd','skin_tone':6,'display_alias':'wrong_alias','display_url':'https://fixture.invalid/combined-emoji-a.png','opaque':{'keep':'known'}},
        {'type':'text','text':' Unknown '},{'type':'emoji','name':'unknown_original','unicode':'0041','display_alias':'wrong_unknown','display_url':'https://fixture.invalid/unknown-never-image.png'},
        {'type':'text','text':' Invalid '},{'type':'emoji','name':'invalid_original','unicode':'1F44D','display_alias':'wrong_invalid','display_url':'https://fixture.invalid/invalid-never-image.png'},
        {'type':'text','text':' Standard '},{'type':'emoji','name':'wave'}]}]}]
    raw=copy.deepcopy(old['stages'][0]['model']['comments']['page'])
    first=raw['messages'][1];first['text']='Known :conflicting_original: Unknown :unknown_original: Invalid :invalid_original: Standard :wave:';first['blocks']=blocks;first['combined_fixture_opaque']=first.pop('display_fixture_opaque')
    root,original=map(V.I['typed'],raw['messages']);changed=copy.deepcopy(original)
    changed.update(type='message',channel='CCOMMENT01',files=[],attachments=[],subtype=None,latest_reply=None,reply_users=[],reply_users_count=None,reply_count=0,no_display=False,_hidden_reply=True,slackbot_feels=None)
    changed['blocks'][0]['elements'][0]['elements'][1]['display_url']='https://fixture.invalid/combined-emoji-b.png'
    user={'id':'UCOMBINED01','name':'combined-self','team_id':'T1','profile':{}}
    seed={'accepted':True,'boot':{'ok':True,'team':{'id':'T1','name':'Combined emoji fixture'},'self':user,'users':[user],'channels':[{'id':'CCOMMENT01','name':'combined-comments','team_id':'T1','is_channel':True,'is_member':True,'updated':100,'previous_names':[]}],'prefs':{}},
        'init':{'ok':True,'self':user,'team':{'id':'T1','name':'Combined emoji fixture'},'workspaces':[{'id':'T1'}],'onboarding':{'user_data':{'onboarding_complete':True}},'user_date_created':123}}
    raw_assignments={'ok':True,'assignments':{'emoji_v17':{'group':'on'}}}
    selected={'current':True,'version':'v17','response':raw_assignments,'owner':{'account':old['final']['account'],'epoch':old['final']['epoch'],'team':'T1','user':'UCOMBINED01','socket':7,'slotSerial':2}}
    isolated=copy.deepcopy(selected);isolated['owner'].update(account='00000000-0000-4000-8000-000000000081',team='TINIT0001',user='UINIT0001',socket=0,slotSerial=1)
    guard={'scope':'isolated-source-guard','mountedOwnerMutated':False,'transportDisconnected':True,'networkDispatched':False,'before':isolated,'after':dict(copy.deepcopy(isolated),current=False),'socketAfter':1,'originalContextCurrent':False,'accessorAvailable':False}
    rows=[]
    for i,(name,prior) in enumerate(zip(('unobserved','pending','ready','replacement-pending','replacement-ready'),(0,0,1,2,3))):
        row=copy.deepcopy(old['stages'][prior]);m,g,p=row['model'],row['geometry'],row['proof'];row.update(stage=name,capture=str(DIRECTORY/f'{fixture}-{name}.png'))
        m.update(noSession=False,noTransport=False);m['comments'].update(page=copy.deepcopy(raw),pageMessages=copy.deepcopy([root,changed if i>=3 else original]))
        for scope in (row['journal']['scope'],m['draftStore']['drafts'][0]['scope'],p['seed']['scope']):scope['user']='UCOMBINED01'
        p.pop('replacement',None);p['isolatedSourceGuard']=copy.deepcopy(guard)
        if i:p['assignments']=copy.deepcopy(selected)
        for key in ('mediaA','mediaB'):
            if key in p:p[key]['url']=p[key]['url'].replace('shared-emoji-','combined-emoji-')
        n=row['native']['before'];n['observed']=i>0
        n['bodyText']='Known '+('\U0001f44d\U0001f3fd' if i==0 else '' if i in (2,4) else ':+1::skin-tone-4:')+' Unknown A Invalid  Standard \U0001f44b'
        box=n['spans'][0]['rect'];unknown={'node':25,'text':'A','title':':unknown_original:','visible':True,'rect':copy.deepcopy(box)}
        if i not in (2,4):n['spans'][0].update(text='\U0001f44d\U0001f3fd' if i==0 else ':+1::skin-tone-4:',title=':conflicting_original:' if i==0 else ':+1::skin-tone-4:')
        n['spans'].insert(-1,unknown)
        if i in (2,4):n['images'][0].update(alt=':+1::skin-tone-4:',title=':+1::skin-tone-4:')
        row['native']['after']=copy.deepcopy(n)
        platform='windows' if sys.platform=='win32' else 'darwin' if sys.platform=='darwin' else 'linux'
        row['source'].update(noSession=False,noTransport=False,syntheticContext=True,noLiveNetwork=True,httpCapability=False,seed=copy.deepcopy(seed),standard=copy.deepcopy(selected) if i else None,
            requests=[{'request':{'method':'POST','path':'/api/experiments.getByUser','args':{'ssb_platform':platform,'ssb_release_channel':'prod'}},'response':copy.deepcopy(raw_assignments),'tokenPresent':True,'factoryMatched':True}] if i else [],
            registered={'a':int(i>0),'b':int(i>=3),'unknown':0,'invalid':0})
        rows.append(row)
    event=copy.deepcopy(old['editEvents'][0]);event['frame']['message']['text']=first['text'];event['frame']['message']['blocks']=copy.deepcopy(changed['blocks']);event['witness']['raw']=copy.deepcopy(event['frame']['message'])
    for key,index in [('before',2),('after',3)]:event[key]={k:copy.deepcopy(rows[index]['model']['comments'][k]) for k in V.E['EVENT_KEYS']}
    for row in rows[3:]:row['proof']['replacement']=copy.deepcopy(event)
    return fixture,dict(old,case=V.CASE,stages=rows,editEvents=[event],final=copy.deepcopy(rows[-1]['model']))

class Checks(unittest.TestCase):
    def check(self,f,r):return V.validate(f,r,DIRECTORY,png=False)
    def reject(self,change):
        f,r=sample();change(r)
        with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError)):self.check(f,r)
    def test_both_widths(self):
        for narrow in (False,True):self.check(*sample(narrow))
    def test_unobserved_map_not_default(self):self.reject(lambda r:r['stages'][0]['source'].update(standard=copy.deepcopy(r['stages'][1]['source']['standard'])))
    def test_unknown_url_not_registered(self):self.reject(lambda r:r['stages'][1]['source']['registered'].update(unknown=1))
    def test_invalid_url_not_registered(self):self.reject(lambda r:r['stages'][1]['source']['registered'].update(invalid=1))
    def test_original_name_does_not_override_known(self):self.reject(lambda r:[r['stages'][2]['native'][side]['images'][0].update(title=':wrong_alias:') for side in ('before','after')])
    def test_unknown_literal_not_alias(self):self.reject(lambda r:[r['stages'][2]['native'][side]['spans'][0].update(text=':wrong_unknown:') for side in ('before','after')])
    def test_missing_original_factory(self):self.reject(lambda r:r['stages'][1]['source']['requests'][0].update(factoryMatched=False))
    def test_wrong_selected_assignment(self):self.reject(lambda r:r['stages'][1]['source']['requests'][0]['response']['assignments']['emoji_v17'].update(group='off'))
    def test_retired_map_refused(self):self.reject(lambda r:r['stages'][3]['source']['standard'].update(current=False))
    def test_wrong_map_owner(self):self.reject(lambda r:r['stages'][3]['source']['standard']['owner'].update(socket=8))
    def test_isolated_guard_not_mounted_claim(self):self.reject(lambda r:r['stages'][0]['proof']['isolatedSourceGuard'].update(mountedOwnerMutated=True))
    def test_isolated_late_accessor_refused(self):self.reject(lambda r:r['stages'][0]['proof']['isolatedSourceGuard'].update(accessorAvailable=True))
    def test_durable_reply_changed(self):self.reject(lambda r:r['stages'][3]['journal'].update(blocks=[]))
    def test_caret_changed(self):self.reject(lambda r:[r['stages'][3]['native'][side]['reply'].update(anchorOffset=0) for side in ('before','after')])
    def test_old_image_cannot_survive_new_url(self):self.reject(lambda r:r['stages'][4]['proof']['mediaB'].update(id=r['stages'][2]['proof']['mediaA']['id']))
    def test_no_extra_reads(self):self.reject(lambda r:r['stages'][1]['model']['ledger']['requests'].append({}))
    def test_no_posts(self):self.reject(lambda r:r['stages'][2]['model']['composeLedger']['posts'].append({}))
    def test_original_page_not_rewritten(self):self.reject(lambda r:r['stages'][3]['model']['comments']['page']['messages'][1].update(blocks=[]))
    def test_closed_sqlite(self):
        _,r=sample();v={'account':r['final']['account'],'inventories':[r['stages'][-1]['journal']],'editInventories':[],'editHistory':[],'contentSha256':'a'*64}
        V.validate_closed(v,r);v['inventories']=[]
        with self.assertRaises(RuntimeError):V.validate_closed(v,r)
    def test_owned_wrapper(self):
        wrapper=runpy.run_path(str(Path(__file__).with_name('check-rich-emoji-combined-native.py')))
        self.assertEqual(wrapper['harness']()['case_contract'](V.CASE)[0],V.STAGES)
        self.assertEqual(wrapper['storage_controller'](V.CASE)['StorageControl'](DIRECTORY,{},case=V.CASE).command_count,0)
        with self.assertRaises(RuntimeError):wrapper['storage_controller']('other')

if __name__=='__main__':unittest.main()
