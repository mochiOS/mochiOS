import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from scripts.testing.evidence import save
from scripts.testing.model import GuestSession, Result, catalog, decode_frame
from scripts.testing.process import command
from scripts.testing.qmp import Frame


class ProtocolTests(unittest.TestCase):
    def session(self):
        return GuestSession("run", [{"id": "first"}, {"id": "second"}])

    def event(self, name, **fields):
        return dict(protocol=1, run_id="run", event=name, **fields)

    def finish_case(self, session, name, status="pass"):
        session.accept(self.event("start", id=name))
        session.accept(self.event("result", id=name, status=status, elapsed_ms=1, detail=""))

    def test_requires_complete_exact_inventory_and_preserves_failure(self):
        session = self.session()
        session.accept(self.event("begin", cases=["first", "second"]))
        self.finish_case(session, "first")
        self.finish_case(session, "second", "fail")
        session.accept(self.event("end", passed=1, failed=1))
        self.assertEqual([r.status for r in session.complete()], ["pass", "fail"])

    def test_missing_summary_or_case_is_not_success(self):
        session = self.session()
        session.accept(self.event("begin", cases=["first", "second"]))
        self.finish_case(session, "first")
        with self.assertRaises(ValueError): session.complete()
        with self.assertRaises(ValueError): session.accept(self.event("end", passed=1, failed=0))

    def test_rejects_wrong_nonce_version_order_skip_and_duplicate(self):
        for frame in [self.event("begin", cases=[]), dict(protocol=1,run_id="old",event="begin"),
                      dict(protocol=True,run_id="run",event="begin")]:
            with self.assertRaises(ValueError): self.session().accept(frame)
        session = self.session()
        session.accept(self.event("begin", cases=["first", "second"]))
        with self.assertRaises(ValueError): session.accept(self.event("start", id="second"))
        session.accept(self.event("start", id="first"))
        with self.assertRaises(ValueError): session.accept(self.event("result", id="first", status="skip", elapsed_ms=0, detail=""))
        with self.assertRaises(ValueError): session.accept(self.event("start", id="first"))

    def test_rejects_forged_summary_and_trailing_results(self):
        session = self.session()
        session.accept(self.event("begin", cases=["first", "second"]))
        self.finish_case(session, "first")
        self.finish_case(session, "second", "fail")
        with self.assertRaises(ValueError): session.accept(self.event("end", passed=2, failed=0))
        session.accept(self.event("end", passed=1, failed=1))
        with self.assertRaises(ValueError): session.accept(self.event("start", id="first"))

    def test_rejects_duplicate_json_truncation_and_size_overflow(self):
        for raw in [b'{"a":1,"a":2}\n',b'{}',b'x'*65537+b'\n',b'[]\n']:
            with self.assertRaises(ValueError): decode_frame(raw)


class ExecutionTests(unittest.TestCase):
    def test_host_exit_status_timeout_and_output_limit(self):
        import os
        import sys
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            for source, expected in [("raise SystemExit(3)", "fail"), ("import time;time.sleep(10)", "error"), ("print('ok')", "pass")]:
                timeout = .1 if expected == "error" else 5
                result = command({"id":"probe", "timeout":timeout, "command":[sys.executable,"-c",source]}, path, path, os.environ.copy())
                self.assertEqual(result.status, expected)
            result = command({"id":"overflow", "timeout":10,"command":[sys.executable,"-c","import os;os.write(1,b'x'*(17*1024*1024))"]},path,path,os.environ.copy())
            self.assertEqual(result.status,"error")
            self.assertIn("log limit",result.detail)

    def test_missing_program_is_error(self):
        import os
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)
            result=command({"id":"missing","timeout":1,"command":["/nonexistent/mochios-test"]},path,path,os.environ.copy())
            self.assertEqual(result.status,"error")

    def test_report_empty_and_incomplete_runs_do_not_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/"run";path.mkdir()
            report={"mode":"kvm"}
            save(path,report,[])
            self.assertEqual(json.loads((path/"result.json").read_text())["status"],"fail")
            save(path,report,[Result("required","error",detail="missing")])
            self.assertIn('errors="1"',(path/"junit.xml").read_text())

    def test_catalog_requires_both_plans_and_valid_timeouts(self):
        root=Path(__file__).resolve().parents[1]
        plan=catalog(root/"tests/catalog.toml")
        self.assertTrue(plan["guest"])
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "bad.toml"
            for invalid in [
                "version=1\nhost=[]\nguest=[]\n",
                "version=true\n",
                "version=1\nhost=[1]\n",
                'version=1\n[[host]]\nid=2\ntimeout=1\ncommand=["true"]\n',
                'version=1\n[[host]]\nid="probe"\ntimeout=true\ncommand=["true"]\n',
            ]:
                path.write_text(invalid)
                with self.assertRaises(ValueError):
                    catalog(path)

    def test_malformed_frames_cannot_be_used_as_gui_evidence(self):
        for data in [b"",b"P6\n1 1\n255\n",b"P6\n99999 1\n255\nabc"]:
            with self.assertRaises(ValueError): Frame(data)

    def test_bad_catalog_creates_a_failed_report_instead_of_leaving_latest_pass(self):
        from scripts.testing.cli import main
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary);(path/"tests").mkdir()
            (path/"tests/catalog.toml").write_text("version=1\nhost=[]\nguest=[]\n")
            self.assertEqual(main(path,["host"]),1)
            self.assertEqual(json.loads((path/"out/tests/host/latest/result.json").read_text())["status"],"fail")

    def test_image_build_failure_records_unexecuted_guest_cases(self):
        from scripts.testing.cli import main
        with tempfile.TemporaryDirectory() as temporary, patch("scripts.testing.cli.source_state",return_value={}), patch("scripts.testing.cli.command",return_value=Result("build.image","fail",detail="compiler error")):
            path=Path(temporary);(path/"tests").mkdir()
            (path/"tests/catalog.toml").write_text('version=1\n[[host]]\nid="host"\ntimeout=1\ncommand=["true"]\n[[guest]]\nid="required"\ntimeout=1\n')
            self.assertEqual(main(path,["kvm","--build"]),1)
            result=json.loads((path/"out/tests/kvm/latest/result.json").read_text())
            self.assertEqual(result["status"],"fail")
            self.assertEqual({r["id"]:r["status"] for r in result["results"]},{"build.image":"fail","runner.infrastructure":"error","required":"error"})

    def test_kvm_preflight_failure_keeps_all_required_cases_failed(self):
        from scripts.testing.kvm import run
        with tempfile.TemporaryDirectory() as temporary, patch("scripts.testing.kvm.socket.socket"), patch("scripts.testing.kvm.VM",side_effect=OSError("KVM unavailable")) as create_vm:
            path=Path(temporary)
            results=run(path,path,path/"disk",[{"id":"required","timeout":1}],"run",1)
            self.assertEqual({r.id:r.status for r in results},{"kvm.infrastructure":"error","required":"error"})
            create_vm.assert_called_once()
            self.assertEqual(results[0].detail,"KVM unavailable")

    def test_interrupted_kvm_run_still_closes_the_owned_vm(self):
        from scripts.testing.kvm import run
        from unittest.mock import Mock
        vm=Mock();vm.boot.side_effect=KeyboardInterrupt()
        with tempfile.TemporaryDirectory() as temporary, patch("scripts.testing.kvm.socket.socket"), patch("scripts.testing.kvm.VM",return_value=vm):
            path=Path(temporary)
            with self.assertRaises(KeyboardInterrupt):run(path,path,path/"disk",[{"id":"required","timeout":1}],"run",1)
            vm.close.assert_called_once()


if __name__ == "__main__":
    unittest.main()
