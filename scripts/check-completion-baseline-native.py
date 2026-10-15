#!/usr/bin/env python3
"""Root-owned disconnected completion baseline. Never builds; no keyboard/clipboard.

Requires an independently passing pure-model reference receipt. Synthetic input
only; two RAF callbacks indicate frame opportunities, not physical pixel paint.
Promote to scripts before launching; staging can only run pure validator tests.
"""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import runpy
import secrets
import sys
import time

SCRIPTS=next(p/'scripts' for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file())
sys.path.insert(0,str(SCRIPTS))
from native_fixture import NativeFixture,DEFAULT_BINARY
from windows_cold_fixture import BinaryGuard,OwnedProcess
CONTRACT=runpy.run_path(str(Path(__file__).with_name('completion_receipts.py')))
UTIL=runpy.run_path(str(SCRIPTS/'check-slack-list-board-add-native.py'))
validate=CONTRACT['validate'];strict=CONTRACT['strict'];STAGES=CONTRACT['STAGES']
WAITING='completion-baseline-waiting: '
CAPTURE='completion-baseline-capture: '
TERMINAL='completion-baseline: '
FIXTURES=tuple(f'completion-directory-{size}{suffix}' for size in (2000,10000,64000) for suffix in ('','-narrow'))


def publish(path,value):
    staged=path.with_suffix('.staged')
    with staged.open('x',encoding='utf-8') as out:
        json.dump(value,out);out.flush();os.fsync(out.fileno())
    try: os.link(staged,path)
    finally: staged.unlink()


def run(args,fixture,reference):
    if Path(__file__).resolve().parent!=SCRIPTS.resolve():
        raise RuntimeError('Promote and review runner before native launch')
    directory=args.output.resolve()/fixture
    directory.mkdir(parents=True,exist_ok=False)
    nonce=secrets.token_hex(16)
    owned=directory/f'completion-baseline-{nonce}'
    owned.mkdir()
    total=int(fixture.split('-')[2])
    marker={'version':1,'fixture':fixture,'nonce':nonce,'rounds':args.rounds,'total':total}
    publish(owned/'owner.json',marker)
    evidence={'ok':False,'fixture':fixture,'nonce':nonce,'scope':'synthetic native-WebView input; two RAF callbacks, not physical paint','captures':{},'reference':str(args.reference.resolve()),'referenceSha256':args.reference_digest}
    binary=args.binary.resolve(strict=True)
    env={'SUPER_PLATINUM_COMPLETION_BASELINE':'1','SUPER_PLATINUM_COMPLETION_BASELINE_NONCE':nonce,
         'SUPER_PLATINUM_COMPLETION_BASELINE_ROUNDS':str(args.rounds),'SUPER_PLATINUM_COMPLETION_BASELINE_DIR':str(owned)}
    native=NativeFixture(fixture,directory/'native.log',binary,env)
    process=None
    try:
        with BinaryGuard(binary) as guard:
            evidence['binaryBefore']=guard.verify()
            with native:
                process=OwnedProcess(native.process.pid,binary)
                evidence['processBefore']=process.verify()
                evidence['pid']=native.process.pid
                evidence['processCreationFiletime']=process.created
                identity={**marker,'pid':native.process.pid}
                deadline=time.monotonic()+30
                while not (waiting:=native.receipts(WAITING)):
                    native.require_running()
                    if native.receipts(TERMINAL) or time.monotonic()>deadline: raise RuntimeError('Fixture did not reach readiness')
                    time.sleep(.05)
                if not strict(waiting,[identity]): raise RuntimeError('Wrong/duplicate readiness')
                with ExitStack() as stack:
                    pointer=UTIL['normalize'](native,evidence,stack,process.api,process.handle)
                    def capture_failure():
                        if evidence.get('nativeValidated'): return
                        try:
                            path=directory/'failure.png';native.capture(path)
                            evidence['failureScreenshot']=str(path)
                        except (OSError,RuntimeError,ValueError): pass
                    stack.callback(capture_failure)
                    publish(owned/'ready.json',identity)
                    deadline=time.monotonic()+210; captured=0;last_check=0
                    while True:
                        process.verify();native.require_running()
                        if time.monotonic()-last_check>=.2:
                            pointer.check('running',require_clear=True);last_check=time.monotonic()
                        stages=native.receipts(CAPTURE)
                        for stage in stages[captured:]:
                            if captured>=len(STAGES): raise RuntimeError('Extra capture stage')
                            expected={'nonce':nonce,'pid':native.process.pid,'stage':STAGES[captured]}
                            if not strict(stage,expected): raise RuntimeError('Wrong capture identity/order')
                            pointer.check(f'{STAGES[captured]}:before',require_clear=True)
                            path=directory/f'{STAGES[captured]}.png'
                            native.capture(path)
                            evidence['captures'][STAGES[captured]]={'path':str(path),'png':UTIL['png_signature'](path,evidence['expectedPngDimensions'])}
                            pointer.check(f'{STAGES[captured]}:after',require_clear=True)
                            publish(owned/f'capture-{STAGES[captured]}.json',expected)
                            captured+=1
                        terminals=native.receipts(TERMINAL)
                        if terminals:
                            if len(terminals)!=1: raise RuntimeError('Duplicate terminal receipt')
                            evidence['native']=terminals[0]
                            samples=validate(terminals[0],identity,reference)
                            if captured!=len(STAGES): raise RuntimeError('Missing capture')
                            evidence['durations']=CONTRACT['durations'](samples)
                            evidence['nativeValidated']=True
                            break
                        if time.monotonic()>deadline: raise TimeoutError('Native completion deadline exceeded')
                        time.sleep(.03)
            evidence['processAfter']=process.verify()
            if not evidence['processAfter']['exit_filetime']: raise RuntimeError('Owned process did not exit')
            UTIL['validate_pointer_cleanup'](evidence['pointer'])
            if not strict(UTIL['closed_receipts'](native.log,WAITING),[identity]): raise RuntimeError('Late readiness')
            if not strict(UTIL['closed_receipts'](native.log,TERMINAL),[evidence['native']]): raise RuntimeError('Late terminal')
            expected_stages=[{'nonce':nonce,'pid':native.process.pid,'stage':stage} for stage in STAGES]
            if not strict(UTIL['closed_receipts'](native.log,CAPTURE),expected_stages): raise RuntimeError('Late capture stage')
            validate(evidence['native'],identity,reference)
            evidence['binaryAfter']=guard.verify()
            if hashlib.sha256(args.reference.read_bytes()).hexdigest()!=args.reference_digest:
                raise RuntimeError('Independent reference changed during run')
            evidence['closedLogRevalidated']=True;evidence['ok']=True
    except BaseException as error:
        evidence['error']=str(error)
        raise
    finally:
        if process:
            try: evidence['cleanupProcess']=process.verify()
            except OSError as error: evidence['cleanupProcessError']=str(error)
            finally: process.close()
        evidence['cleanupErrors']=list(native.cleanup_errors)
        evidence['processExited']=native.process is not None and native.process.poll() is not None
        (directory/'receipt.json').write_text(json.dumps(evidence,indent=2)+'\n',encoding='utf-8')
        print(json.dumps({'ok':evidence['ok'],'receipt':str(directory/'receipt.json')}),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--reference',type=Path,required=True)
    parser.add_argument('--rounds',type=int,default=5)
    parser.add_argument('--fixture',action='append',choices=FIXTURES)
    args=parser.parse_args()
    if sys.platform!='win32' or not 1<=args.rounds<=12: parser.error('Windows and 1..12 rounds required')
    reference_bytes=args.reference.read_bytes()
    reference=json.loads(reference_bytes)
    args.reference_digest=hashlib.sha256(reference_bytes).hexdigest()
    for fixture in args.fixture or FIXTURES: run(args,fixture,reference)
