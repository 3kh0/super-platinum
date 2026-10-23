"""Focused source/ownership mutations; modeled receipts are not native evidence."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch
import profile_link_completion_source as S
import profile_link_title_completion as O
import test_profile_empty_canvas_title as T

ROOT=Path(__file__).resolve().parent.parent
H=runpy.run_path(str(Path(__file__).with_name('check-profile-file-completion-native.py')))
W=runpy.run_path(str(Path(__file__).with_name('check-profile-link-title-completion-native.py')))


def sample(narrow=False):
    fixture,value=T.completion(narrow)
    def labels(node):
        if isinstance(node,dict):
            for key,item in node.items():
                if key in ('label','text') and item=='Untitled':node[key]=S.LABEL
                else:labels(item)
        elif isinstance(node,list):
            for item in node:labels(item)
    labels(value)
    value.pop('emptyCanvas');value['linkTitle']=S.snapshot()
    raw=S.completion_bytes();files=json.loads(raw)['files']
    for row in value['stages']:
        proof=row['proof'];proof.pop('emptyCanvas');proof['linkTitle']=S.snapshot()
        for request in row['source']['requests']:
            if request['method']=='search.autocomplete.files':request['raw']=raw
            if request['method']=='files.info':request['raw']=dict(ok=True,file=copy.deepcopy(files[0]))
        if proof['sourceFiles']:
            for new,old in zip(files,proof['sourceFiles']['rows']):
                channels=old['effective']['channels']
                old.update(source=copy.deepcopy(new),effective=dict(new,channels=channels))
    value['source']=copy.deepcopy(value['stages'][-1]['source'])
    return fixture,value


class LinkCompletionTests(unittest.TestCase):
    def setUp(self):self.fixture,self.value=sample()
    def validate(self):O.validate(self.fixture,self.value,T.PATH,H,platform='win32',png=False)
    def test_normal(self):self.validate()
    def test_narrow(self):self.fixture,self.value=sample(True);self.validate()
    def test_original_response_order_and_rank_preserved(self):
        original=(ROOT/'src/desktop/profile_edit/sections/fixture/completion/files-response.json').read_bytes().decode('utf-8')
        self.assertEqual(original,S.ORIGINAL_COMPLETION_BYTES)
        parsed=json.loads(S.completion_bytes())
        first=parsed['files'][0]
        self.assertEqual(first['title'],'Alex Canvas');self.assertEqual(first['name'],'Alex Canvas')
        self.assertEqual(list(first['channel_map']),['z-last','a-first','m-foreign'])
        self.assertEqual(parsed['files'][1:],json.loads(original)['files'][1:])
    def test_exact_closed_harness_and_flags(self):
        base=W['harness']()
        self.assertEqual(base['PREFIX'],'profile-file-completion-check: ')
        self.assertEqual(base['EDGE'],'profile-channel-completion-capture: ')
        self.assertEqual(base['STAGES'],S.COMPLETION_STAGES)
        self.assertIs(base['validate_closed'].__globals__,base)
        cls=base['NativeFixture'];parent=cls.__bases__[0];seen=[]
        with patch.object(parent,'__init__',lambda self,*a,**k:seen.append((a,k))):
            cls('profile-sections-mentions',Path('unused'),Path('unused'),{})
        self.assertEqual(seen[0][0][-1],{S.FLAG:'1'})
    def test_conflicting_mode_refused(self):
        for flag in (S.FLAG,'SUPER_PLATINUM_PROFILE_FILE_TITLES_CHECK','SUPER_PLATINUM_PROFILE_EMPTY_CANVAS_TITLE_CHECK',
                     'SUPER_PLATINUM_PROFILE_USER_TITLE_COMPLETION_CHECK','SUPER_PLATINUM_PROFILE_FILE_COMPLETION_OFF'):
            with self.assertRaises(RuntimeError):W['flags']({flag:'1'})
    def test_original_context_gate(self):
        text=(ROOT/'src/desktop/profile_edit/sections/fixture/link_completion.rs').read_text()
        for expected in ('feature = "protected-native-fixture"','completion::files::named()',
                         'std::env::var(mentions::FLAG)','SUPER_PLATINUM_PROFILE_USER_MENTIONS_CHECK',S.FLAG):
            self.assertIn(expected,text)


def change(value,path,replacement):
    for key in path[:-1]:value=value[key]
    value[path[-1]]=replacement


MUTATIONS={
 'foreign_owner':(['stages',1,'model','account'],'foreign'),
 'stale_section_readback':(['stages',3,'model','serial'],3),
 'query_sequence_replaced':(['stages',1,'proof','exercise','native','sequence'],0),
 'query_choice_wire':(['stages',1,'proof','choices','values',0,'value'],{}),
 'source_projection_unavailable':(['stages',1,'proof','sourceFiles','unavailableIds'],['FCOMPLETE1']),
 'source_opaque':(['stages',1,'proof','sourceFiles','rows',0,'source','title_blocks',0,'opaque'],{}),
 'effective_link_decoded':(['stages',1,'proof','sourceFiles','rows',0,'effective','title_blocks',0,'elements',0,'elements',0,'text'],'Review & %2F'),
 'effective_plain_rank':(['stages',1,'proof','sourceFiles','rows',0,'effective','title'],'Review'),
 'query_quip_foreign':(['stages',1,'proof','choices','canvasFallback','FCOMPLETE1','quipTitle','fileId'],'FCOMPLETE2'),
 'query_quip_retired':(['stages',1,'proof','choices','canvasFallback','FCOMPLETE1','quipTitle','current'],False),
 'query_quip_missing':(['stages',1,'proof','choices','canvasFallback','FCOMPLETE1','quipTitle'],None),
 'fallback_retired':(['stages',1,'proof','choices','canvasFallback','FCOMPLETE1','current'],False),
 'invented_member_title':(['stages',1,'proof','choices','titleUsers'],{'FCOMPLETE1':{'current':True}}),
 'selection_old_sequence':(['stages',2,'proof','exercise','choice','sequence'],0),
 'selection_url_as_label':(['stages',2,'proof','exercise','choice','label'],'https://example.test/not-the-label'),
 'inserted_decodes_entity':(['stages',2,'geometry','tokens',2,'text'],S.LABEL.replace('&amp;','&')),
 'saved_decodes_percent':(['stages',3,'geometry','tokens',2,'text'],S.LABEL.replace('%2F','/')),
 'saved_outer_wire_label':(['stages',3,'model','blocks',0,'elements',0,'elements',3,'text'],S.LABEL),
 'reopened_missing':(['stages',3,'proof','exercise','reopened'],{}),
 'hidden_inserted_token':(['stages',2,'geometry','tokens',2,'visible'],False),
 'extra_file_request':(['stages',3,'source','fileSearches'],2),
 'extra_network':(['stages',3,'source','networkCalls'],1),
 'missing_stage':(['stages',3,'stage'],'inserted'),
 'raw_expected_changed':(['linkTitle','completionResponseBytes'],'{}'),
}
for name,(path,replacement) in MUTATIONS.items():
    def test(self,path=path,replacement=replacement):
        change(self.value,path,replacement)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):self.validate()
    setattr(LinkCompletionTests,'test_reject_'+name,test)


def request_test(method,mutation):
    def test(self):
        rows=self.value['stages'][-1]['source']['requests']
        request=next(r for r in rows if r['method']==method)
        mutation(request,rows)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):self.validate()
    return test

setattr(LinkCompletionTests,'test_reject_independent_info_missing',request_test('files.info',lambda r,rows:rows.remove(r)))
setattr(LinkCompletionTests,'test_reject_independent_info_foreign',request_test('files.info',lambda r,rows:r['args'].update(file='FCOMPLETE2')))
setattr(LinkCompletionTests,'test_reject_independent_raw_replaced',request_test('files.info',lambda r,rows:r.update(raw=dict(ok=True,file={}))))
setattr(LinkCompletionTests,'test_reject_search_bytes_reserialized',request_test('search.autocomplete.files',lambda r,rows:r.update(raw=json.dumps(json.loads(r['raw']),sort_keys=True))))

if __name__=='__main__':unittest.main()
