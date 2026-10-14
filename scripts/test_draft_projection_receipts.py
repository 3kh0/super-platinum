"""Pure fixture receipt validation; no app launch or storage reads."""
import copy
import runpy
import unittest
import tempfile
import struct
from pathlib import Path
from native_check import expected_viewport
R=runpy.run_path(str(Path(__file__).with_name("check-draft-projection-native.py")))
class Receipts(unittest.TestCase):
    def good(self,scenario="ahead",phase="exercise"):
        result=dict.fromkeys(R["SPEC"].required,True)
        result.update(ok=True,preparedRequests=0,viewport=expected_viewport("draft-native-projection"),scenario=scenario,phase=phase,nonce="a"*32,pid=11,isolatedDatabase="private.sqlite",routedInputs=2,durable={},results={})
        for target in ("channel","thread"):
            local={"composer":{"text":R["AFTER"],"reply_broadcast":target=="thread"},"attachments":[]}
            newer=copy.deepcopy(local);newer["composer"]["text"]=R["NEWER"]
            alternate={"writer":"fixture-writer","serial":1,"local":newer}
            result["durable"][target]={"key":{"team":"T1","channel":"C2","thread":"1719800400.000000" if target=="thread" else None},"canonical":local,"storageRevision":2,"blocked":{"token":"fixture-blocked","local":newer} if scenario in R["ALTERNATE"] else None,"alternates":[alternate] if scenario in R["ALTERNATE"] else []}
            text=R["AFTER"] if phase=="restart" or scenario=="apply" else R["NEWER"] if scenario in R["ALTERNATE"] else "Original \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"
            result["results"][target]={"ok":True,"serial":1,"barrierRetained":scenario!="apply","native":{"text":text,"display":text,"codePoints":[ord(c) for c in text],"utf16Length":len(text.encode("utf-16-le"))//2},"preservation":{**alternate,"storageRevision":2}}
        return result
    def validate(self,receipt):
        R["validate"]("draft-native-projection",receipt["scenario"],receipt["phase"],"a"*32,11,Path("private.sqlite"),receipt)
    def test_every_scenario_and_phase(self):
        for scenario in R["SCENARIOS"]:
            for phase in ("exercise","restart"):
                self.validate(self.good(scenario,phase))
    def test_rejects_false_missing_isolation_and_wrong_owned_identity(self):
        for flag in R["SPEC"].required:
            for value in (None,False):
                receipt=self.good();receipt[flag]=value
                with self.assertRaises(RuntimeError):self.validate(receipt)
        for key,value in (("nonce","b"*32),("pid",12),("isolatedDatabase","elsewhere.sqlite"),("preparedRequests",1)):
            receipt=self.good();receipt[key]=value
            with self.assertRaises(RuntimeError):self.validate(receipt)
    def test_rejects_similar_content_from_other_writer_or_serial(self):
        for key,value in (("writer","different-writer"),("serial",2),("local",{}),("storageRevision",3)):
            receipt=self.good();receipt["results"]["thread"]["preservation"][key]=value
            with self.assertRaises(RuntimeError):self.validate(receipt)
    def test_png_header_rejects_corrupt_or_zero_sized_files(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/"capture.png"
            for data in (b"",b"stale screenshot",b"\x89PNG\r\n\x1a\n"+struct.pack(">I",13)+b"IHDR"+struct.pack(">II",0,1)):
                path.write_bytes(data)
                with self.assertRaises(RuntimeError):R["valid_png"](path)
    def test_every_input_case_requires_exact_final_editor_serial(self):
        for scenario in R["ALTERNATE"]:
            for value in (None, True, 0, 2):
                receipt=self.good(scenario)
                receipt["results"]["thread"]["serial"]=value
                with self.subTest(scenario=scenario,serial=value):
                    with self.assertRaises(RuntimeError):self.validate(receipt)
    def test_readiness_tuple_rejects_wrong_owner_phase_and_extra_fields(self):
        expected=R["ready_tuple"]("a"*32,42,"apply","exercise")
        R["validate_ready"](expected,expected)
        for key,value in (("version",True),("nonce","b"*32),("pid",43),("scenario","ahead"),("phase","restart")):
            changed={**expected,key:value}
            with self.assertRaises(RuntimeError):R["validate_ready"](changed,expected)
            missing=expected.copy();del missing[key]
            with self.assertRaises(RuntimeError):R["validate_ready"](missing,expected)
        with self.assertRaises(RuntimeError):R["validate_ready"]({**expected,"extra":True},expected)
    def test_readiness_publication_never_overwrites_an_existing_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            database=Path(directory)/"draft-projection.sqlite"
            expected=R["ready_tuple"]("a"*32,42,"apply","exercise")
            R["release_ready"](database,expected)
            marker=database.parent/"ready-exercise.json"
            original=marker.read_bytes()
            with self.assertRaises(FileExistsError):R["release_ready"](database,{**expected,"pid":43})
            self.assertEqual(marker.read_bytes(),original)
            self.assertFalse(marker.with_suffix(".staged").exists())
    def test_rejects_wrong_thread_broadcast_or_barrier(self):
        for alter in (lambda r:r["durable"]["thread"]["canonical"]["composer"].update(reply_broadcast=False),lambda r:r["results"]["channel"].update(barrierRetained=False),lambda r:r["results"]["thread"]["native"].update(text="lost newer input")):
            receipt=self.good();alter(receipt)
            with self.assertRaises(RuntimeError):self.validate(receipt)
if __name__=="__main__":unittest.main()
