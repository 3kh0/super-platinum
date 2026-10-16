"""Pure oracle regressions. Does not start HTTP/native or open a Save dialog."""
import copy
import unittest
import sys
from pathlib import Path
# Read-only dependency path for this staged test; installed script imports normally.
sys.path.append(str(next(parent / "scripts" for parent in Path(__file__).resolve().parents
                         if (parent / "scripts/native_check.py").is_file())))
from original_download_native import (BYTES, STAGES, LocalOriginal, validate_stage, validate_terminal)

OWNER = {"version":1,"nonce":"0"*32,"pid":123,"fixture":"original-download"}


def stage(index):
    name = STAGES[index]
    busy = name in ("busy", "retry-busy", "close-busy")
    closed = name == "closed"
    outcomes = [[], [], [[1,"canceled"]], [[1,"canceled"]], [[1,"canceled"],[2,"saved"]],
                [[1,"canceled"],[2,"saved"]], [[1,"canceled"],[2,"saved"],[3,"canceled"]]][index]
    files = [["attempt-2.bin",BYTES]] if index >= 4 else []
    if busy:
        files.insert(0,[".super-platinum-download-01234567-89ab-cdef-0123-456789abcdef.part",65536])
    return {"dom":{"stage":name,"closed":closed,"viewport":__import__("native_check").expected_viewport("original-download"),"control":None if closed else {
        "label":"Cancel" if busy else "Download", "aria":"Cancel download original-fixture.bin" if busy else "Download original-fixture.bin",
        "disabled":False,"inside":True,"hit":True,"x":200,"y":400,"width":96,"height":32}},
        "files":files,"issued":[0,1,1,2,2,3,3][index],"outcomes":outcomes,"noSession":True,"noTransport":True,
        "toast":"Saved original-fixture.bin" if name == "saved" else None}


def terminal(rows):
    return dict(OWNER,ok=True,stages=rows,actualHandlers=True,syntheticAdmissionTransport=True,
                noSession=True,noTransport=True,osSaveDialog=False,bytes=BYTES,
                outcomes=[[1,"canceled"],[2,"saved"],[3,"canceled"]])


class Oracle(unittest.TestCase):
    def test_thread_start_failure_preserves_primary_when_socket_close_fails(self):
        import original_download_native as module
        from unittest.mock import Mock, patch
        server=Mock();thread=Mock()
        primary=RuntimeError("thread start failed")
        thread.start.side_effect=primary
        server.server_close.side_effect=RuntimeError("socket close failed")
        with patch.object(module,"ThreadingHTTPServer",return_value=server), patch.object(module.threading,"Thread",return_value=thread):
            with self.assertRaises(RuntimeError) as raised: module.LocalOriginal(OWNER["nonce"])
        self.assertIs(raised.exception,primary)
        server.server_close.assert_called_once()
        self.assertTrue(any("socket close failed" in note for note in primary.__notes__))

    def test_actual_handler_rejects_and_records_every_unexpected_method(self):
        import io
        import original_download_native as module
        from unittest.mock import Mock, patch
        class Socket:
            def __init__(self, request): self.request=request;self.sent=bytearray()
            def settimeout(self, value): pass
            def makefile(self, *_): return io.BytesIO(self.request)
            def sendall(self, value): self.sent.extend(value)
        for method in ("HEAD", "PUT", "OPTIONS", "DELETE", "PATCH", "POST", "CUSTOM"):
            captured = {}
            def server_factory(address, handler):
                captured["handler"] = handler
                return Mock(server_address=address)
            with patch.object(module,"ThreadingHTTPServer",side_effect=server_factory), patch.object(module.threading,"Thread"):
                owner=module.LocalOriginal(OWNER["nonce"])
            socket=Socket((method+" /original/"+OWNER["nonce"]+" HTTP/1.1\r\nHost: localhost\r\n\r\n").encode())
            captured["handler"](socket,("127.0.0.1",123),owner.server)
            self.assertIn(b"405",socket.sent)
            self.assertEqual(owner.rows,[{"error":"unexpected method","method":method}])
            with self.assertRaises(RuntimeError): owner.validate()
            for _ in range(10):
                socket=Socket(b"HEAD / HTTP/1.1\r\nHost: localhost\r\n\r\n")
                captured["handler"](socket,("127.0.0.1",123),owner.server)
            self.assertEqual(len(owner.rows),4)

    def test_teardown_attempts_all_cleanup_and_preserves_first_failure(self):
        from unittest.mock import Mock
        for failed in ("shutdown", "server_close"):
            owner=object.__new__(LocalOriginal)
            owner.gates=[Mock(),Mock(),Mock()]
            owner.server=Mock()
            owner.thread=Mock()
            owner.thread.is_alive.return_value=False
            primary=RuntimeError(failed)
            getattr(owner.server,failed).side_effect=primary
            owner.thread.join.side_effect=RuntimeError("secondary join")
            with self.assertRaises(RuntimeError) as raised: owner.close()
            self.assertIs(raised.exception,primary)
            owner.server.shutdown.assert_called_once()
            owner.server.server_close.assert_called_once()
            owner.thread.join.assert_called_once_with(timeout=5)
            owner.thread.is_alive.assert_called_once()
            self.assertTrue(any("secondary join" in note for note in primary.__notes__))
            for gate in owner.gates: gate.set.assert_called_once()

    def test_closed_destination_rechecks_full_inventory_and_compares_digest(self):
        import original_download_native as module
        from unittest.mock import patch
        original={"name":"attempt-2.bin","bytes":BYTES,"sha256":"original"}
        with patch.object(module,"validate_files",return_value=original.copy()) as check:
            self.assertEqual(module.validate_closed_files(Path("owned"),original),original)
            check.assert_called_once_with(Path("owned"))
        for changed in (dict(original,sha256="changed"),dict(original,bytes=BYTES-1)):
            with patch.object(module,"validate_files",return_value=changed):
                with self.assertRaises(RuntimeError): module.validate_closed_files(Path("owned"),original)
        with patch.object(module,"validate_files",side_effect=RuntimeError("extra partial")):
            with self.assertRaisesRegex(RuntimeError,"extra partial"): module.validate_closed_files(Path("owned"),original)

    def test_all_seven_stages_and_exact_terminal(self):
        rows = [stage(i) for i in range(7)]
        validate_terminal(terminal(rows),rows,OWNER)

    def test_missing_reordered_or_duplicate_stage_refused(self):
        for indexes in ([0,1,2,3,4,5], [0,2,1,3,4,5,6], [0,1,2,3,4,5,5]):
            rows = [stage(i) for i in indexes]
            with self.assertRaises(RuntimeError): validate_terminal(terminal(rows),rows,OWNER)

    def test_clipping_coverage_disabled_and_width_shift_refused(self):
        for key,value in (("hit",False),("disabled",True),("inside",False),("width",80),("width",112),("x",1260),("height",float("nan"))):
            row = stage(1)
            row["dom"]["control"][key] = value
            with self.assertRaises(RuntimeError): validate_stage(row,1,96)

    def test_cancellation_cannot_claim_saved_or_leave_parts(self):
        for mutate in (lambda row: row.update(toast="Saved original-fixture.bin"),
                       lambda row: row.update(outcomes=[[1,"saved"]]),
                       lambda row: row.update(files=[["attempt-1.bin",65536]]),
                       lambda row: row.update(issued=2)):
            row = stage(2);mutate(row)
            with self.assertRaises(RuntimeError): validate_stage(row,2,96)

    def test_busy_requires_real_partial_and_completed_requires_full_bytes(self):
        for index,files in ((1,[]),(3,[["attempt-2.bin",BYTES]]),(4,[["attempt-2.bin",BYTES-1]]),
                            (6,[["attempt-2.bin",BYTES],["attempt-3.bin",65536]])):
            row = stage(index);row["files"] = files
            with self.assertRaises(RuntimeError): validate_stage(row,index,96)

    def test_owner_and_scope_cannot_claim_auth_or_os_dialog(self):
        rows = [stage(i) for i in range(7)]
        for key,value in (("pid",124),("nonce","1"*32),("fixture","other"),("osSaveDialog",True),
                          ("noTransport",False),("syntheticAdmissionTransport",False)):
            value_row = terminal(rows);value_row[key] = value
            with self.assertRaises(RuntimeError): validate_terminal(value_row,rows,OWNER)

    def test_normal_narrow_and_closed_viewports_are_exact(self):
        from native_check import expected_viewport
        for index in (0,6):
            for fixture in ("original-download","original-download-narrow"):
                row=stage(index);row["dom"]["viewport"]=expected_viewport(fixture)
                if row["dom"]["control"]: row["dom"]["control"]["y"]=100
                validate_stage(row,index,96,fixture)
                for axis in (0,1):
                    bad=copy.deepcopy(row);bad["dom"]["viewport"][axis]+=1
                    with self.assertRaises(RuntimeError): validate_stage(bad,index,96,fixture)

    def test_native_constructor_failure_closes_server_and_retains_primary(self):
        import importlib.util,tempfile,json
        from unittest.mock import patch
        path=Path(__file__).with_name("check-original-download-native.py")
        spec=importlib.util.spec_from_file_location("download_runner_under_test",path)
        runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)
        class Server:
            port=12345
            rows=[]
            closed=False
            def __init__(self,*_): pass
            def close(self): self.closed=True
        server=Server()
        with tempfile.TemporaryDirectory() as folder:
            with patch.object(runner,"LocalOriginal",return_value=server), patch.object(runner,"NativeFixture",side_effect=RuntimeError("constructor failed")):
                with self.assertRaisesRegex(RuntimeError,"constructor failed"):
                    runner.run("original-download",Path("unused.exe"),Path(folder))
            self.assertTrue(server.closed)
            receipt=json.loads((Path(folder)/"original-download/receipt.json").read_text())
            self.assertFalse(receipt["ok"])
            self.assertIn("constructor failed",receipt["error"])
            self.assertEqual(receipt["httpLedger"],[])

    def test_http_exact_three_gets_no_credentials_or_replay(self):
        server = object.__new__(LocalOriginal);server.nonce = OWNER["nonce"]
        rows = [{"index":i+1,"method":"GET","path":"/original/"+server.nonce,
                 "cookieAbsent":True,"authorizationAbsent":True,"encoding":"identity",
                 "bytesSent":BYTES if i == 1 else 65536,"closed":True} for i in range(3)]
        server.rows = rows;server.validate()
        for bad in (rows[:2], rows+[copy.deepcopy(rows[2])]):
            server.rows = bad
            with self.assertRaises(RuntimeError): server.validate()
        for key,value in (("cookieAbsent",False),("authorizationAbsent",False),("method","POST"),
                          ("path","/other"),("bytesSent",0),("closed",False)):
            server.rows = copy.deepcopy(rows);server.rows[0][key] = value
            with self.assertRaises(RuntimeError): server.validate()


if __name__ == "__main__":
    unittest.main()
