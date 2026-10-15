#!/usr/bin/env python3
"""Separate focused offline startup diagnostic. No completion input or idle claim.

The existing runtime probe adds twenty seconds of observation work. Do not merge
its CPU or startup timings with probe-off performance samples. This launcher
requests foreground after first agent ping; it is not a background-launch test.
"""
import argparse
from contextlib import ExitStack
import json
from pathlib import Path
import runpy
import sys
import time

SCRIPTS=next(p/'scripts' for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file())
sys.path.insert(0,str(SCRIPTS))
from native_fixture import NativeFixture,DEFAULT_BINARY
from windows_cold_fixture import BinaryGuard,OwnedProcess
UTIL=runpy.run_path(str(SCRIPTS/'check-slack-list-board-add-native.py'))
PREFIX='super-platinum runtime-probe: '


def run(args):
    if Path(__file__).resolve().parent!=SCRIPTS.resolve(): raise RuntimeError('Promote before native launch')
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    env={'SUPER_PLATINUM_RUNTIME_PROBE':'1','SUPER_PLATINUM_STARTUP_TRACE':'1'}
    evidence={'ok':False,'fixture':args.fixture,'environment':env,'agentObservations':[],
              'scope':'focused disconnected startup diagnostic; no input, idle CPU, live startup or stall-fix claim'}
    binary=args.binary.resolve(strict=True);native=NativeFixture(args.fixture,out/'startup.log',binary,env);process=None
    try:
        with BinaryGuard(binary) as guard:
            evidence['binaryBefore']=guard.verify();launch_started=time.monotonic()
            with native:
                evidence['firstPingAndFocusObservedMs']=(time.monotonic()-launch_started)*1000
                process=OwnedProcess(native.process.pid,binary)
                evidence['processBefore']=process.verify();evidence['processCreationFiletime']=process.created
                with ExitStack() as stack:
                    pointer=UTIL['normalize'](native,evidence,stack,process.api,process.handle)
                    started=time.monotonic();next_observation=0
                    while True:
                        process.verify();pointer.check('diagnostic',require_clear=True)
                        elapsed=time.monotonic()-started
                        if elapsed>=next_observation:
                            observation={'runnerElapsedMs':(time.monotonic()-launch_started)*1000}
                            try:
                                snapshot=native.control('state',timeout=2)['data']
                                observation.update(startup=snapshot.get('performance',{}).get('startup',{}),screen=snapshot.get('screen'),activeChannel=snapshot.get('active_channel'))
                            except (TimeoutError,OSError,RuntimeError) as error: observation['agentError']=str(error)
                            evidence['agentObservations'].append(observation);next_observation=elapsed+2
                        records=native.receipts(PREFIX)
                        if any(r.get('phase')=='finished' for r in records):
                            evidence['probe']=records;break
                        if elapsed>30: raise TimeoutError('No terminal native watchdog receipt')
                        time.sleep(.2)
                    path=out/'startup-final.png';native.capture(path)
                    evidence['screenshot']={'path':str(path),'png':UTIL['png_signature'](path,evidence['expectedPngDimensions'])}
                    pointer.check('captured',require_clear=True)
            evidence['processAfter']=process.verify()
            if not evidence['processAfter']['exit_filetime']: raise RuntimeError('Owned startup process did not exit')
            UTIL['validate_pointer_cleanup'](evidence['pointer'])
            records=UTIL['closed_receipts'](native.log,PREFIX)
            if sum(r.get('phase')=='configured' for r in records)!=1 or sum(r.get('phase')=='finished' for r in records)!=1:
                raise RuntimeError('Missing/duplicate closed diagnostic boundaries')
            if any(r.get('delay_ms')!=0 for r in records): raise RuntimeError('Unexpected delayed probe')
            evidence['probe']=records;evidence['binaryAfter']=guard.verify();evidence['closedLogRevalidated']=True;evidence['ok']=True
    except BaseException as error:
        evidence['error']=str(error);raise
    finally:
        if process:
            try:evidence['cleanupProcess']=process.verify()
            except (OSError,RuntimeError) as error:evidence['cleanupProcessError']=str(error)
            finally:process.close()
        evidence['processExited']=native.process is not None and native.process.poll() is not None
        (out/'receipt.json').write_text(json.dumps(evidence,indent=2)+'\n',encoding='utf-8')
        print(json.dumps({'ok':evidence['ok'],'receipt':str(out/'receipt.json')}),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--fixture',choices=['completion-directory-2000','completion-directory-10000','completion-directory-64000'],default='completion-directory-64000')
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if sys.platform!='win32':parser.error('Windows native diagnostic required')
    run(args)
