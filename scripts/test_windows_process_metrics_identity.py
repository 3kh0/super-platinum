"""Offline tests; no native calls, enumeration, subprocesses or app launch."""
import ctypes as C
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import windows_process_metrics as m

class FakeAPI:
    def __init__(self, created=50, query_error=False, retained_error=False, replaced=False, parent_exit=0):
        self.created,self.query_error,self.retained_error,self.replaced=created,query_error,retained_error,replaced
        self.parent_exit=parent_exit;self.opens=[];self.closed=[]
        self.kernel=SimpleNamespace(CloseHandle=self.closed.append)
    def processes(self):
        return [{"pid":2,"parent_pid":1,"name":"candidate.exe"}]
    def creation(self,pid):
        if self.query_error: raise OSError("query denied")
        return self.created
    def open(self,pid):
        self.opens.append(pid)
        if pid==2 and self.retained_error: raise OSError("retain denied")
        return pid
    def times(self,handle):
        return {"creation_filetime":100 if handle==1 else self.created+int(self.replaced),
                "exit_filetime":self.parent_exit if handle==1 else 0,"kernel_ticks":10,"user_ticks":20}
    def memory(self,handle):
        return {"working_set_bytes_shared_inclusive":100,"private_commit_bytes":50}

class IdentityTests(unittest.TestCase):
    def collect(self,api):
        with patch.object(m,"WindowsAPI",return_value=api):
            with m.OwnedProcessTree(1) as tree:
                before=tree.snapshot();after=tree.snapshot()
                result=m.summarize(before,after,1000)
        return before,after,result
    def test_old_parent_pid_candidate_is_excluded_before_retained_open(self):
        a=FakeAPI(created=50,retained_error=True);b,z,s=self.collect(a)
        self.assertEqual(a.opens,[1]);self.assertEqual(a.closed,[1])
        self.assertEqual(b["errors"],[]);self.assertTrue(s["observed_coverage_complete"])
        self.assertEqual([x["pid"] for x in z["processes"]],[1])
    def test_child_after_parent_exit_is_excluded(self):
        a=FakeAPI(created=201,parent_exit=200,retained_error=True);_,_,s=self.collect(a)
        self.assertEqual(a.opens,[1]);self.assertTrue(s["observed_coverage_complete"])
    def test_query_denial_is_incomplete_not_ignored(self):
        a=FakeAPI(query_error=True);b,_,s=self.collect(a)
        self.assertFalse(s["observed_coverage_complete"])
        self.assertEqual(b["errors"][0]["access_stage"],"query_creation")
        self.assertEqual(b["errors"][0]["name"],"candidate.exe")
    def test_real_candidate_retained_denial_is_still_incomplete(self):
        a=FakeAPI(created=150,retained_error=True);b,_,s=self.collect(a)
        self.assertFalse(s["observed_coverage_complete"])
        self.assertEqual(b["errors"][0]["access_stage"],"retain_and_validate")
    def test_pid_reuse_between_query_and_retained_open_fails_and_closes(self):
        a=FakeAPI(created=150,replaced=True);b,_,s=self.collect(a)
        self.assertFalse(s["observed_coverage_complete"])
        self.assertIn("changed identity",b["errors"][0]["error"])
        self.assertEqual(a.closed,[2,2,1])
    def test_valid_child_retains_identity_and_full_counters(self):
        a=FakeAPI(created=150);b,z,s=self.collect(a)
        self.assertTrue(s["observed_coverage_complete"])
        self.assertEqual(a.opens,[1,2]);self.assertEqual(a.closed,[1,2])
        self.assertEqual(s["groups"]["total_owned"]["cpu_processes_measured"],2)
        self.assertEqual(s["groups"]["total_owned"]["memory_processes_measured"],2)
    def test_query_only_api_closes_and_ignores_undefined_exit(self):
        calls=[]
        def get_times(handle,created,exited,kernel,user):
            created._obj.dwLowDateTime=123;exited._obj.dwLowDateTime=999;return True
        a=object.__new__(m.WindowsAPI)
        a.kernel=SimpleNamespace(OpenProcess=lambda rights,inherit,pid:calls.append((rights,inherit,pid)) or 7,
            GetProcessTimes=get_times,CloseHandle=lambda h:calls.append(("close",h)))
        self.assertEqual(a.creation(42),123)
        self.assertEqual(calls,[(0x1000,False,42),("close",7)])
    def test_query_failure_closes_handle(self):
        closed=[];a=object.__new__(m.WindowsAPI)
        a.kernel=SimpleNamespace(OpenProcess=lambda *_:7,GetProcessTimes=lambda *_:False,CloseHandle=closed.append)
        with self.assertRaises(OSError):a.creation(42)
        self.assertEqual(closed,[7])

if __name__=="__main__":unittest.main()
