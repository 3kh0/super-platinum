#!/usr/bin/env python3
# Run after scripts/macos-app.sh; only disconnected fixtures and task-owned storage.
import os
import sqlite3
import json
import pathlib
import subprocess
import time
root=pathlib.Path(__file__).resolve().parents[1]
os.chdir(root)
(root/'tmp/agent-ui').mkdir(parents=True,exist_ok=True)
binary=pathlib.Path(os.environ.get('SUPER_PLATINUM_BINARY',str(root/'target/debug/super-platinum')))
marker=pathlib.Path(os.environ['TMPDIR'])/'super-platinum-agent.sock.path'
storage=root/'tmp'/f'file-recovery-{os.getpid()}.sqlite'
for fixture, label, mode in [('file-recovery','file-recovery-seed','seed'),('file-recovery-narrow','file-recovery-restart','restart'),('file-recovery','file-recovery-retire','retire')]:
    socket=pathlib.Path(os.environ['TMPDIR'])/f'recoverycheck-{os.getpid()}.sock'
    env=dict(os.environ,SUPER_PLATINUM_FIXTURE=fixture,SUPER_PLATINUM_FILE_RECOVERY_CHECK=mode,SUPER_PLATINUM_FILE_RECOVERY_DB=str(storage),SUPER_PLATINUM_AGENT='1',SUPER_PLATINUM_AGENT_SOCK=str(socket))
    log=root/'tmp/agent-ui'/f'{label}.log'
    previous=marker.read_text() if marker.exists() else None
    with log.open('w') as output:
        process=subprocess.Popen([str(binary)],env=env,stdout=output,stderr=output)
        try:
            ready=time.monotonic()+8
            while not socket.exists() and process.poll() is None and time.monotonic()<ready: time.sleep(.05)
            for attempt in range(50):
                activation=subprocess.run(['osascript','-l','JavaScript','-e',f'ObjC.import("AppKit"); var app=$.NSRunningApplication.runningApplicationWithProcessIdentifier({process.pid}); if(app.finishedLaunching) app.activateWithOptions(2); app.finishedLaunching && app.active'],capture_output=True,text=True,check=True)
                if activation.stdout.strip()=='true': break
                time.sleep(.1)

            deadline=time.monotonic()+35
            last_activation=0
            while True:
                if time.monotonic()-last_activation>1:
                    subprocess.run(['osascript','-l','JavaScript','-e',f'ObjC.import("AppKit"); $.NSRunningApplication.runningApplicationWithProcessIdentifier({process.pid}).activateWithOptions(2)'],stdout=subprocess.DEVNULL,check=True)
                    last_activation=time.monotonic()
                result=log.read_text()
                if 'file recovery check failed' in result:
                    raise RuntimeError(result)
                if 'file recovery check passed' in result:
                    break
                if process.poll() is not None or time.monotonic()>deadline:
                    raise RuntimeError(f'{fixture}: exited or timed out: {result}')
                time.sleep(.1)
            time.sleep(.25)
            subprocess.run(['scripts/agentctl.sh','screenshot',str(root/'tmp/agent-ui'/f'{label}.png')],env=env,check=True,stdout=subprocess.DEVNULL)
            print(f'{label}: checked and captured',flush=True)
        finally:
            if process.poll() is None:
                process.terminate()
                try:process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill();process.wait()
            if marker.exists() and pathlib.Path(marker.read_text().strip())==socket:
                marker.unlink()
                if previous:
                    restore_env=dict(os.environ,SUPER_PLATINUM_AGENT_SOCK=previous.strip())
                    live=subprocess.run(['scripts/agentctl.sh','ping'],env=restore_env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=3)
                    if live.returncode==0:marker.write_text(previous)
            socket.unlink(missing_ok=True)

with sqlite3.connect(storage) as connection:
    receipts=[json.loads(row[0]) for row in connection.execute("SELECT body FROM receipts")]
assert len(receipts)==2
assert sorted(row["status"] for row in receipts)==["Retired","Uncertain"]
assert any(row["status"]=="Uncertain" and row["operation"].get("Rename",{}).get("requested_title")=="Recovered name · 日本語" for row in receipts)
print("restart database verified: rename pending, deletion explicitly retired",flush=True)
