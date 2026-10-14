"""Read-only Win32 process-tree counters, scoped to an explicitly supplied PID.

Identity is PID plus GetProcessTimes creation FILETIME, with retained handles.
Only descendants whose parent identity/lifetime validates are opened or reported.
References:
https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/ns-tlhelp32-processentry32w
https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes
https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-getprocessmemoryinfo
https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex
"""
import ctypes as C
from ctypes import wintypes as W
import sys
import time

TICKS_PER_SECOND = 10_000_000
FILETIME_UNIX_OFFSET = 116444736000000000


class ProcessEntry(C.Structure):
    _fields_ = [
        ("dwSize", W.DWORD), ("cntUsage", W.DWORD), ("pid", W.DWORD),
        ("heap", C.c_size_t), ("module", W.DWORD), ("threads", W.DWORD),
        ("parent_pid", W.DWORD), ("priority", W.LONG), ("flags", W.DWORD),
        ("name", W.WCHAR * 260),
    ]


class MemoryCounters(C.Structure):
    _fields_ = [("cb", W.DWORD), ("page_faults", W.DWORD)] + [
        (name, C.c_size_t) for name in (
            "peak_working_set", "working_set", "peak_paged_pool", "paged_pool",
            "peak_nonpaged_pool", "nonpaged_pool", "pagefile_usage",
            "peak_pagefile_usage", "private_usage",
        )
    ]


def filetime_now():
    return time.time_ns() // 100 + FILETIME_UNIX_OFFSET


def ticks(value):
    return (value.dwHighDateTime << 32) | value.dwLowDateTime


def parent_lifetime_contains(parent, child_creation):
    return (child_creation >= parent["creation_filetime"]
            and (not parent["exit_filetime"] or child_creation <= parent["exit_filetime"]))


class WindowsAPI:
    def __init__(self):
        if sys.platform != "win32":
            raise RuntimeError("process-tree counters require native Windows")
        self.kernel = C.WinDLL("kernel32", use_last_error=True)
        self.psapi = C.WinDLL("psapi", use_last_error=True)
        signatures = [
            ("CreateToolhelp32Snapshot", [W.DWORD, W.DWORD], W.HANDLE),
            ("Process32FirstW", [W.HANDLE, C.POINTER(ProcessEntry)], W.BOOL),
            ("Process32NextW", [W.HANDLE, C.POINTER(ProcessEntry)], W.BOOL),
            ("OpenProcess", [W.DWORD, W.BOOL, W.DWORD], W.HANDLE),
            ("CloseHandle", [W.HANDLE], W.BOOL),
            ("WaitForSingleObject", [W.HANDLE, W.DWORD], W.DWORD),
            ("GetProcessTimes", [W.HANDLE] + [C.POINTER(W.FILETIME)] * 4, W.BOOL),
        ]
        for name, arguments, result in signatures:
            function = getattr(self.kernel, name)
            function.argtypes, function.restype = arguments, result
        self.psapi.GetProcessMemoryInfo.argtypes = [W.HANDLE, C.POINTER(MemoryCounters), W.DWORD]
        self.psapi.GetProcessMemoryInfo.restype = W.BOOL

    def processes(self):
        handle = self.kernel.CreateToolhelp32Snapshot(0x2, 0)  # TH32CS_SNAPPROCESS
        if handle == C.c_void_p(-1).value:
            raise C.WinError(C.get_last_error())
        try:
            entry = ProcessEntry()
            entry.dwSize = C.sizeof(entry)
            result = []
            available = self.kernel.Process32FirstW(handle, C.byref(entry))
            while available:
                result.append({"pid": int(entry.pid), "parent_pid": int(entry.parent_pid),
                               "name": entry.name})
                available = self.kernel.Process32NextW(handle, C.byref(entry))
            if C.get_last_error() != 18:  # ERROR_NO_MORE_FILES
                raise C.WinError(C.get_last_error())
            return result
        finally:
            self.kernel.CloseHandle(handle)

    def open(self, pid):
        # Modern GetProcessTimes/GetProcessMemoryInfo both accept QUERY_LIMITED.
        # No VM_READ, injection, debug privilege, or process mutation is needed.
        handle = self.kernel.OpenProcess(0x1000 | 0x100000, False, pid)  # QUERY_LIMITED | SYNCHRONIZE
        if not handle:
            raise C.WinError(C.get_last_error())
        return handle

    def times(self, handle):
        status = self.kernel.WaitForSingleObject(handle, 0)
        if status not in (0, 258):  # WAIT_OBJECT_0 (exited), WAIT_TIMEOUT (live)
            raise C.WinError(C.get_last_error())
        creation, exit_time, kernel, user = (W.FILETIME() for _ in range(4))
        if not self.kernel.GetProcessTimes(handle, C.byref(creation), C.byref(exit_time),
                                          C.byref(kernel), C.byref(user)):
            raise C.WinError(C.get_last_error())
        # GetProcessTimes explicitly leaves exit time undefined for a live
        # process. Only read it after the process handle becomes signaled.
        return {"creation_filetime": ticks(creation), "exit_filetime": ticks(exit_time) if status == 0 else 0,
                "kernel_ticks": ticks(kernel), "user_ticks": ticks(user)}

    def memory(self, handle):
        counters = MemoryCounters()
        counters.cb = C.sizeof(counters)
        if not self.psapi.GetProcessMemoryInfo(handle, C.byref(counters), counters.cb):
            raise C.WinError(C.get_last_error())
        return {"working_set_bytes_shared_inclusive": counters.working_set,
                "private_commit_bytes": counters.private_usage}


class OwnedProcessTree:
    def __init__(self, root_pid):
        self.api = WindowsAPI()
        self.root_pid = root_pid
        handle = self.api.open(root_pid)
        try:
            creation = self.api.times(handle)["creation_filetime"]
        except BaseException:
            self.api.kernel.CloseHandle(handle)
            raise
        self.owned = {root_pid: {"pid": root_pid, "parent_pid": None,
                                "name": "super-platinum.exe", "handle": handle,
                                "creation_filetime": creation, "group": "host"}}

    def snapshot(self):
        started = time.monotonic()
        errors = []
        entries = self.api.processes()
        # Iterate the one system snapshot until all reachable levels have been
        # considered. Unrelated browser PIDs are neither opened nor reported.
        remaining = {entry["pid"]: entry for entry in entries if entry["pid"] not in self.owned}
        changed = True
        while changed:
            changed = False
            for pid, entry in list(remaining.items()):
                parent = self.owned.get(entry["parent_pid"])
                if parent is None:
                    continue
                del remaining[pid]
                handle = None
                try:
                    handle = self.api.open(pid)
                    creation = self.api.times(handle)["creation_filetime"]
                    parent_times = self.api.times(parent["handle"])
                    if not parent_lifetime_contains(parent_times, creation):
                        self.api.kernel.CloseHandle(handle)
                        continue  # A reused PID cannot establish ownership.
                    self.owned[pid] = {
                        **entry, "handle": handle, "creation_filetime": creation,
                        "parent_creation_filetime": parent["creation_filetime"],
                        "group": ("webview2_descendants" if entry["name"].lower() == "msedgewebview2.exe"
                                  else "other_owned_descendants"),
                    }
                    changed = True
                except OSError as error:
                    if handle:
                        self.api.kernel.CloseHandle(handle)
                    errors.append({"pid": pid, "parent_pid": entry["parent_pid"],
                                   "stage": "discover", "error": str(error)})
        records = []
        for process in self.owned.values():
            record = {key: value for key, value in process.items() if key != "handle"}
            record["observed_monotonic"] = time.monotonic()
            try:
                counters = self.api.times(process["handle"])
                if counters["creation_filetime"] != process["creation_filetime"]:
                    raise RuntimeError("retained process handle changed identity")
                record.update(counters)
                if not counters["exit_filetime"]:
                    try:
                        record.update(self.api.memory(process["handle"]))
                    except OSError:
                        # An exit between the timing and memory reads is valid:
                        # retain final CPU times, omit no-longer-resident memory.
                        record.update(self.api.times(process["handle"]))
                        if not record["exit_filetime"]:
                            raise
            except OSError as error:
                errors.append({"pid": process["pid"], "stage": "counters", "error": str(error)})
            records.append(record)
        return {"started_monotonic": started, "completed_monotonic": time.monotonic(),
                "processes": records, "errors": errors}

    def close(self):
        for process in self.owned.values():
            self.api.kernel.CloseHandle(process["handle"])
        self.owned.clear()

    def __enter__(self):
        return self

    def __exit__(self, *exception):
        self.close()


def summarize(before, after, window_start_filetime):
    """CPU totals cover observed identities only; no attribution from names alone."""
    initial = {(p["pid"], p["creation_filetime"]): p for p in before["processes"]}
    deltas = []
    groups = {name: {"cpu_seconds_delta_observed": 0.0, "cpu_processes_measured": 0,
                     "working_set_bytes_sum_shared_inclusive": 0,
                     "private_commit_bytes_sum": 0, "memory_processes_measured": 0}
              for name in ("host", "webview2_descendants", "other_owned_descendants", "total_owned")}
    incomplete = []
    for final in after["processes"]:
        key = (final["pid"], final["creation_filetime"])
        first = initial.get(key)
        delta = {"pid": final["pid"], "creation_filetime": final["creation_filetime"],
                 "group": final["group"], "exited": bool(final.get("exit_filetime"))}
        current_cpu = final.get("kernel_ticks", 0) + final.get("user_ticks", 0)
        if "kernel_ticks" not in final or (first is not None and "kernel_ticks" not in first):
            delta["unmeasured_reason"] = "missing timing counters"
        elif first is not None:
            delta["cpu_seconds"] = (current_cpu - first["kernel_ticks"] - first["user_ticks"]) / TICKS_PER_SECOND
            delta["observed_interval_seconds"] = final["observed_monotonic"] - first["observed_monotonic"]
        elif final["creation_filetime"] >= window_start_filetime:
            delta["cpu_seconds"] = current_cpu / TICKS_PER_SECOND
            delta["started_during_window"] = True
        else:
            delta["unmeasured_reason"] = "newly observed process predates quiet window"
        if "unmeasured_reason" in delta:
            incomplete.append(key)
        if delta.get("cpu_seconds", 0) < 0:
            raise RuntimeError("process CPU counter moved backwards")
        for group in (final["group"], "total_owned"):
            aggregate = groups[group]
            if "cpu_seconds" in delta:
                aggregate["cpu_seconds_delta_observed"] += delta["cpu_seconds"]
                aggregate["cpu_processes_measured"] += 1
            if "working_set_bytes_shared_inclusive" in final:
                aggregate["working_set_bytes_sum_shared_inclusive"] += final["working_set_bytes_shared_inclusive"]
                aggregate["private_commit_bytes_sum"] += final["private_commit_bytes"]
                aggregate["memory_processes_measured"] += 1
        deltas.append(delta)
    return {"groups": groups, "memory_snapshot": "after_boundary", "process_cpu_deltas": deltas,
            "unmeasured_observed_identities": incomplete,
            "observed_coverage_complete": not incomplete and not before["errors"] and not after["errors"]}
