"""Modal-aware passive capture; no pointer movement or UI mutation."""
import runpy
import time
from pathlib import Path
from native_fixture import ROOT
from save_dialog_discovery import settle_samples
from save_workflow_contracts import require, digest


class Capture:
    def __init__(self, directory, expected, evidence, verify):
        from windows_capture import capture_rect
        from windows_client_occlusion import sample_client_ownership
        self.capture_rect = capture_rect
        self.sample_hit = sample_client_ownership
        self.visual = runpy.run_path(str(ROOT / 'scripts/check-visual-native.py'))
        self.directory, self.expected, self.evidence, self.verify = directory, expected, evidence, verify
        self.sample_dir = directory / 'capture-samples'
        self.sample_dir.mkdir()

    def visible_windows(self, stage):
        import ctypes as C
        from ctypes import wintypes as W
        self.verify()
        user = C.WinDLL('user32', use_last_error=True)
        callback_type = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
        user.EnumWindows.argtypes = [callback_type, W.LPARAM]
        user.EnumWindows.restype = W.BOOL
        user.IsWindowVisible.argtypes = [W.HWND]
        user.IsWindowVisible.restype = W.BOOL
        user.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
        user.GetWindowThreadProcessId.restype = W.DWORD
        handles = []
        def visit(hwnd, _):
            pid = W.DWORD()
            user.GetWindowThreadProcessId(hwnd, C.byref(pid))
            if pid.value == self.expected['pid'] and user.IsWindowVisible(hwnd):
                handles.append(int(hwnd))
            return True
        callback = callback_type(visit)
        require(user.EnumWindows(callback, 0), 'owned visible-window inventory failed')
        self.verify()
        require(len(handles) <= 32, 'bounded owned top-level windows')
        result = sorted(handles)
        self.evidence['windows'].append(dict(stage=stage, hwnds=result))
        return result

    def observe(self, hwnd=None, *, stage='check', settling=False):
        self.verify()
        require(len(self.evidence['observations']) < 8000, 'observation budget exhausted')
        observation = dict(stage=stage, expectedHwnd=hwnd, monotonic=time.monotonic(), ok=False)
        self.evidence['observations'].append(observation)
        try:
            window = self.visual['window_observation'](self.expected['pid'])
            observation['window'] = window
            hit = self.sample_hit(window)
            observation['hit'] = hit
            if hwnd is not None:
                require(window['hwnd'] == hwnd, 'foreground HWND changed')
            require(window['window_within_virtual_bounds'] is True, 'window outside desktop')
            require(hit['ok'], 'owned window covered at sampled points')
        except (RuntimeError, OSError) as error:
            observation['error'] = f'{type(error).__name__}: {error}'
            if settling:
                return None
            raise
        observation['ok'] = True
        return window

    def wait_other(self, hwnd, seconds=15):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            window = self.observe(stage='await-modal', settling=True)
            if window and window['hwnd'] != hwnd:
                return window
            time.sleep(.1)
        raise RuntimeError('owned modal did not appear')

    def capture(self, name, hwnd, *, final=True):
        def sample(index):
            row = dict(stage=name, index=index, eligible=False)
            self.evidence['captureSamples'].append(row)
            before = self.observe(hwnd, stage=f'{name}-{index}-before', settling=True)
            if before is None:
                return None
            rect = before['window_rect_physical']
            path = self.sample_dir / f'{name}-{index:02}.png'
            require(not path.exists(), 'fresh capture sample')
            self.capture_rect(path, *rect)
            row.update(path=str(path), sha256=digest(path), rect=rect, hwnd=hwnd)
            after = self.observe(hwnd, stage=f'{name}-{index}-after', settling=True)
            if after is None:
                return None
            if any(before[key] != after[key] for key in ('window_rect_physical', 'client_rect_physical')):
                row['error'] = 'capture geometry changed'
                return None
            row['eligible'] = True
            return dict(fingerprint=(hwnd, tuple(rect), tuple(after['client_rect_physical']), row['sha256']), record=row, window=after)
        result, settled = settle_samples(sample)
        source = result['record']
        path = self.directory / f'{name}.png'
        with path.open('xb') as output:
            output.write(Path(source['path']).read_bytes())
        record = dict(stage=name, path=str(path), hwnd=hwnd, rect=source['rect'], sha256=digest(path), sample=source['path'], settle=settled)
        self.evidence['captures' if final else 'reviewCaptures'].append(record)
        return record
