#!/usr/bin/env python3
"""One fixed six-process observer series; never builds or launches a live session."""
import argparse
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import runpy
import secrets
import sys
import time
from emoji_retirement_receipts import ROLES, aggregate, decode, samples, require, integer
from emoji_retirement_windows import Capabilities
from emoji_retirement_overhead import validate as validate_overhead

ROOT=next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'scripts/native_fixture.py').is_file())
SCRIPTS=ROOT/'scripts';sys.path.insert(0,str(SCRIPTS))
from native_fixture import NativeFixture
from windows_process_metrics import WindowsAPI
from canvas_evidence import pointer_cleanup
U=runpy.run_path(str(SCRIPTS/'check-slack-list-board-add-native.py'))
normalize,validate_pointer_cleanup=U['normalize'],U['validate_pointer_cleanup']
VISUAL=runpy.run_path(str(SCRIPTS/'check-visual-native.py'))
WAITING='emoji-retirement-waiting: '
CLOSE='emoji-retirement-close-ready: '
FAILED='emoji-retirement-terminal: '

@contextmanager
def cleanup_boundary(evidence,cap):
    try:yield
    finally:
        evidence.setdefault('cleanupStartedNs',time.monotonic_ns())
        if cap.event:
            try:cap.api.signal(cap.event)
            except BaseException as error:cap.errors.append({'stage':'pre-native-cleanup-signal','error':str(error)})

def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()

def sealed(binary,receipt):
    require(Path(__file__).resolve().parent==SCRIPTS,'Staged runner cannot launch')
    value=json.loads(receipt.read_text(encoding='utf-8'))
    require(set(value)=={'schema','binary_sha256','manifest_path','manifest_sha256','profile','features','overrides'},'Sealed build shape')
    require(type(value['schema']) is int and value['schema']==1 and value['profile']=='dev' and value['features']==['emoji-retirement-observation'] and value['overrides']=={},'Wrong build profile/features')
    require(value['binary_sha256']==digest(binary),'Binary differs from sealed build')
    manifest_path=ROOT/'tmp/emoji-retirement-observation-prep-v2/manifest.json'
    require(Path(value['manifest_path']).resolve()==manifest_path and value['manifest_sha256']==digest(manifest_path),'Wrong observer manifest')
    manifest=json.loads(manifest_path.read_text(encoding='utf-8'))
    for row in manifest['targets']:
        require(digest(ROOT/row['target'])==row['sha256'],'Promoted target changed: '+row['target'])
    for row in manifest['dependencies']:
        require(digest(Path(row['path']) if Path(row['path']).is_absolute() else ROOT/row['path'])==row['sha256'],'Dependency changed: '+row['path'])
    return value

def overhead_gate(path,manifest):
    gate=json.loads(path.read_text(encoding='utf-8'))
    require(type(gate['schema']) is int and gate['schema']==1 and gate['kind']=='sealed-empty-wrapper-overhead' and gate['ok'] is True,'Overhead gate did not pass')
    require(gate['exited'] is True and gate['cleanupErrors']==[] and type(gate['exitCode']) is int and gate['exitCode']==0,'Overhead process did not cleanly finish')
    require(gate['test']=='performance::emoji_retirement::overhead::finite_actual_empty_wrapper_observer_overhead','Wrong overhead test')
    require(gate['build']['manifest_sha256']==manifest,'Overhead tested another observer source')
    require(sealed(Path(gate['binary']),Path(gate['build_receipt']))==gate['build'],'Overhead binary/provenance changed')
    receipt_path=Path(gate['receipt']);directory=receipt_path.parent
    require(set(gate['raw'])=={'receipt.json','disabled.bin','enabled.bin'},'Overhead raw inventory')
    for name,sha in gate['raw'].items():require(digest(directory/name)==sha,'Overhead artifact changed')
    receipt=json.loads(receipt_path.read_text(encoding='utf-8'))
    require(integer(gate['pid']) and gate['pid']==receipt['pid'],'Overhead PID mismatch')
    result=validate_overhead(receipt,(directory/'disabled.bin').read_bytes(),(directory/'enabled.bin').read_bytes(),manifest)
    require(result==gate['analysis'],'Overhead validation changed')
    return {'path':str(path),'sha256':digest(path),'analysis':result}

def one(role,binary,folder,provenance):
    start=time.monotonic();folder.mkdir(parents=True,exist_ok=False)
    nonce=secrets.token_hex(16);private=folder/f'emoji-retirement-{nonce}';private.mkdir()
    env={'SUPER_PLATINUM_EMOJI_RETIREMENT':'1','SUPER_PLATINUM_EMOJI_RETIREMENT_NONCE':nonce,
         'SUPER_PLATINUM_EMOJI_RETIREMENT_PROCESS':role,'SUPER_PLATINUM_EMOJI_RETIREMENT_DIR':str(private)}
    native=NativeFixture('emoji-retirement-64000',folder/'native.log',binary,env)
    evidence={'ok':False,'process':role,'nonce':nonce,'build':provenance,'cleanupErrors':[]}
    metrics=handle=cap=None;raw=None
    try:
        cap=Capabilities(role) # Before Popen, inside the retained failure boundary.
        with native, cleanup_boundary(evidence,cap):
            pid=native.process.pid;evidence['pid']=pid
            metrics=WindowsAPI();handle=metrics.open(pid)
            creation=metrics.times(handle)['creation_filetime'];evidence['processCreationFiletime']=creation
            with pointer_cleanup(evidence) as stack, cleanup_boundary(evidence,cap):
                deadline=start+25
                while not (waiting:=native.receipts(WAITING)):
                    native.require_running()
                    require(time.monotonic()<deadline,'Startup deadline');time.sleep(.025)
                expected=dict(version=2,nonce=nonce,pid=pid,creation=creation,fixture='emoji-retirement-64000',process=role)
                require(waiting==[expected],'Readiness challenge identity/inventory')
                require(all(integer(waiting[0][k]) for k in ('version','pid','creation')),'Readiness identity types')
                pointer=normalize(native,evidence,stack,metrics,handle)
                require(time.monotonic()<deadline,'Startup normalization deadline')
                ready=cap.attach(pid,creation,nonce);evidence['readiness']=ready
                native.capture(folder/'initial.png');evidence['initialScreenshot']=str(folder/'initial.png')
                evidence['pngDimensions']=U['png_signature'](folder/'initial.png',evidence['expectedPngDimensions'])
                require(time.monotonic()<deadline,'Startup capture deadline')
                cap.publish(private,ready)
                sampled=0.
                while not (terminal:=native.receipts(CLOSE)):
                    native.require_running()
                    require(not native.receipts(FAILED),'Native protocol failed')
                    require(time.monotonic()-start<600,'Process schedule deadline')
                    if pointer and time.monotonic()-sampled>=.2:
                        pointer.check('running',require_clear=True);sampled=time.monotonic()
                    time.sleep(.025)
                require(len(terminal)==1,'Repeated close-ready')
                terminal=terminal[0];require(terminal['ready']==ready,'Close readiness identity changed')
                evidence['terminal']=terminal
                if pointer:pointer.check('pre-close',require_clear=True)
                observed=VISUAL['window_observation'](pid)
                require(observed['foreground_owned'],'Owned window no longer foreground')
                require(observed['hwnd']==evidence['placement']['after']['hwnd'],'Owned main window identity changed')
                evidence['preCloseWindow']=observed
                # Everything above may block. The next method posts and then
                # signals B without any dispatcher, capture, log or disk read.
                close=cap.close_request(observed['hwnd'],native.process.poll)
                evidence['close']=close
                until=close['request_ns']+5_000_000_000
                while native.process.poll() is None and time.monotonic_ns()<until:time.sleep(.005)
                require(native.process.poll() is not None,'Close process timeout (cleanup kill is not evidence)')
                evidence['exitCode']=native.process.returncode
                require(native.process.returncode==0,'Unsuccessful native exit')
                evidence['exit_ns']=time.monotonic_ns()
                require(evidence['exit_ns']<=until,'Exit outside fixed close deadline')
                raw=cap.snapshot_after_exit()
                export_start=time.monotonic()
                (folder/'trace.bin').write_bytes(raw)
                trace=decode(raw,ready)
                schedule=json.loads(Path(__file__).with_name('emoji_retirement_schedule.json').read_text(encoding='utf-8'))
                evidence['analysis']=samples(schedule,terminal,trace)
                evidence['traceSha256']=hashlib.sha256(raw).hexdigest()
                evidence['traceEvents']=len(trace['events'])
                require(time.monotonic()-export_start<=10,'Post-exit export deadline')
        validate_pointer_cleanup(evidence.get('pointer',{}))
        require(native.receipts(WAITING)==[expected] and native.receipts(CLOSE)==[terminal] and native.receipts(FAILED)==[],'Closed log differs from live receipts')
        evidence['closedLogRevalidated']=True
        evidence['ok']=evidence['analysis']['shutdown']['complete']
        if not evidence['ok']:evidence['error']='Shutdown incomplete: missing actual terminal child destruction'
    except BaseException as error:
        evidence.update(ok=False,error=f'{type(error).__name__}: {error}')
        # No screenshot/agent request is attempted during close or B signalling.
        # Partial scalar mapping is retained only after owned process termination.
        raise
    finally:
        cleanup_start=evidence.get('cleanupStartedNs',time.monotonic_ns())
        errors=[]
        try:
            try:evidence['processExited']=native.process is not None and native.process.poll() is not None
            except BaseException as e:errors.append({'stage':'process-poll','error':str(e)})
            try:
                evidence['failureReceipts']=native.receipts(FAILED) if native.log.is_file() else []
                if raw is None and cap is not None and cap.view and evidence.get('processExited'):
                    raw=cap.snapshot_after_exit();(folder/'partial-trace.bin').write_bytes(raw)
            except BaseException as e:errors.append({'stage':'partial-export','error':str(e)})
        finally:
            if cap is not None:cap.close()
            if handle:
                try:
                    evidence['exitIdentity']=metrics.times(handle)
                except BaseException as e:errors.append({'stage':'exit-identity','error':str(e)})
                finally:
                    try:require(bool(metrics.kernel.CloseHandle(handle)),'Metrics handle close failed')
                    except BaseException as e:errors.append({'stage':'metrics-close','error':str(e)})
        evidence['controlAudit']=cap.audit if cap is not None else []
        evidence['cleanupErrors']=native.cleanup_errors+(cap.errors if cap is not None else [])+errors
        if evidence.get('pointerCleanupError'):evidence['cleanupErrors'].append({'stage':'pointer','error':evidence['pointerCleanupError']})
        if time.monotonic_ns()-cleanup_start>5_000_000_000:evidence['cleanupErrors'].append({'stage':'cleanup-deadline','error':'Exceeded 5s'})
        if evidence['cleanupErrors']:evidence['ok']=False
        try:
            evidence['binarySha256After']=digest(binary)
            if evidence['binarySha256After']!=provenance['binary_sha256']:
                evidence['ok']=False;evidence.setdefault('error','Binary changed')
        except BaseException as error:
            evidence['ok']=False;evidence.setdefault('error','Final binary hash read failed')
            evidence['binaryHashError']=f'{type(error).__name__}: {error}'
        (folder/'receipt.json').write_text(json.dumps(evidence,indent=2)+'\n',encoding='utf-8')
    return evidence

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--build-receipt',type=Path,required=True)
    parser.add_argument('--overhead-gate',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();require(sys.platform=='win32','Windows only')
    binary=args.binary.resolve();provenance=sealed(binary,args.build_receipt.resolve())
    overhead=overhead_gate(args.overhead_gate.resolve(),provenance['manifest_sha256'])
    args.output.mkdir(parents=True,exist_ok=False)
    receipts=[];report={'ok':False,'overhead':overhead}
    try:
        for role in ROLES:
            require(sealed(binary,args.build_receipt.resolve())==provenance,'Pre-process provenance changed')
            require(overhead_gate(args.overhead_gate.resolve(),provenance['manifest_sha256'])==overhead,'Overhead admission changed')
            receipts.append(one(role,binary,args.output/role,provenance))
            require(receipts[-1]['ok'],'Stop on failed/censored shutdown; no replacement process')
        require(sealed(binary,args.build_receipt.resolve())==provenance,'Final provenance changed')
        report.update(aggregate(receipts),ok=True)
    except BaseException as error:
        report['error']=f'{type(error).__name__}: {error}';raise
    finally:
        report['processes']=[{'role':r['process'],'ok':r['ok']} for r in receipts]
        try:
            report['terminalProvenanceCurrent']=sealed(binary,args.build_receipt.resolve())==provenance
            if not report['terminalProvenanceCurrent']:report['ok']=False
        except BaseException as error:
            report['ok']=False;report['terminalProvenanceError']=f'{type(error).__name__}: {error}'
        (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    require(report['ok'],'Final series admission failed')

if __name__=='__main__':main()
