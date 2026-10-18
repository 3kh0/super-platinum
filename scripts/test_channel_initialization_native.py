"""Pure channel initialization native receipt validation; no process launch."""
import copy
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest

R = runpy.run_path(str(Path(__file__).with_name('check-channel-initialization-native.py')))


def geometry(narrow=False):
    viewport = [668, 380] if narrow else [1280, 780]
    box = dict(left=50, top=260, right=220, bottom=285, width=170, height=25)
    heading = dict(left=50, top=230, right=220, bottom=255, width=170, height=25)
    rows = [dict(name='warm-retained', section='Channels')]
    return dict(ok=True, noOverflow=True, viewport=viewport, pngDimensions=[1280,800],
                expectedRows=rows, actualRows=copy.deepcopy(rows),
                focus=dict(**rows[0],visible=True,labelsFit=True,rect=box),
                heading=dict(text='Channels',visible=True,rect=heading))


def sample(captures, narrow=False):
    fixture = 'channel-initialization' + ('-narrow' if narrow else '')
    requests = []
    for boot in (True, False, False):
        for branch in ('init', 'channels', 'extras'):
            reason = {'init': 'fetch-client-init-data', 'channels': 'deferred-data', 'extras': 'fetch-client-extras-data'}[branch] if boot else 'resynced-topics'
            fields = dict(token='synthetic-channel-token', _x_reason=reason)
            if branch == 'init': fields['include_relevant_onboarding'] = 'true' if boot else 'false'
            elif branch == 'channels': fields.update(version_all_channels='false', return_all_relevant_mpdms='true')
            else: fields['extras'] = json.dumps(R['EXTRAS'], separators=(',', ':'))
            requests.append(dict(method='POST', endpoint=f'https://example.test/api/client.{branch}',
                                 form=[list(pair) for pair in fields.items()]))
    rows = []
    for index, stage in enumerate(R['STAGES']):
        name = R['NAMES'][index]
        section = 'External connections' if index in (2, 3, 4) else 'Channels'
        layout = geometry(narrow)
        layout['expectedRows'] = [dict(name=name, section=section)]
        layout['actualRows'] = copy.deepcopy(layout['expectedRows'])
        layout['focus'].update(name=name, section=section)
        layout['heading']['text'] = section
        kinds = [] if index in (0, 5) else ['Init'] if index == 1 else ['Init', 'Extras']
        model = dict(enabled=True, channels=[dict(id='CINIT0001', name=name, section=section)], focusChannel=name,
                     account='00000000-0000-4000-8000-000000000082' if index == 5 else '00000000-0000-4000-8000-000000000081',
                     epoch=21 if index == 5 else 20, socketGeneration=0 if index in (0, 5) else 1,
                     queuePending=index in (0, 5), initialPending=index in (1, 4), networkCalls=0,
                     settlements=[dict(kind=kind, ok=True, failedKind=None) for kind in kinds],
                     preparedRequests=copy.deepcopy(requests[:(0, 3, 3, 6, 9, 9)[index]]), lastProof=copy.deepcopy(R['PROOFS'][stage]))
        rows.append(dict(stage=stage, model=model, geometry=layout, proof=copy.deepcopy(R['PROOFS'][stage])))
    return dict(ok=True, fixture=fixture, viewport=geometry(narrow)['viewport'], stages=rows,
                captures={stage: str(captures / f'{fixture}-{stage}.png') for stage in R['STAGES']},
                final=copy.deepcopy(rows[-1]['model']), captureChecksPassed=True,
                visualReview='PNG inspection required separately')


class ValidatorTests(unittest.TestCase):
    def check(self, value, captures):
        R['validate'](value['fixture'], value, captures, platform='win32', png=False)

    def test_all_six_transitions_normal_and_narrow(self):
        with tempfile.TemporaryDirectory() as directory:
            captures = Path(directory).resolve()
            for narrow in (False, True): self.check(sample(captures, narrow), captures)

    def test_stale_socket_and_foreign_publication_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            captures = Path(directory).resolve()
            for index, key in ((4, 'staleSocketRejected'), (5, 'foreignPublicationRejected'), (5, 'oldOwnerRetired')):
                value = sample(captures)
                value['stages'][index]['proof'][key] = False
                with self.assertRaisesRegex(RuntimeError, 'transition proof'): self.check(value, captures)
            value = sample(captures)
            value['stages'][4]['proof']['sourceName'] = 'delayed-harbor'
            with self.assertRaisesRegex(RuntimeError, 'transition proof'): self.check(value, captures)

    def test_warm_queue_and_failed_refresh_retain_state(self):
        with tempfile.TemporaryDirectory() as directory:
            captures = Path(directory).resolve()
            for index, field, wrong in ((0, 'queuePending', False), (3, 'initialPending', True),
                                        (3, 'channels', []), (3, 'settlements', []), (5, 'queuePending', False)):
                value = sample(captures)
                value['stages'][index]['model'][field] = wrong
                with self.assertRaises(RuntimeError): self.check(value, captures)

    def test_account_epoch_and_native_state_must_actually_change(self):
        with tempfile.TemporaryDirectory() as directory:
            captures = Path(directory).resolve()
            for key in ('account', 'epoch', 'focusChannel', 'channels'):
                value = sample(captures)
                value['stages'][5]['model'][key] = value['stages'][4]['model'][key]
                with self.assertRaises(RuntimeError): self.check(value, captures)

    def test_request_policy_replay_and_network_use_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            captures = Path(directory).resolve()
            value = sample(captures)
            value['stages'][3]['model']['preparedRequests'].append(value['stages'][3]['model']['preparedRequests'][0])
            with self.assertRaisesRegex(RuntimeError, 'replay'): self.check(value, captures)
            value = sample(captures)
            value['stages'][1]['model']['preparedRequests'][0]['form'][-1][1] = 'false'
            with self.assertRaisesRegex(RuntimeError, 'policy'): self.check(value, captures)
            value = sample(captures)
            value['stages'][0]['model']['networkCalls'] = 1
            with self.assertRaisesRegex(RuntimeError, 'Network'): self.check(value, captures)

    def test_missing_capture_wrong_owner_and_boolean_proof_spoof_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            captures = Path(directory).resolve()
            value = sample(captures)
            del value['captures']['rejected']
            with self.assertRaisesRegex(RuntimeError, 'inventory'): self.check(value, captures)
            value = sample(captures)
            value['captures']['warm'] = str(captures / 'foreign.png')
            with self.assertRaisesRegex(RuntimeError, 'owned capture'): self.check(value, captures)
            value = sample(captures)
            with self.assertRaisesRegex(RuntimeError, 'Missing native capture'):
                R['validate'](value['fixture'], value, captures, platform='win32')
            value['stages'][1]['proof']['initAdopted'] = 1
            with self.assertRaisesRegex(RuntimeError, 'proof types'): self.check(value, captures)

    def test_closed_log_rejects_duplicate_terminal_and_incomplete_tail(self):
        with tempfile.TemporaryDirectory() as directory:
            captures = Path(directory).resolve()
            value = sample(captures)
            expected = R['ready_tuple'](value['fixture'], 'a' * 32, 12)
            value['readiness'] = expected
            path = captures / 'fixture.log'
            class Log:
                log = path
                def receipts(self, prefix):
                    return [json.loads(line.split(prefix, 1)[1]) for line in self.log.read_text().splitlines(keepends=True)
                            if line.endswith('\n') and prefix in line]
            lines = [R['WAITING'] + json.dumps(expected)]
            lines.extend(R['EDGE'] + json.dumps(dict(stage=stage, edge=edge))
                         for stage in R['STAGES'] for edge in ('before', 'after'))
            lines.append(R['PREFIX'] + json.dumps(value))
            text = '\n'.join(lines) + '\n'
            path.write_text(text)
            R['validate_closed'](Log(), expected, captures, value, platform='win32', png=False)
            path.write_text(text + lines[-1] + '\n')
            with self.assertRaisesRegex(RuntimeError, 'duplicate terminal'):
                R['validate_closed'](Log(), expected, captures, value, platform='win32', png=False)
            path.write_text(text + R['EDGE'] + '{')
            with self.assertRaisesRegex(RuntimeError, 'Incomplete final'):
                R['validate_closed'](Log(), expected, captures, value, platform='win32', png=False)

    def test_capture_stage_order_cannot_be_replayed(self):
        with tempfile.TemporaryDirectory() as directory:
            captures = Path(directory).resolve()
            value = sample(captures)
            class Log:
                def receipts(self, prefix):
                    if prefix == R['PREFIX']: return [value]
                    return [dict(stage=stage, edge=edge) for stage in reversed(R['STAGES']) for edge in ('before', 'after')]
            with self.assertRaisesRegex(RuntimeError, 'capture order'):
                R['validate_log'](Log(), value['fixture'], captures, 'win32', False)

    def test_png_header_must_match_actual_capture_dimensions(self):
        import struct
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'capture.png'
            path.write_bytes(b'\x89PNG\r\n\x1a\n' + struct.pack('>I', 13) + b'IHDR' + struct.pack('>II', 1280, 800))
            R['validate_png'](path, [1280, 800])
            with self.assertRaisesRegex(RuntimeError, 'dimensions'): R['validate_png'](path, [668, 400])

    def test_actual_checker_script_accepts_rounded_rows_and_rejects_occlusion(self):
        # Execute the embedded checker, with the chrome.rs svg+span heading and a
        # rounded hit region. The former 1px diagonal corner probes miss this row.
        source = (Path(__file__).resolve().parents[1] / 'src/desktop/app_home/channel_fetches/fixture/check.rs').read_text()
        script = source.split('r#"(()=>{{', 1)[1].split('"#', 1)[0]
        script = '(()=>{{' + script
        script = script.replace('expected={}', 'expected=' + json.dumps([dict(name='warm-harbor', section='Channels')]))
        script = script.replace('focusName={}', 'focusName="warm-harbor"').replace('{{', '{').replace('}}', '}')
        setup = r'''
const mode=process.argv[1];
globalThis.innerWidth=1280;globalThis.innerHeight=780;
const label={textContent:'warm-harbor',scrollWidth:95,clientWidth:170};
const title={textContent:'Channels'};
const rect=(left,top,width,height)=>({left,top,right:left+width,bottom:top+height,width,height});
const heading={tagName:'BUTTON',getAttribute:()=> 'section-label section-toggle',
  querySelector:s=>s==='span'?title:null,matches:s=>s==='.section-toggle',
  getBoundingClientRect:()=>rect(50,230,170,25),scrollIntoView(){},contains:a=>a===title};
const button={tagName:'BUTTON',getAttribute:()=> 'channel active',previousElementSibling:heading,
  querySelector:s=>s==='.channel-name'?label:null,querySelectorAll:()=>[label],
  getBoundingClientRect:()=>rect(50,mode==='clipped'?775:260,170,25),scrollIntoView(){},contains:a=>a===label};
const parent={tagName:'NAV',getAttribute:()=> 'channel-list'};
const overlay={tagName:'DIV',getAttribute:()=> 'overlay'};
globalThis.getComputedStyle=e=>({visibility:'visible',display:'flex'});
globalThis.document={documentElement:{scrollWidth:1280},querySelector:()=>({querySelectorAll:()=>[button]}),
  elementFromPoint(x,y){
    for(const e of [button,heading]){
      const r=e.getBoundingClientRect();
      if(x<r.left||x>r.right||y<r.top||y>r.bottom)continue;
      if(e===button){
        if(mode==='covered')return overlay;
        const cx=Math.max(r.left+5,Math.min(x,r.right-5)),cy=Math.max(r.top+5,Math.min(y,r.bottom-5));
        if((x-cx)**2+(y-cy)**2>25)return parent;
      }
      return e;
    }
    return parent;
  }};
if(document.elementFromPoint(51,261)!==parent&&mode==='rounded')throw Error('Mock must reject old corner probe');
'''
        for mode, success in [('rounded', True), ('covered', False), ('clipped', False)]:
            result = subprocess.run(['node', '-e', setup + '\nconsole.log(JSON.stringify(' + script + '));', mode],
                                    capture_output=True, text=True, check=True)
            measured = json.loads(result.stdout)
            self.assertIs(measured['ok'], success)
            self.assertEqual(measured['heading']['text'], 'Channels')
            self.assertEqual(len(measured['focus']['hitTests']), 5)
            if mode == 'covered': self.assertEqual(measured['focus']['hitTests'][0]['class'], 'overlay')

    def test_normal_and_narrow_visible_projection(self):
        for narrow in (False,True):
            value=geometry(narrow)
            R['validate_layout'](value,value['viewport'])

    def test_sidebar_name_and_classification_must_match_actual_dom(self):
        for key in ('name','section'):
            value=geometry()
            value['actualRows'][0][key]='other'
            with self.assertRaisesRegex(RuntimeError,'projection'): R['validate_layout'](value,value['viewport'])
        value=geometry()
        value['heading']['text']='Direct messages'
        with self.assertRaisesRegex(RuntimeError,'classification'): R['validate_layout'](value,value['viewport'])

    def test_hidden_clipped_and_ellipsized_channel_rejected(self):
        for key in ('visible','labelsFit'):
            value=geometry(True)
            value['focus'][key]=False
            with self.assertRaises(RuntimeError): R['validate_layout'](value,value['viewport'])
        value=geometry(True)
        value['focus']['rect'].update(top=375,bottom=400)
        with self.assertRaisesRegex(RuntimeError,'clipped'): R['validate_layout'](value,value['viewport'])

    def test_wrong_viewport_nonfinite_or_boolean_dimensions_rejected(self):
        value=geometry(True)
        with self.assertRaises(RuntimeError): R['validate_layout'](value,[1280,780])
        value['focus']['rect']['top']=float('nan')
        with self.assertRaises(RuntimeError): R['validate_layout'](value,value['viewport'])
        value=geometry()
        value['pngDimensions'][0]=True
        with self.assertRaises(RuntimeError): R['validate_layout'](value,value['viewport'])

    def test_exact_nonce_fixture_pid_ownership(self):
        expected=R['ready_tuple']('channel-initialization','a'*32,12)
        R['validate_ready'](expected.copy(),expected)
        for key,wrong in (('fixture','channel-initialization-narrow'),('pid',13),('pid',True),('version',True),('nonce','b'*32),('extra',1)):
            with self.assertRaises(RuntimeError): R['validate_ready']({**expected,key:wrong},expected)
        for fixture,nonce,pid in [('other','a'*32,12),('channel-initialization','A'*32,12),('channel-initialization','a'*32,False)]:
            with self.assertRaises(RuntimeError): R['ready_tuple'](fixture,nonce,pid)

    def test_stale_ready_file_is_never_overwritten(self):
        expected=R['ready_tuple']('channel-initialization','a'*32,12)
        with tempfile.TemporaryDirectory() as directory:
            directory=Path(directory)
            R['H']['publish_ready'](directory,expected)
            with self.assertRaises(FileExistsError): R['H']['publish_ready'](directory,{**expected,'pid':13})
            self.assertEqual(json.loads((directory/'ready.json').read_text()),expected)

    def test_native_dimensions_cannot_change_after_placement(self):
        expected=R['ready_tuple']('channel-initialization','a'*32,12)
        value=dict(readiness=expected,stages=[dict(geometry=geometry())])
        R['validate_owned'](value,expected,[1280,800])
        with self.assertRaisesRegex(RuntimeError,'dimensions'): R['validate_owned'](value,expected,[1200,800])

    def test_process_exit_and_pointer_cleanup_are_required(self):
        pointer=dict(movement_observed=False,restore_observed=True,restore_attempted=True)
        R['H']['validate_pointer_cleanup'](pointer)
        for key,wrong in (('movement_observed',True),('restore_observed',False),('restore_attempted',False),('restore_error','failed')):
            with self.assertRaises(RuntimeError): R['H']['validate_pointer_cleanup']({**pointer,key:wrong})
        with self.assertRaises(RuntimeError): R['H']['validate_exit'](123,dict(creation_filetime=124,exit_filetime=125))


if __name__=='__main__':
    unittest.main()
