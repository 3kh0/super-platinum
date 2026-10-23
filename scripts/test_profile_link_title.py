"""Modeled receipt mutations; native rendering remains root-owned acceptance."""
import copy
import hashlib
import json
from pathlib import Path
import runpy
import tempfile
import unittest
import profile_link_title as O

ROOT=Path(__file__).resolve().parent.parent
PATH=Path(tempfile.gettempdir())/'profile-link-title-oracle-only'
WRAPPER=runpy.run_path(str(Path(__file__).with_name('check-profile-link-title-native.py')))
SAMPLE=runpy.run_path(str(Path(__file__).with_name('test_profile_file_titles_native.py')))['sample']
SAMPLE.__globals__['R']=O.G


def sample(narrow=False):
    fixture,value=SAMPLE(PATH,narrow)
    value['linkTitle']=O.snapshot()
    for row in value['stages']:
        row['proof']['linkTitle']=O.snapshot()
        cache=row['proof']['cache']
        if cache:
            changed=row['stage'] in ('renamed-mounted','confirmed')
            cache['source']=O.raw_file(changed)['file']
            cache['canvasFallback']=dict(current=cache['current'],descriptor=copy.deepcopy(O.DESCRIPTOR))
    return fixture,value


class LinkTitleTests(unittest.TestCase):
    def setUp(self):self.fixture,self.value=sample()
    def validate(self):O.validate(self.fixture,self.value,PATH,platform='win32',png=False)
    def test_positive_normal(self):self.validate()
    def test_positive_narrow(self):self.fixture,self.value=sample(True);self.validate()
    def test_original_baseline_bytes_unchanged(self):
        for row in json.loads((ROOT/'tmp/profile-link-title-20261022/fixture/before.json').read_text()):
            if row['path'].startswith('scripts/'):
                self.assertEqual(hashlib.sha256((ROOT/row['path']).read_bytes()).hexdigest(),row['sha256'])
    def test_isolated_full_original_validator(self):
        old=runpy.run_path(str(Path(__file__).with_name('check-profile-file-titles-native.py')))
        old['MODE'].update(list=False,blocks=True)
        self.assertEqual(old['label'](),'&amp; *literal*')
        self.assertEqual(O.G['label'](),O.ORIGINAL)
        base=WRAPPER['harness']()
        self.assertEqual(base['PREFIX'],'profile-file-titles-check: ')
        self.assertEqual(base['stages'](),O.STAGES)
        self.assertIs(base['validate'],O.validate)
        for key in ('validate_closed','validate_owned','run_fixture'):
            self.assertEqual(base[key].__code__.co_filename,old['B'][key].__code__.co_filename)
    def test_exact_flag_and_conflict_refusal(self):
        self.assertEqual(WRAPPER['flags']({}),{O.FLAG:'1'})
        for flag in (O.FLAG,'SUPER_PLATINUM_PROFILE_EMPTY_CANVAS_TITLE_CHECK',
                     'SUPER_PLATINUM_PROFILE_USER_TITLE_CHECK','SUPER_PLATINUM_PROFILE_FILE_COMPLETION_CHECK'):
            with self.assertRaises(RuntimeError):WRAPPER['flags']({flag:'1'})
    def test_source_gate_and_shared_receipts_scoped(self):
        source=(ROOT/'src/desktop/profile_edit/sections/fixture/link_title.rs').read_text()
        for required in ('feature = "protected-native-fixture"','mentions::named()',
                         'std::env::var(mentions::FLAG)','std::env::var(files::FLAG)',
                         'std::env::var(files::BLOCKS_FLAG)','!files::list()',O.FLAG):
            self.assertIn(required,source)
        self.assertNotIn('Transport::execute',source)
        self.assertNotIn('pub(crate) async fn',source)


def mutate(value,path,replacement):
    for key in path[:-1]:value=value[key]
    value[path[-1]]=replacement


CASES={
 'entity_decoding':(['stages',1,'geometry','tokens',0,'text'],'& *x* A%2F E N [ ]'),
 'markdown_parse':(['stages',1,'geometry','tokens',0,'text'],'&amp; x A%2F E N [ ]'),
 'url_decode':(['stages',1,'geometry','tokens',0,'text'],'&amp; *x* A/F E N [ ]'),
 'whitespace_url_fallback':(['stages',1,'geometry','tokens',0,'text'],'&amp; *x* A%2F E N [SHOULD_NOT_RENDER]'),
 'rename_relabels_mounted':(['stages',2,'geometry','tokens',0,'text'],O.RENAMED),
 'readback_old_label':(['stages',3,'geometry','tokens',0,'text'],O.ORIGINAL),
 'retry_wrong_label':(['stages',5,'geometry','tokens',0,'text'],O.RENAMED),
 'absent_to_null':(['stages',1,'proof','cache','source','title_blocks',0,'elements',0,'elements',2,'text'],None),
 'null_to_empty':(['stages',1,'proof','cache','effective','title_blocks',0,'elements',0,'elements',6,'text'],''),
 'truncated_changed':(['stages',1,'proof','cache','source','title_blocks',0,'elements',0,'elements',0,'truncated'],False),
 'opaque_changed':(['stages',1,'proof','cache','effective','title_blocks',0,'opaque_block'],{}),
 'raw_false_to_zero':(['stages',1,'proof','cache','source','title_blocks',0,'elements',0,'opaque_section','retain'],1),
 'released_response_foreign':(['stages',7,'proof','source','requests',3,'releasedResponse','file','id'],'FOREIGN'),
 'original_response_missing':(['stages',1,'proof','source','requests',0,'response'],None),
 'retry_failure_reused':(['stages',5,'proof','source','requests',2,'response'],dict(error='offline')),
 'extra_read':(['stages',5,'proof','source','networkCalls'],1),
 'wrong_factory':(['stages',0,'proof','source','requests',0,'method'],'users.info'),
 'late_adoption':(['stages',7,'proof','cache','current'],True),
 'late_delivered':(['stages',7,'proof','exercise','release','delivered'],True),
 'quip_missing':(['stages',1,'proof','cache','quipTitle'],None),
 'quip_foreign':(['stages',1,'proof','cache','quipTitle','fileId'],'FOREIGN'),
 'quip_retired':(['stages',1,'proof','cache','quipTitle','current'],False),
 'quip_override_invented':(['stages',1,'proof','cache','quipTitle','value'],'text'),
 'member_authority_borrowed':(['stages',1,'proof','cache','titleUsers'],dict(current=True)),
 'fallback_owner_retired':(['stages',1,'proof','cache','canvasFallback','current'],False),
 'foreign_owner':(['stages',1,'model','account'],'foreign'),
 'outer_wire_replaced':(['stages',1,'model','blocks',0,'elements',0,'elements',1,'text'],O.ORIGINAL),
 'outer_style_lost':(['stages',1,'geometry','captured','snapshot','blocks',0,'elements',0,'elements',1,'style'],{}),
 'hidden_token':(['stages',1,'geometry','tokens',0,'visible'],False),
 'original_vector_changed':(['linkTitle','original'],[]),
}
for name,(path,value) in CASES.items():
    def test(self,path=path,value=value):
        mutate(self.value,path,value)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):self.validate()
    setattr(LinkTitleTests,'test_reject_'+name,test)

if __name__=='__main__':unittest.main()
