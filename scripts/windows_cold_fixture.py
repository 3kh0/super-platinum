"""Narrow ownership helpers for the explicit cold notification fixture runner.

No process-name termination, registry mutation, or arbitrary ledger paths.
Win32 contracts:
https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-createfilew
https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew
https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes
https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess
https://learn.microsoft.com/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject
"""
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import re
import uuid

from windows_process_metrics import WindowsAPI, FILETIME_UNIX_OFFSET


def normalized(path):
    value = str(path)
    if value.startswith('\\\\?\\'):
        value = value[4:]
    return os.path.normcase(os.path.normpath(value))


def known_local_app_data():
    class Guid(C.Structure):
        _fields_ = [('a', W.DWORD), ('b', W.WORD), ('c', W.WORD), ('d', C.c_ubyte * 8)]

    folder = Guid.from_buffer_copy(uuid.UUID('f1b32785-6fba-4fcf-9d55-7b8e7f157091').bytes_le)
    shell, ole = C.WinDLL('shell32'), C.WinDLL('ole32')
    shell.SHGetKnownFolderPath.argtypes = [C.POINTER(Guid), W.DWORD, W.HANDLE, C.POINTER(C.c_void_p)]
    shell.SHGetKnownFolderPath.restype = C.c_long
    ole.CoTaskMemFree.argtypes = [C.c_void_p]
    ole.CoTaskMemFree.restype = None
    pointer = C.c_void_p()
    result = shell.SHGetKnownFolderPath(C.byref(folder), 0, None, C.byref(pointer))
    if result < 0:
        raise OSError(f'SHGetKnownFolderPath(LocalAppData): 0x{result & 0xffffffff:08x}')
    try:
        return Path(C.wstring_at(pointer))
    finally:
        ole.CoTaskMemFree(pointer)


def handoff_directory(nonce):
    if not isinstance(nonce, str) or not re.fullmatch('[0-9a-f]{32}', nonce):
        raise ValueError('Invalid cold fixture nonce')
    path = known_local_app_data()
    for component in ('Super Platinum', 'notification-fixtures', nonce):
        path /= component
        if path.exists():
            reject_reparse(path)
    return path


def reject_reparse(path):
    if path.lstat().st_file_attributes & 0x400:
        raise RuntimeError(f'Cold fixture path is a reparse point; preserved: {path}')


def read_json(path, limit=256 * 1024):
    reject_reparse(path)
    with path.open('rb') as source:
        data = source.read(limit + 1)
    if len(data) > limit:
        raise RuntimeError(f'Cold receipt exceeds its size limit: {path.name}')
    value = json.loads(data)
    if not isinstance(value, dict):
        raise RuntimeError(f'Cold receipt must be an object: {path.name}')
    return value


def validate_process(value):
    if (not isinstance(value, dict) or set(value) != {'pid', 'created'} or
            any(type(value[key]) is not int or value[key] <= 0 for key in value)):
        raise RuntimeError('Invalid PID/creation-time ownership record')
    return value


def validate_claim(value, parent, parent_exit, earliest_created):
    process = validate_process(value)
    if (process['pid'] == parent['pid'] or process['created'] <= parent_exit or
            process['created'] < earliest_created):
        raise RuntimeError('Cold claim does not identify a new process after publisher exit and invocation')
    return process


class BinaryGuard:
    """Deny writes/rename/delete while parent, COM child and recovery use the exe."""
    def __init__(self, binary):
        self.path = binary.resolve(strict=True)
        self.kernel = C.WinDLL('kernel32', use_last_error=True)
        self.kernel.CreateFileW.argtypes = [W.LPCWSTR, W.DWORD, W.DWORD, C.c_void_p,
                                           W.DWORD, W.DWORD, W.HANDLE]
        self.kernel.CreateFileW.restype = W.HANDLE
        self.kernel.CloseHandle.argtypes = [W.HANDLE]
        self.kernel.CloseHandle.restype = W.BOOL
        # GENERIC_READ, FILE_SHARE_READ only, OPEN_EXISTING, normal attributes.
        self.handle = self.kernel.CreateFileW(str(self.path), 0x80000000, 1, None, 3, 0x80, None)
        if self.handle == C.c_void_p(-1).value:
            self.handle = None
            raise C.WinError(C.get_last_error())
        try:
            self.sha = self.digest()
        except BaseException:
            self.close()
            raise

    def digest(self):
        with self.path.open('rb') as source:
            return hashlib.file_digest(source, 'sha256').hexdigest()

    def verify(self, declared=None):
        if self.digest() != self.sha:
            raise RuntimeError('Cold executable changed; recovery is not authorized')
        if declared is not None and (not isinstance(declared, dict) or
                normalized(declared.get('path', '')) != normalized(self.path) or
                declared.get('sha256') != self.sha):
            raise RuntimeError('Cold receipt is bound to another executable')
        return {'path': str(self.path), 'sha256': self.sha}

    def close(self):
        if self.handle:
            self.kernel.CloseHandle(self.handle)
            self.handle = None

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()


class OwnedProcess:
    """Retained handle; termination requires exact creation time and executable."""
    def __init__(self, pid, binary, expected=None):
        self.api = WindowsAPI()
        kernel = self.api.kernel
        kernel.QueryFullProcessImageNameW.argtypes = [W.HANDLE, W.DWORD, W.LPWSTR, C.POINTER(W.DWORD)]
        kernel.QueryFullProcessImageNameW.restype = W.BOOL
        kernel.TerminateProcess.argtypes = [W.HANDLE, W.UINT]
        kernel.TerminateProcess.restype = W.BOOL
        self.pid, self.binary = pid, binary
        self._verified_live_path = None
        self.handle = kernel.OpenProcess(0x1000 | 0x100000 | 1, False, pid)
        if not self.handle:
            raise C.WinError(C.get_last_error())
        try:
            self.created = self.api.times(self.handle)['creation_filetime']
            if expected is not None and self.identity != validate_process(expected):
                raise RuntimeError('PID was reused or does not match the cold ownership claim')
            self.verify()
        except BaseException:
            self.close()
            raise

    @property
    def identity(self):
        return {'pid': self.pid, 'created': self.created}

    def verify(self):
        times = self.api.times(self.handle)
        if times['creation_filetime'] != self.created:
            raise RuntimeError('Owned process creation identity changed')
        status = self.api.kernel.WaitForSingleObject(self.handle, 0)
        if status == 0:
            # QueryFullProcessImageNameW can fail with ERROR_GEN_FAILURE after
            # exit. The retained handle still identifies the same process;
            # never reopen by PID or infer an executable from an exited PID.
            times = self.api.times(self.handle)
            if times['creation_filetime'] != self.created or not times['exit_filetime']:
                raise RuntimeError('Signaled owned process lacks matching creation and exit times')
            if self._verified_live_path is None:
                raise RuntimeError('Process exited before its executable was verified while alive')
            method = 'verified-live-path-retained-handle-after-exit'
        elif status == 258:
            if times['exit_filetime']:
                raise RuntimeError('Owned process wait state disagrees with its exit time')
            if self._verified_live_path is None:
                buffer, size = C.create_unicode_buffer(32768), W.DWORD(32768)
                if not self.api.kernel.QueryFullProcessImageNameW(self.handle, 0, buffer, C.byref(size)):
                    raise OSError('Initial live process executable query failed; ownership unverified') from C.WinError(C.get_last_error())
                if normalized(buffer.value) != normalized(self.binary):
                    raise RuntimeError('Owned PID executable path does not match the immutable binary')
                # A successful query alone is insufficient to cache ownership
                # if the process exited during initial verification.
                if self.api.kernel.WaitForSingleObject(self.handle, 0) != 258:
                    raise RuntimeError('Process did not remain alive through initial executable verification')
                self._verified_live_path = buffer.value
            method = 'verified-live-path-retained-handle'
        else:
            raise OSError(f'Owned process wait failed (status {status:#x})') from C.WinError(C.get_last_error())
        return {**self.identity, 'executable': self._verified_live_path,
                'executable_verification': method, 'process_signaled': status == 0,
                'exit_filetime': times['exit_filetime']}

    def exited(self):
        return bool(self.verify()['exit_filetime'])

    def terminate(self):
        if not self.exited():
            if not self.api.kernel.TerminateProcess(self.handle, 0):
                raise C.WinError(C.get_last_error())
            if self.api.kernel.WaitForSingleObject(self.handle, 5000) != 0:
                raise RuntimeError('Owned fixture process did not exit within 5 seconds')
        return self.verify()

    def close(self):
        if self.handle:
            self.api.kernel.CloseHandle(self.handle)
            self.handle = None


def validate_child(receipt, nonce, parent, action_ms, parent_exit, binary):
    process = validate_process(receipt.get('process'))
    binary.verify(receipt.get('executable'))
    if (receipt.get('nonce') != nonce or process == parent or process['pid'] == parent['pid'] or
            process['created'] <= parent_exit or
            process['created'] < action_ms * 10000 + FILETIME_UNIX_OFFSET or
            any(receipt.get(key) is not True for key in ('ok', 'noSession', 'noTransport')) or
            receipt.get('pid') != process['pid'] or receipt.get('activationCount') != 1 or
            receipt.get('handledCount') != 1 or type(receipt.get('hwnd')) is not int or
            receipt['hwnd'] <= 0 or receipt.get('foregroundHwnd') != receipt['hwnd']):
        raise RuntimeError('Cold child is not one new isolated foreground activation')
    callback, handled = receipt.get('callbackTimestampMs'), receipt.get('handledTimestampMs')
    foreground = receipt.get('foregroundTimestampMs')
    if (type(callback) is not int or type(handled) is not int or
            type(foreground) is not int or receipt.get('minimized') is not False or
            not action_ms <= callback <= handled <= foreground or foreground - action_ms > 30000):
        raise RuntimeError('Cold activation timestamps do not follow the one OS invocation')
    return process
