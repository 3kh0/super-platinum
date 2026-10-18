"""Pure validation of the closed file-title acceptance receipts."""
import copy
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest

R=runpy.run_path(str(Path(__file__).with_name('check-profile-file-titles-native.py')))


def sample(directory,narrow=False):
    fixture=R['FIXTURES'][int(narrow)]
    viewport=[668,380] if narrow else [1280,780]
    rect=dict(left=120,top=180,right=360,bottom=204,width=240,height=24)
    dialog=dict(left=100,top=12,right=540,bottom=368 if narrow else 700,width=440,height=356 if narrow else 688)
    attempt=[dict(element_id='long',long_text=dict(blocks=json.dumps(R['blocks']('Saved with ')))),
             dict(element_id='locked',text=dict(text='Preserved organization value'))]
    rows=[]
    for index,stage in enumerate(R['STAGES']):
        rich=stage in R['RICH']; label=R['renamed']() if stage=='confirmed' else R['label']()
        model=dict(enabled=True,account='00000000-0000-4000-8000-000000000081',epoch=0,visit=2,serial=3 if index<3 else 4,
                   blocks=R['blocks'](R['prefix'](stage)),busy=False,uncertain=False,canSave=stage=='limit5000',
                   managedValue='Preserved organization value',choiceValue='Morning',countryValue='US',
                   reads=int(index>=3),attempts=[copy.deepcopy(attempt)] if index>=3 else [])
        changed=stage in ('renamed-mounted','confirmed')
        raw=dict(id=R['ID'],title=R['raw_title'](changed),
                 mode='list' if R['MODE']['list'] else 'quip',fixture_opaque=dict(preserve=[R['ID'],1,2]))
        if R['MODE']['blocks']:raw['title_blocks']=R['title_blocks'](changed)
        effective=copy.deepcopy(raw);del effective['fixture_opaque']
        cache=None if index==0 else dict(source=raw,effective=effective,current=stage not in ('unavailable','held','cancelled'))
        if cache is not None and R['MODE']['blocks']:cache['quipTitle']=dict(current=True,value='nullish',fileId=R['ID'])
        exercise={'cold-loading':dict(cold=True),'opened':dict(release=dict(delivered=True,cancelledBeforeReply=False)),
                  'renamed-mounted':dict(frozenLabel=R['label']()),'confirmed':dict(reopenedLabel=R['renamed']()),
                  'unavailable':dict(draftRetained=True),'retried':dict(draftRetained=True),'held':dict(originalOpening=True),
                  'cancelled':dict(release=dict(delivered=False,cancelledBeforeReply=True)),
                  'limit5000':dict(sourceLength=5000,labelUnits=R['units'](R['label']()),occurrences=2),
                  'limit5001':dict(sourceLength=5001,labelUnits=R['units'](R['label']()),occurrences=2)}[stage]
        controls=['Back','Save section']
        if rich:controls+=['Use description','Cancel description edit']
        if stage in ('cold-loading','unavailable','held'):controls+=['Retry description','Cancel description edit']
        notice={'cold-loading':R['LOADING'],'held':R['LOADING'],'unavailable':R['UNAVAILABLE'],
                'confirmed':'Profile fields saved.','limit5001':"Check this field's value or length."}.get(stage)
        captured=dict(snapshot=dict(blocks=R['blocks'](R['prefix'](stage)),generation='22',revision=1,composing=False),
                      tokens=[dict(value=t,label=label) for t in R['blocks']('')[0]['elements'][0]['elements'][1:]]) if rich else None
        geometry=dict(ok=True,noOverflow=True,viewport=viewport,dialog=True,dialogRect=copy.deepcopy(dialog),
                      controls=[dict(text=c,disabled=c=='Save section' or (c=='Retry description' and stage in ('cold-loading','held')),
                                     visible=True,rect=copy.deepcopy(rect)) for c in controls],
                      notices=[dict(text=notice,visible=True,rect=copy.deepcopy(rect))] if notice else [],
                      tokens=[dict(text=label,editable=False,fontWeight=weight,fontStyle=style,visible=True,rect=copy.deepcopy(rect))
                              for weight,style in [('700','normal'),('400','italic')]] if rich else [],
                      richEditable=True if rich else None,captured=captured,pngDimensions=[1002,600] if narrow else [1920,1200],
                      longDisabled=False,managedFieldPreserved=True,choiceValue='Morning',countryValue='US')
        rows.append(dict(stage=stage,model=model,geometry=geometry,
                         proof=dict(source=R['expected_source'](index),cache=cache,exercise=exercise)))
    receipt=dict(ok=True,fixture=fixture,mentionKind=R['kind'](),titleBlocks=R['MODE']['blocks'],viewport=viewport,captureChecksPassed=True,
                 visualReview='Original PNG inspection required separately',stages=rows,
                 captures={s:str(directory.resolve()/f'{fixture}-{s}.png') for s in R['STAGES']})
    return fixture,receipt


class FileTitleReceiptTests(unittest.TestCase):
    def setUp(self):
        R['MODE'].update(list=False,blocks=False)
        self.directory=Path(tempfile.gettempdir())/'file-title-validator-only'
        self.fixture,self.receipt=sample(self.directory)
    def validate(self):R['validate'](self.fixture,self.receipt,self.directory,platform='win32',png=False)
    def reject(self,mutate):
        mutate(self.receipt)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):self.validate()
    def test_canvas_and_list_both_widths(self):
        for mode,blocks in ((False,False),(True,False),(False,True)):
            R['MODE'].update(list=mode,blocks=blocks)
            for narrow in (False,True):
                f,r=sample(self.directory,narrow)
                R['validate'](f,r,self.directory,platform='win32',png=False)
    def block_sample(self):
        R['MODE'].update(list=False,blocks=True)
        self.fixture,self.receipt=sample(self.directory)
    def test_block_mode_needs_explicit_receipt(self):
        self.block_sample()
        self.reject(lambda r:r.pop('titleBlocks'))
    def test_blocks_never_use_misleading_plain_title(self):
        self.block_sample()
        self.reject(lambda r:r['stages'][1]['geometry']['tokens'][0].update(text=R['raw_title']()))
    def test_blocks_do_not_unescape_literal_entity(self):
        self.block_sample()
        self.reject(lambda r:r['stages'][1]['geometry']['tokens'][0].update(text='& *literal*'))
    def test_blocks_do_not_parse_literal_mrkdwn(self):
        self.block_sample()
        self.reject(lambda r:r['stages'][1]['geometry']['tokens'][0].update(text='&amp; literal'))
    def test_leading_colon_emoji_is_removed_once(self):
        self.block_sample()
        self.reject(lambda r:r['stages'][1]['geometry']['tokens'][0].update(text=':wave: &amp; *literal*'))
    def test_block_runs_and_styles_retained_in_source(self):
        self.block_sample()
        self.reject(lambda r:r['stages'][1]['proof']['cache']['source']['title_blocks'][0]['elements'][0]['elements'][1].pop('style'))
    def test_block_opaque_fields_retained_in_effective(self):
        self.block_sample()
        self.reject(lambda r:r['stages'][1]['proof']['cache']['effective']['title_blocks'][0].pop('opaque_block'))
    def test_blocks_cannot_be_cleared_on_rename(self):
        self.block_sample()
        self.reject(lambda r:r['stages'][2]['proof']['cache']['effective'].update(title_blocks=[]))
    def test_runtime_absence_is_required(self):
        self.block_sample()
        self.reject(lambda r:r['stages'][1]['proof']['cache'].pop('quipTitle'))
    def test_runtime_override_not_treated_as_absence(self):
        self.block_sample()
        self.reject(lambda r:r['stages'][1]['proof']['cache']['quipTitle'].update(value='text'))
    def test_retired_runtime_cannot_authorize_block_title(self):
        self.block_sample()
        self.reject(lambda r:r['stages'][5]['proof']['cache']['quipTitle'].update(current=False))
    def test_runtime_observation_binds_original_file(self):
        self.block_sample()
        self.reject(lambda r:r['stages'][1]['proof']['cache']['quipTitle'].update(fileId='FFOREIGN'))
    def test_cli_blocks_and_list_are_exclusive(self):
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('check-profile-file-titles-native.py')),
                               '--captures',str(self.directory),'--list','--blocks'],capture_output=True,text=True)
        self.assertEqual(result.returncode,2)
        self.assertIn('not allowed with argument',result.stderr)
    def test_real_request_contract(self):
        self.reject(lambda r:r['stages'][1]['proof']['source']['requests'][0]['args'].update(count='25'))
    def test_no_live_fallback(self):
        self.reject(lambda r:r['stages'][4]['proof']['source'].update(networkCalls=1))
    def test_cold_cache_cannot_be_preseeded(self):
        self.reject(lambda r:r['stages'][0]['proof'].update(cache=r['stages'][1]['proof']['cache']))
    def test_renamed_mounted_label_must_stay_frozen(self):
        self.reject(lambda r:r['stages'][2]['geometry']['tokens'][0].update(text=R['renamed']()))
    def test_reopen_observes_new_title(self):
        self.reject(lambda r:r['stages'][3]['geometry']['tokens'][0].update(text=R['label']()))
    def test_label_cannot_replace_wire_url(self):
        self.reject(lambda r:r['stages'][3]['model']['blocks'][0]['elements'][0]['elements'][1].update(text=R['label']()))
    def test_opaque_and_style_custody(self):
        self.reject(lambda r:r['stages'][1]['geometry']['captured']['tokens'][1]['value'].update(style=dict(bold=True)))
    def test_computed_bold_required_even_when_raw_style_survives(self):
        self.reject(lambda r:r['stages'][1]['geometry']['tokens'][0].update(fontWeight='400'))
    def test_computed_italic_required_even_when_raw_style_survives(self):
        self.reject(lambda r:r['stages'][1]['geometry']['tokens'][1].update(fontStyle='normal'))
    def test_style_must_not_bleed_between_duplicates(self):
        self.reject(lambda r:r['stages'][2]['geometry']['tokens'][1].update(fontWeight='700'))
    def test_bold_token_must_not_inherit_italic(self):
        self.reject(lambda r:r['stages'][5]['geometry']['tokens'][0].update(fontStyle='italic'))
    def test_reopened_tokens_need_actual_computed_styles(self):
        self.reject(lambda r:r['stages'][3]['geometry']['tokens'][0].pop('fontWeight'))
    def test_limit_tokens_need_actual_computed_styles(self):
        self.reject(lambda r:r['stages'][8]['geometry']['tokens'][1].pop('fontStyle'))
    def test_cancelled_reply_not_delivered(self):
        self.reject(lambda r:r['stages'][7]['proof']['exercise']['release'].update(delivered=True))
    def test_cancelled_cache_not_current(self):
        self.reject(lambda r:r['stages'][7]['proof']['cache'].update(current=True))
    def test_late_read_not_reported_completed_without_drop(self):
        self.reject(lambda r:r['stages'][7]['proof']['source'].update(cancelled=0))
    def test_exact_label_units_not_id_units(self):
        self.reject(lambda r:r['stages'][8]['proof']['exercise'].update(labelUnits=len(R['ID'])))
    def test_5001_not_saveable(self):
        self.reject(lambda r:r['stages'][9]['model'].update(canSave=True))
    def test_retry_not_extra_write(self):
        self.reject(lambda r:r['stages'][5]['model']['attempts'].append(copy.deepcopy(r['stages'][5]['model']['attempts'][0])))
    def test_source_raw_not_full_projection(self):
        self.reject(lambda r:r['stages'][1]['proof']['cache']['effective'].update(fixture_opaque={'preserve':[R['ID'],1,2]}))
    def test_hidden_feedback_rejected(self):
        self.reject(lambda r:r['stages'][4]['geometry']['notices'][0].update(visible=False))
    def test_narrow_clipping_rejected(self):
        self.fixture,self.receipt=sample(self.directory,True)
        self.reject(lambda r:r['stages'][1]['geometry']['tokens'][0]['rect'].update(right=900,width=780))
    def test_same_editor_serial_only_replaced_by_save(self):
        self.reject(lambda r:r['stages'][5]['model'].update(serial=100))
    def test_png_path_custody(self):
        self.reject(lambda r:r['captures'].update(opened=str(self.directory/'foreign.png')))
    def test_stage_order(self):
        self.reject(lambda r:r['stages'].reverse())


if __name__=='__main__':unittest.main()
