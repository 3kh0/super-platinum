"""Focused mutations of source, media, node custody and the original closed harness."""
import copy
from pathlib import Path
import runpy
import unittest
import rich_emoji_display_source as V

BASE=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_edits.py')))
DIRECTORY=Path('/private/display-source')

def sample(narrow=False):
    fixture,old=BASE['sample']('edits',narrow)
    raw={'ok':True,'channel':'CCOMMENT01','messages':[
        {'ts':'1781524800.000001','subtype':'list_record_comment','slack_list':{'list_id':'FCOMMENT01','list_record_id':'R_LIST_000'},'text':'Record comment root','unknown':{'keep':True}},
        {'ts':'1781524801.000001','thread_ts':'1781524800.000001','user':'U1','text':'Shared :external_original: Standard :wave:',
         'blocks':[{'type':'rich_text','block_id':'original-external-emoji','opaque':{'keep':True},'elements':[{'type':'rich_text_section','elements':[
             {'type':'text','text':'Shared '},{'type':'emoji','name':'external_original','display_alias':'shared_party','display_url':'https://fixture.invalid/shared-emoji-a.png','display_team_id':'TFOREIGN001','opaque':{'keep':'original appearance'}},
             {'type':'text','text':' Standard '},{'type':'emoji','name':'wave'}]}]}],
         'reactions':[{'name':'wave','count':2,'users':['U1','U2']}],'display_fixture_opaque':{'keep':'received'}}],
         'has_more':True,'response_metadata':{'next_cursor':'comments-next+/='}}
    root,first=map(V.I['typed'],raw['messages']);new=copy.deepcopy(first)
    new.update(type='message',channel='CCOMMENT01',files=[],attachments=[],subtype=None,latest_reply=None,reply_users=[],reply_users_count=None,reply_count=0,no_display=False,_hidden_reply=True,slackbot_feels=None)
    new['blocks'][0]['elements'][0]['elements'][1]['display_url']='https://fixture.invalid/shared-emoji-b.png'
    rows=[]
    for i,name in enumerate(('pending','ready','replacement-pending','replacement-ready')):
        row=copy.deepcopy(old['stages'][0]);m,g,p=row['model'],row['geometry'],row['proof']
        row.update(stage=name,capture=str(DIRECTORY/f'{fixture}-{name}.png'))
        m['comments'].update(page=copy.deepcopy(raw),pageMessages=copy.deepcopy([root,first if i<2 else new]))
        row['journal']=copy.deepcopy(p.pop('journal'))
        g.pop('editBody');g['rows'][0]['text']='Shared :shared_party: Standard \U0001f44b'
        box={'left':20,'top':20,'right':40,'bottom':40,'width':20,'height':20}
        reply={'node':1,'connected':True,'sameNode':True,'anchor':2,'focus':2,'anchorOffset':1,'focusOffset':3,'sameSelection':True,'blocks':copy.deepcopy(p['originalA']),'revision':2,'composing':False,'editable':True}
        ready=i%2==1
        images=[] if not ready else [{'node':10+i,'src':f'http://super-platinum-media.emoji/{"a" if i==1 else "b"}?v={2 if i==1 else 4}',
            'alt':':shared_party:','title':':shared_party:','complete':True,'width':16 if i==1 else 32,'height':16 if i==1 else 32,'visible':True,'rect':box}]
        wave={'node':4,'text':'\U0001f44b','title':':wave:','visible':True,'rect':box}
        spans=[wave] if ready else [{'node':5+i,'text':':shared_party:','title':':shared_party:','visible':True,'rect':box},wave]
        n={'ok':True,'ready':ready,'row':6,'body':7,'bodyText':'Shared  Standard \U0001f44b' if ready else 'Shared :shared_party: Standard \U0001f44b','images':images,'spans':spans,'reply':reply,'geometry':copy.deepcopy(g)}
        row['native']={'before':n,'after':copy.deepcopy(n)}
        row['source']={'named':True,'noSession':True,'noTransport':True,'customEmoji':{'previous':{'name':'previous'}},'customRevision':3,'mediaEpoch':i+1}
        p['arm']={'ok':True,'reply':copy.deepcopy(reply)}
        if i>=1:p['mediaA']={'url':'https://fixture.invalid/shared-emoji-a.png','id':'super-platinum-media://emoji/a','uriBefore':'http://super-platinum-media.emoji/a?v=1','epochBefore':1,'readyBefore':False,'readyAfter':True,'originalId':'super-platinum-media://emoji/a','originalStillReady':True,'replacement':False,'bytes':(V.REPO/'assets/icons/png/icon_16.png').stat().st_size,'mime':'image/png'}
        if i==3:p['mediaB']={'url':'https://fixture.invalid/shared-emoji-b.png','id':'super-platinum-media://emoji/b','uriBefore':'http://super-platinum-media.emoji/b?v=3','epochBefore':3,'readyBefore':False,'readyAfter':True,'originalId':'super-platinum-media://emoji/a','originalStillReady':True,'replacement':True,'bytes':(V.REPO/'assets/icons/png/icon_32.png').stat().st_size,'mime':'image/png'}
        rows.append(row)
    event={'stage':'replacement','team':'T1','generation':7,'frame':{'type':'message','subtype':'message_changed','channel':'CCOMMENT01','message':{'ts':'1781524801.000001','thread_ts':'1781524800.000001','user':'U1','text':'Shared :external_original: Standard :wave:','blocks':copy.deepcopy(new['blocks'])}},'parsed':True,'changed':True}
    event['witness']={'kind':'Changed','outerChannel':'CCOMMENT01','typedChannel':'CCOMMENT01','typedTs':'1781524801.000001','raw':copy.deepcopy(event['frame']['message'])}
    for key,index in [('before',1),('after',2)]:event[key]={k:copy.deepcopy(rows[index]['model']['comments'][k]) for k in V.E['EVENT_KEYS']}
    for row in rows[2:]:row['proof']['replacement']=copy.deepcopy(event)
    return fixture,dict(ok=True,case=V.CASE,fixture=fixture,noLiveNetwork=True,readiness=old['readiness'],initialRecords=old['initialRecords'],stages=rows,editEvents=[event],final=copy.deepcopy(rows[-1]['model']))

class Checks(unittest.TestCase):
    def check(self,f,r):return V.validate(f,r,DIRECTORY,png=False)
    def reject(self,change):
        f,r=sample();change(r)
        with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError)):self.check(f,r)
    def test_valid_widths(self):
        for narrow in (False,True):self.check(*sample(narrow))
    def test_original_raw_not_rewritten(self):self.reject(lambda r:r['stages'][2]['model']['comments']['page']['messages'][1]['blocks'][0].update(block_id='replaced'))
    def test_typed_source_must_replace_url(self):self.reject(lambda r:r['stages'][2]['model']['comments']['pageMessages'][1]['blocks'][0]['elements'][0]['elements'][1].update(display_url=V.URLS[0]))
    def test_global_catalog_not_published(self):self.reject(lambda r:r['stages'][1]['source']['customEmoji'].update(shared_party={}))
    def test_same_alias_cannot_reuse_ready_asset(self):self.reject(lambda r:r['stages'][3]['proof']['mediaB'].update(id=r['stages'][1]['proof']['mediaA']['id']))
    def test_draft_mutation(self):self.reject(lambda r:r['stages'][2]['journal'].update(blocks=[]))
    def test_caret_mutation(self):self.reject(lambda r:[r['stages'][1]['native'][side]['reply'].update(anchorOffset=2) for side in ('before','after')])
    def test_old_image_during_replacement(self):self.reject(lambda r:[r['stages'][2]['native'][side].update(images=copy.deepcopy(r['stages'][1]['native'][side]['images'])) for side in ('before','after')])
    def test_wrong_decoded_bytes(self):self.reject(lambda r:[r['stages'][3]['native'][side]['images'][0].update(width=16,height=16) for side in ('before','after')])
    def test_external_image_rejected(self):self.reject(lambda r:[r['stages'][3]['native'][side]['images'][0].update(src=V.URLS[1]) for side in ('before','after')])
    def test_no_extra_reads(self):self.reject(lambda r:r['stages'][2]['model']['ledger']['requests'].append(copy.deepcopy(r['stages'][2]['model']['ledger']['requests'][0])))
    def test_no_send(self):self.reject(lambda r:r['stages'][2]['model']['composeLedger']['posts'].append({}))
    def test_event_owner(self):self.reject(lambda r:r['editEvents'][0].update(generation=8))
    def test_closed_database(self):
        _,r=sample();value=dict(account=r['final']['account'],inventories=[r['stages'][-1]['journal']],editInventories=[],editHistory=[],contentSha256='a'*64)
        V.validate_closed(value,r);value['editHistory']=[{}]
        with self.assertRaises(RuntimeError):V.validate_closed(value,r)
    def test_wrapper_guards_preserved(self):
        wrapper=runpy.run_path(str(Path(__file__).with_name('check-rich-emoji-display-source-native.py')))
        base=wrapper['harness']();self.assertEqual(base['case_contract'](V.CASE)[0],V.STAGES)
        self.assertEqual(wrapper['storage_controller'](V.CASE)['StorageControl'](DIRECTORY,{},case=V.CASE).command_count,0)
        with self.assertRaises(RuntimeError):wrapper['storage_controller']('other')

if __name__=='__main__':unittest.main()
