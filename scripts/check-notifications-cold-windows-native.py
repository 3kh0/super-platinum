#!/usr/bin/env python3
"""Explicit isolated Windows cold toast activation and durable cleanup proof.

Creates one fixture toast, terminates its verified publisher, invokes that exact
nonce-marked toast once, and checks the OS-launched child's identity/foreground.
No session, settings, unrelated notification, or preexisting fixture registration
is changed. Keep the executable immutable until this run/recovery finishes.
The known-folder nonce directory is retained as evidence, not recursively deleted.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture
from windows_cold_fixture import (BinaryGuard, OwnedProcess, handoff_directory,
                                  normalized, read_json, reject_reparse,
                                  validate_child, validate_claim, validate_process)
from windows_process_metrics import FILETIME_UNIX_OFFSET

_spec = importlib.util.spec_from_file_location('warm_notification_runner',
    ROOT / 'scripts/check-notifications-windows-native.py')
warm = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(warm)
APP_ID = 'com.echonet.superplatinum.fixture.notifications'
CLSID = '{910CE832-EE9F-4C71-832F-0F4BDB37A2B8}'


def wait_json(path, seconds, process=None):
    deadline = time.monotonic() + seconds
    while True:
        if path.is_file():
            return read_json(path)
        if process is not None:
            process.require_running()
        if time.monotonic() >= deadline:
            raise RuntimeError(f'Timed out after {seconds} seconds waiting for {path.name}')
        time.sleep(.05)


def fixture_absent(snapshot):
    fixture_keys = [value for key, value in snapshot['registry'].items()
                    if APP_ID in key or CLSID.lower() in key.lower()]
    shortcuts = [value for key, value in snapshot['shortcuts'].items()
                 if Path(key).name == 'Super Platinum Notification Fixture.lnk']
    return len(fixture_keys) == 2 and len(shortcuts) == 1 and all(
        value is None for value in fixture_keys + shortcuts)


def validate_parent(receipt, ledger, lease, nonce, directory, parent, binary):
    binary.verify(receipt.get('executable'))
    binary.verify(ledger.get('executable'))
    binary.verify(lease.get('executable'))
    identity = parent.identity
    expected = {'ok': True, 'nonce': nonce, 'parent': identity, 'app_id': APP_ID,
                'appName': 'Super Platinum Fixture', 'title': 'Super Platinum',
                'body': f'Super Platinum cold activation check {nonce}',
                'noSession': True, 'noTransport': True, 'exitAuthorized': False}
    if (any(receipt.get(key) != value for key, value in expected.items()) or
            normalized(receipt.get('directory', '')) != normalized(directory)):
        raise RuntimeError('Cold publisher ready receipt has the wrong identity or fixture contract')
    for value in (ledger, lease):
        if (value.get('version') != 1 or value.get('nonce') != nonce or
                value.get('owner') != identity or value.get('child') is not None):
            raise RuntimeError('Cold ledger/lease ownership does not match the retained publisher')
    if (lease.get('phase') != 'Acquiring' or lease.get('artifacts') != [] or
            ledger.get('phase') != 'Ready' or ledger.get('preview_removed') is not False or
            ledger.get('preview') != {'tag': nonce[:16], 'group': nonce[16:]}):
        raise RuntimeError('Cold ownership journal is not ready for one OS launch')
    submission = receipt.get('submitted', {})
    if (submission.get('app_id') != APP_ID or
            any(submission.get(key) is not True for key in
                ('silent', 'preview', 'submitted', 'history_present')) or
            submission.get('failed_hresult') is not None or
            submission.get('tag') != nonce[:16] or submission.get('group') != nonce[16:]):
        raise RuntimeError('Missing exact silent cold preview submission/history receipt')


def verify_registered(ledger, binary, nonce):
    """Compare only the four fixed artifacts with their recorded intended bytes."""
    import winreg

    keys = {'Clsid': rf'Software\Classes\CLSID\{CLSID}',
            'LocalServer': rf'Software\Classes\CLSID\{CLSID}\LocalServer32',
            'AppId': rf'Software\Classes\AppUserModelId\{APP_ID}'}
    artifacts = ledger.get('artifacts', [])
    if len(artifacts) != 4 or {item.get('kind') for item in artifacts} != {*keys, 'Shortcut'}:
        raise RuntimeError('Cold ledger must contain exactly the four declared artifacts')
    for artifact in artifacts:
        if (artifact.get('complete') is not True or artifact.get('created') is not True or
                artifact.get('removed') is not False):
            raise RuntimeError('Cold artifact creation was not durably confirmed')
        kind, expected = artifact['kind'], artifact.get('expected')
        if kind == 'Shortcut':
            path = warm.programs_directory() / 'Super Platinum Notification Fixture.lnk'
            reject_reparse(path)
            if expected != {'File': list(path.read_bytes())}:
                raise RuntimeError('Cold fixture shortcut differs from recorded bytes')
            continue
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, keys[kind]) as handle:
            child_count, value_count, _ = winreg.QueryInfoKey(handle)
            children = [winreg.EnumKey(handle, index).lower() for index in range(child_count)]
            if children != (['localserver32'] if kind == 'Clsid' else []):
                raise RuntimeError('Cold fixture registry has unexpected subkeys')
            values, decoded = [], {}
            for index in range(value_count):
                name, value, value_type = winreg.EnumValue(handle, index)
                if value_type != winreg.REG_SZ or not isinstance(value, str):
                    raise RuntimeError('Cold registry value has an unexpected type')
                values.append({'name': name, 'kind': value_type,
                               'bytes': list((value + '\0').encode('utf-16-le'))})
                decoded[name] = value
            if expected != {'Registry': sorted(values, key=lambda item: item['name'])}:
                raise RuntimeError('Cold registry differs from recorded intended values')
            intended = ({'': f'"{ledger["executable"]["path"]}" --super-platinum-toast-cold-activated={nonce}'}
                        if kind == 'LocalServer' else
                        {'DisplayName': 'Super Platinum Fixture', 'CustomActivator': CLSID}
                        if kind == 'AppId' else {})
            if {k: v.lower() if k == 'CustomActivator' else v for k, v in decoded.items()} != {
                    k: v.lower() if k == 'CustomActivator' else v for k, v in intended.items()}:
                raise RuntimeError('Cold registration contains a foreign command or app identity')
    binary.verify(ledger['executable'])


def validate_cleanup(receipt, nonce, expected_process=None):
    if (any(receipt.get(key) is not True for key in ('ok', 'previewRemoved', 'registrationRestored', 'leaseReleased'))
            or receipt.get('nonce') != nonce or type(receipt.get('recovery')) is not bool or
            type(receipt.get('ownedArtifacts')) is not int or not 0 <= receipt['ownedArtifacts'] <= 4):
        raise RuntimeError('Cold cleanup receipt is incomplete')
    process = validate_process(receipt.get('process'))
    if expected_process is not None and process != expected_process:
        raise RuntimeError('Cold cleanup was not performed by the expected owned process')


def wait_cleanup(directory, nonce, process, seconds=20):
    deadline = time.monotonic() + seconds
    while True:
        path = directory / 'cleanup.json'
        if path.is_file():
            receipt = read_json(path)
            if receipt.get('leaseReleased') is True:
                validate_cleanup(receipt, nonce, process)
                return receipt
        if time.monotonic() >= deadline:
            raise RuntimeError('Cold cleanup completion is unknown; live child and ledger preserved for review')
        time.sleep(.05)


def recover(binary, nonce, directory, captures, evidence):
    """Run only the immutable executable's early-main, exact-ledger recovery."""
    binary.verify()
    output = captures / 'notification-cold-recovery.log'
    env = {key: value for key, value in os.environ.items() if not key.startswith('SUPER_PLATINUM_')}
    with output.open('w', encoding='utf-8') as stream:
        process = subprocess.Popen([str(binary.path), f'--super-platinum-toast-cold-recover={nonce}'],
            cwd=ROOT, env=env, stdout=stream, stderr=stream, creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            code = process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            # This Popen handle belongs to the one exact recovery command.
            process.kill()
            process.wait(timeout=5)
            raise RuntimeError('Cold recovery timed out; completion is unknown and is not replayed')
    binary.verify()
    events = [json.loads(line.split('notification-cold-recovery: ', 1)[1])
              for line in output.read_text(encoding='utf-8').splitlines()
              if line.startswith('notification-cold-recovery: ')]
    evidence['recovery_log'] = str(output)
    evidence['recovery'] = events
    if code != 0 or len(events) != 1 or events[0].get('ok') is not True:
        raise RuntimeError(f'Exact cold recovery failed; preserve ledger and inspect {output}')
    result = read_json(directory / 'cleanup.json')
    validate_cleanup(result, nonce)
    if result.get('recovery') is not True:
        raise RuntimeError('Expected explicit recovery cleanup receipt')
    return result


def run(binary_path, captures):
    from windows_capture import capture_rect, primary_rect

    captures.mkdir(parents=True, exist_ok=True)
    nonce = uuid.uuid4().hex
    directory = handoff_directory(nonce)
    owner_path = directory.parent / 'owner'
    before = warm.registration_snapshot()
    if not fixture_absent(before) or owner_path.exists() or directory.exists():
        raise RuntimeError('Cold fixture registration or ownership already exists; preserved')
    evidence = {'ok': False, 'nonce': nonce, 'directory': str(directory), 'screenshots': [],
                'actual_os_activation': False, 'visible_banner_verified': False,
                'registration_before': before}
    native = NativeFixture('channel', captures / 'notification-cold-parent.log', binary_path,
        {'SUPER_PLATINUM_NOTIFICATION_CHECK': '1', 'SUPER_PLATINUM_NOTIFICATION_COLD_CHECK': '1',
         'SUPER_PLATINUM_NOTIFICATION_COLD_NONCE': nonce})
    parent = child = None
    invoked = False
    earliest_child = None
    cleanup_error = None
    with BinaryGuard(binary_path) as binary:
        evidence['executable'] = binary.verify()
        try:
            native.__enter__()
            parent = OwnedProcess(native.process.pid, binary.path)
            evidence['parent_process'] = parent.verify()
            ready = wait_json(directory / 'parent.json', 30, native)
            ledger, lease = read_json(directory / 'ledger.json'), read_json(owner_path)
            validate_parent(ready, ledger, lease, nonce, directory, parent, binary)
            verify_registered(ledger, binary, nonce)
            evidence['parent'] = ready
            evidence['registration_ready_verified'] = True
            parent_exit = parent.terminate()
            native.process.wait(timeout=5)
            evidence['parent_exit'] = parent_exit
            if not parent_exit['exit_filetime']:
                raise RuntimeError('Publisher did not exit before OS invocation')
            binary.verify()
            left, top, width, height = primary_rect()
            path = captures / 'notification-cold-before-invoke.png'
            capture_rect(path, left + width - min(width, 1100), top + height - min(height, 1200),
                         min(width, 1100), min(height, 1200))
            evidence['screenshots'].append({'path': str(path)})
            earliest_child = time.time_ns() // 100 + FILETIME_UNIX_OFFSET
            invoked = True  # Any helper exception after this point is treated as ambiguous.
            action_ms = warm.invoke_owned_toast({**ready, 'pid': parent.pid, 'mode': 'cold'}, captures, evidence)
            evidence['action_timestamp_ms'] = action_ms
            claim = wait_json(directory / 'child-claim.json', 30)
            validate_claim(claim, parent.identity, parent_exit['exit_filetime'], earliest_child)
            child = OwnedProcess(claim['pid'], binary.path, claim)
            evidence['child_process'] = child.verify()
            receipt = wait_json(directory / 'child.json', 30)
            foreground_receipt_observed = time.monotonic()
            validate_child(receipt, nonce, parent.identity, action_ms, parent_exit['exit_filetime'], binary)
            if receipt['process'] != child.identity:
                raise RuntimeError('Cold child receipt differs from the immutable child claim')
            evidence['child'] = receipt
            foreground = warm.foreground_snapshot(receipt['hwnd'])
            evidence['foreground'] = foreground
            if (foreground['pid'] != child.pid or foreground['minimized'] or
                    foreground['foregroundHwnd'] != receipt['hwnd']):
                raise RuntimeError('Cold child is not the actual foreground window')
            path = captures / 'notification-cold-activated-screen.png'
            capture_rect(path, *primary_rect())
            evidence['activation_screenshot'] = str(path)
            evidence['activation_screenshots'] = [{
                'path': str(path), 'stage': 'immediate-startup',
                'seconds_after_foreground_receipt': time.monotonic() - foreground_receipt_observed,
                'foreground': foreground,
            }]
            # Foreground restoration precedes WebView paint. Keep the owned
            # child alive for bounded visual evidence; do not refocus it or
            # infer rendered layout from an activation/cleanup receipt.
            for delay in (1, 3):
                time.sleep(max(0, foreground_receipt_observed + delay - time.monotonic()))
                process = child.verify()
                before_capture = warm.foreground_snapshot(receipt['hwnd'])
                if (process['exit_filetime'] or before_capture['pid'] != child.pid or
                        before_capture['minimized'] or
                        before_capture['foregroundHwnd'] != receipt['hwnd']):
                    raise RuntimeError(f'Cold child lost its verified foreground window before {delay}-second capture')
                path = captures / f'notification-cold-activated-screen-{delay}s.png'
                capture_rect(path, *primary_rect())
                after_capture = warm.foreground_snapshot(receipt['hwnd'])
                evidence['activation_screenshots'].append({
                    'path': str(path), 'stage': f'{delay}-seconds',
                    'seconds_after_foreground_receipt': time.monotonic() - foreground_receipt_observed,
                    'process': process, 'foreground_before': before_capture,
                    'foreground_after': after_capture,
                })
                if (after_capture['pid'] != child.pid or after_capture['minimized'] or
                        after_capture['foregroundHwnd'] != receipt['hwnd']):
                    raise RuntimeError(f'Cold child lost its verified foreground window during {delay}-second capture')
            cleanup = wait_cleanup(directory, nonce, child.identity)
            if cleanup['recovery'] or cleanup['ownedArtifacts'] != 4:
                raise RuntimeError('Cold child did not retire its complete owned registration')
            evidence['cleanup'] = cleanup
            evidence['child_exit'] = child.terminate()
            evidence['actual_os_activation'] = True
        except BaseException as error:
            evidence['error'] = str(error)
        finally:
            try:
                # Popen owns the publisher even if startup failed before its
                # ready receipt. Never resolve a process to kill by name.
                if parent is None and native.process is not None:
                    parent = OwnedProcess(native.process.pid, binary.path)
                if parent is not None:
                    evidence['parent_exit'] = parent.terminate()
                if child is None and invoked:
                    try:
                        claim = wait_json(directory / 'child-claim.json', 35)
                        validate_claim(claim, parent.identity, evidence['parent_exit']['exit_filetime'], earliest_child)
                        child = OwnedProcess(claim['pid'], binary.path, claim)
                    except RuntimeError:
                        # There may still be a pending OS launch. Do not replay
                        # Invoke or race unconfirmed COM startup with deletion.
                        raise RuntimeError('Cold launch completion is ambiguous; exact ledger preserved for reviewed recovery')
                if child is not None:
                    if not child.exited():
                        # The native worker may still hold its operation lock;
                        # timeout is not completion or permission to interrupt it.
                        evidence['cleanup'] = wait_cleanup(directory, nonce, child.identity)
                    evidence['child_exit'] = child.terminate()
                if owner_path.exists():
                    lease = read_json(owner_path)
                    binary.verify(lease.get('executable'))
                    if (lease.get('nonce') != nonce or parent is None or
                            lease.get('owner') != parent.identity or not parent.exited() or
                            (child is not None and not child.exited())):
                        raise RuntimeError('Cold recovery ownership or process exit is unconfirmed')
                    evidence['cleanup'] = recover(binary, nonce, directory, captures, evidence)
                binary.verify()
            except BaseException as error:
                cleanup_error = str(error)
                evidence['cleanup_error'] = cleanup_error
            finally:
                try:
                    after = warm.registration_snapshot()
                    evidence['registration_after'] = after
                    evidence['registration_restored'] = after == before and fixture_absent(after)
                    evidence['ownership_lease_absent'] = not owner_path.exists()
                    if not evidence['registration_restored'] or not evidence['ownership_lease_absent']:
                        raise RuntimeError('Cold registration or ownership lease was not restored')
                except BaseException as error:
                    evidence['registration_verification_error'] = str(error)
                    cleanup_error = cleanup_error or str(error)
                if parent is not None:
                    parent.close()
                if child is not None:
                    child.close()
                native.close()
                evidence['executable_after'] = binary.verify()
                evidence['ok'] = (evidence['actual_os_activation'] and not evidence.get('error')
                                  and not cleanup_error and evidence.get('registration_restored') is True)
                receipt_path = captures / 'notification-cold-windows.json'
                receipt_path.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
                print(json.dumps({'receipt': str(receipt_path), 'ok': evidence['ok'],
                                  'nonce': nonce, 'directory': str(directory)}), flush=True)
    if not evidence['ok']:
        raise RuntimeError(evidence.get('error') or cleanup_error or 'Cold activation verification failed')
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=DEFAULT_BINARY)
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/agent-ui/windows-notifications-cold')
    args = parser.parse_args()
    if sys.platform != 'win32':
        raise SystemExit('This check requires native Windows')
    run(args.binary.resolve(), args.captures.resolve())


if __name__ == '__main__':
    main()
