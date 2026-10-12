#!/usr/bin/env python3
"""Fixture checks for the process-tree PSS sampler; no desktop session needed."""

import importlib.util
import contextlib
import errno
import io
import json
import os
import shutil
import subprocess
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "process_tree_memory", Path(__file__).with_name("measure-process-tree-memory.py"))
memory = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = memory
spec.loader.exec_module(memory)


def stat(pid, starttime=123, command="shell worker ) (name)", ppid=0, state="S"):
    # State is field 3, starttime is field 22.
    if ppid:
        fields = [state, str(ppid)] + ["0"] * 17 + [str(starttime)]
        return f"{pid} ({command}) " + " ".join(fields)
    return f"{pid} ({command}) " + " ".join(["S", *(["0"] * 18), str(starttime)])


def smaps(pss=20, rss=30, swap=2):
    return f"Pss: {pss} kB\nRss: {rss} kB\nSwapPss: {swap} kB\n"


class MemoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.proc = Path(self.temp.name)
        boot = self.proc / "sys/kernel/random/boot_id"
        boot.parent.mkdir(parents=True)
        boot.write_text("fixture-boot\n")

    def process(self, pid, children=(), starttime=123, threads=(), ppid=0):
        directory = self.proc / str(pid)
        directory.mkdir(exist_ok=True)
        (directory / "stat").write_text(stat(pid, starttime, ppid=ppid))
        (directory / "smaps_rollup").write_text(smaps())
        for tid in (pid, *threads):
            task = directory / "task" / str(tid)
            task.mkdir(parents=True, exist_ok=True)
            (task / "children").write_text(" ".join(map(str, children)))
        return directory

    def test_stat_with_spaces_and_parentheses(self):
        self.assertEqual(memory.parse_stat(stat(42, 987)),
                         (42, "shell worker ) (name)", 0, 987))
        self.assertEqual(memory.parse_stat_record(stat(42, 987, ppid=7, state="Z")),
                         (42, "shell worker ) (name)", "Z", 7, 987))
        with self.assertRaises(ValueError):
            memory.parse_stat("1 (truncated) S 0")

    def test_zombie_child_is_excluded_without_treating_live_memory_errors_as_zero(self):
        self.process(10, [20])
        child = self.process(20, ppid=10)
        (child / "stat").write_text(stat(20, ppid=10, state="Z"))
        (child / "smaps_rollup").unlink()
        sampler = memory.Sampler([10], self.proc)
        sample = sampler.sample("zombie-child")
        self.assertTrue(sample["complete"])
        self.assertEqual([entry["pid"] for entry in sample["processes"]], [10])

    def test_missing_stat_confirms_exit_even_if_proc_directory_still_exists(self):
        directory = self.process(10)
        sampler = memory.Sampler([10], self.proc)
        identity = sampler.roots[0]
        (directory / "stat").unlink()
        self.assertTrue(directory.exists())
        self.assertEqual(sampler.status(identity), "gone")

    def test_threads_descendants_and_duplicate_roots(self):
        self.process(10, [20], threads=[11])
        self.process(20, [30], ppid=10)
        self.process(30, ppid=20)
        sampler = memory.Sampler([10, 20, 10], self.proc)
        sample = sampler.sample("busy")
        self.assertEqual([entry["pid"] for entry in sample["processes"]], [10, 20, 30])
        self.assertEqual(sample["totals"], {"pss_kib": 60, "rss_kib": 90, "swappss_kib": 6})
        self.assertEqual(len(sampler.roots), 2)
        self.assertTrue(sample["complete"])
        self.assertEqual(sample["scenario"], "busy")

    def test_child_visible_only_from_worker_thread(self):
        parent = self.process(10, threads=[11])
        self.process(20, ppid=10)
        (parent / "task/11/children").write_text("20")
        sampler = memory.Sampler([10], self.proc)
        self.assertEqual({entry["pid"] for entry in sampler.sample("busy")["processes"]}, {10, 20})

    def test_reparented_process_and_its_new_descendant_stay_in_scope(self):
        parent = self.process(10, [20])
        child = self.process(20, ppid=10)
        sampler = memory.Sampler([10], self.proc)
        sampler.sample("first")
        (parent / "task/10/children").write_text("")
        (child / "task/20/children").write_text("30")
        self.process(30, ppid=20)
        sample = sampler.sample("reparented")
        self.assertEqual({entry["pid"] for entry in sample["processes"]}, {10, 20, 30})

    @unittest.skipUnless(hasattr(os, "fork"), "requires Linux fork semantics")
    def test_live_reparented_child_is_retained_after_parent_exits(self):
        pid_file = self.proc / "child.pid"
        code = ("import os,time; child=os.fork(); "
                f"open({str(pid_file)!r}, 'w').write(str(child) + '\\n') if child else time.sleep(3); "
                "time.sleep(0.5) if child else None")
        parent = subprocess.Popen([sys.executable, "-c", code])
        try:
            deadline = time.monotonic() + 1
            while not pid_file.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(pid_file.exists())
            child_pid = int(pid_file.read_text())
            sampler = memory.Sampler([parent.pid])
            first = sampler.sample("live-parent-child")
            self.assertIn(child_pid, [entry["pid"] for entry in first["processes"]])
            parent.wait(timeout=2)
            second = sampler.sample("live-reparented-child")
            self.assertTrue(second["complete"], second["errors"])
            self.assertIn(child_pid, [entry["pid"] for entry in second["processes"]])
        finally:
            if parent.poll() is None:
                parent.terminate()
                parent.wait(timeout=2)
            if pid_file.exists():
                child_pid = int(pid_file.read_text())
                try:
                    os.kill(child_pid, 9)
                except ProcessLookupError:
                    pass

    def test_pid_reuse_does_not_retain_reparented_identity(self):
        parent = self.process(10, [20])
        child = self.process(20, ppid=10)
        sampler = memory.Sampler([10], self.proc)
        sampler.sample("first")
        (parent / "task/10/children").write_text("")
        (child / "stat").write_text(stat(20, 456, ppid=99))
        sample = sampler.sample("reuse")
        self.assertEqual([entry["pid"] for entry in sample["processes"]], [10])

    def test_recycled_child_pid_is_not_adopted(self):
        self.process(10, [20])
        self.process(20, ppid=99, starttime=456)
        sampler = memory.Sampler([10], self.proc)
        sample = sampler.sample("recycled-child")
        self.assertEqual([entry["pid"] for entry in sample["processes"]], [10])
        self.assertFalse(sample["complete"])
        self.assertEqual(sample["errors"][0]["operation"], "child_parent")

    def test_child_exit_after_children_snapshot_is_retryable_even_if_directory_remains(self):
        self.process(10, [20])
        child = self.process(20, ppid=10)
        sampler = memory.Sampler([10], self.proc)
        original = Path.read_text
        first = True

        def disappear_after_snapshot(path, *args, **kwargs):
            nonlocal first
            if path == child / "stat" and first:
                first = False
                (child / "stat").unlink()
                raise FileNotFoundError(errno.ENOENT, "fixture child exited")
            return original(path, *args, **kwargs)

        with patch.object(Path, "read_text", disappear_after_snapshot):
            sample = sampler.sample("child-discovery-exit", max_attempts=2)
        self.assertFalse(sample["complete"])
        self.assertEqual(sample["attempt_count"], 2)
        self.assertEqual(sample["attempts"][0]["errors"][0]["operation"], "lifecycle")
        self.assertTrue(sample["attempts"][0]["errors"][0]["retryable_lifecycle"])
        self.assertTrue(child.exists())

    def test_unreadable_live_parent_marks_discovery_incomplete(self):
        parent = self.process(10, [20])
        self.process(20, ppid=10)
        sampler = memory.Sampler([10], self.proc)
        original = Path.read_text

        def deny_parent_stat(path, *args, **kwargs):
            if path == parent / "stat":
                raise PermissionError("fixture permission denied")
            return original(path, *args, **kwargs)

        with patch.object(Path, "read_text", deny_parent_stat):
            sample = sampler.sample("unreadable-parent")
        self.assertFalse(sample["complete"])
        self.assertNotIn(20, [entry["pid"] for entry in sample["processes"]])
        self.assertIn("parent_identity", {error["operation"] for error in sample["errors"]})

    def test_empty_scope_after_root_exit_is_not_a_zero_memory_pass(self):
        root = self.process(10)
        sampler = memory.Sampler([10], self.proc)
        shutil.rmtree(root)
        sample = sampler.sample("root-exited")
        summary = memory.summarize([sample])
        self.assertEqual(sample["processes"], [])
        self.assertFalse(sample["complete"])
        self.assertEqual(sample["errors"][-1]["operation"], "scope")
        self.assertIsNone(summary["max_pss_bytes"])
        self.assertIsNone(summary["median_pss_bytes"])
        self.assertIsNone(summary["threshold_exceeded"])
        self.assertEqual(memory.result_code(summary), 1)

    def test_extra_root_keeps_scope_valid_after_primary_root_exit(self):
        primary = self.process(10)
        self.process(20)
        sampler = memory.Sampler([10, 20], self.proc)
        shutil.rmtree(primary)
        sample = sampler.sample("extra-root")
        self.assertEqual([entry["pid"] for entry in sample["processes"]], [20])
        self.assertFalse(sample["roots"][0]["alive"])
        self.assertTrue(sample["complete"])
        self.assertEqual(memory.result_code(memory.summarize([sample])), 0)

    def test_retained_descendant_keeps_scope_valid_after_root_exit(self):
        primary = self.process(10, [20])
        child = self.process(20, ppid=10)
        sampler = memory.Sampler([10], self.proc)
        self.assertTrue(sampler.sample("before-exit")["complete"])
        shutil.rmtree(primary)
        (child / "stat").write_text(stat(20, ppid=1))
        sample = sampler.sample("child-remains")
        self.assertEqual([entry["pid"] for entry in sample["processes"]], [20])
        self.assertFalse(sample["roots"][0]["alive"])
        self.assertTrue(sample["complete"])
        self.assertEqual(memory.result_code(memory.summarize([sample])), 0)

    def test_zero_pss_for_live_process_is_a_valid_measurement(self):
        root = self.process(10)
        (root / "smaps_rollup").write_text("Pss: 0 kB\nRss: 0 kB\nSwapPss: 0 kB\n")
        sampler = memory.Sampler([10], self.proc)
        sample = sampler.sample("zero-pss")
        summary = memory.summarize([sample])
        self.assertEqual(len(sample["processes"]), 1)
        self.assertTrue(sample["complete"])
        self.assertEqual(summary["max_pss_bytes"], 0)
        self.assertFalse(summary["threshold_exceeded"])
        self.assertEqual(memory.result_code(summary), 0)

    def test_rollup_and_fallback_sum_mapping_pss(self):
        directory = self.process(10)
        sampler = memory.Sampler([10], self.proc)
        identity = sampler.roots[0]
        self.assertEqual(sampler.memory(identity)["source"], "smaps_rollup")
        (directory / "smaps_rollup").unlink()
        (directory / "smaps").write_text(
            "1000-2000 rw-p 0 00:00 0\n" + smaps(11, 20, 1) +
            "2000-3000 rw-p 0 00:00 0\n" + smaps(7, 14, 3))
        result = sampler.memory(identity)
        self.assertEqual((result["source"], result["pss_kib"], result["rss_kib"], result["swappss_kib"]),
                         ("smaps", 18, 34, 4))

    def test_identity_change_during_read_is_skipped_and_marks_incomplete(self):
        directory = self.process(10)
        sampler = memory.Sampler([10], self.proc)
        original = Path.read_text

        def read_then_reuse(path, *args, **kwargs):
            value = original(path, *args, **kwargs)
            if path == directory / "smaps_rollup":
                (directory / "stat").write_text(stat(10, 456))
            return value

        with patch.object(Path, "read_text", read_then_reuse):
            sample = sampler.sample("race")
        self.assertEqual(sample["processes"], [])
        self.assertFalse(sample["complete"])
        self.assertIn("identity changed", sample["errors"][0]["error"])

    def test_exit_during_memory_read_retries_whole_sample_and_preserves_diagnostics(self):
        root = self.process(10, [20])
        self.process(20, ppid=10)
        sampler = memory.Sampler([10], self.proc)
        original_read = Path.read_text
        removed = False

        def exit_during_read(path, *args, **kwargs):
            nonlocal removed
            value = original_read(path, *args, **kwargs)
            if path == self.proc / "20" / "smaps_rollup" and not removed:
                shutil.rmtree(self.proc / "20")
                (root / "task/10/children").write_text("")
                removed = True
            return value

        with patch.object(Path, "read_text", exit_during_read):
            sample = sampler.sample("exit-race", max_attempts=3)
        self.assertTrue(sample["complete"])
        self.assertEqual(sample["attempt_count"], 2)
        self.assertEqual(sample["attempts"][0]["errors"][0]["operation"], "lifecycle")
        self.assertTrue(sample["attempts"][0]["errors"][0]["retryable_lifecycle"])
        self.assertEqual(sample["errors"], [])
        self.assertEqual([process["pid"] for process in sample["processes"]], [10])

    def test_live_process_memory_error_is_not_retried(self):
        self.process(10)
        sampler = memory.Sampler([10], self.proc)
        original_memory = sampler.memory

        def malformed(identity):
            if identity.pid == 10:
                raise ValueError("malformed live smaps")
            return original_memory(identity)

        with patch.object(sampler, "memory", malformed):
            sample = sampler.sample("live-error", max_attempts=4)
        self.assertFalse(sample["complete"])
        self.assertEqual(sample["attempt_count"], 1)
        self.assertEqual(sample["errors"][0]["operation"], "memory")
        self.assertFalse(sample["errors"][0]["retryable_lifecycle"])

    def test_retry_limit_is_bounded_and_keeps_every_attempt(self):
        self.process(10)
        sampler = memory.Sampler([10], self.proc)
        retryable = {**sampler.roots[0].as_dict(), "operation": "lifecycle",
                     "error": "exit race", "retryable_lifecycle": True}
        incomplete = {"type": "sample", "timestamp": "fixture", "scenario": "retry-limit",
                      "processes": [], "totals": {"pss_kib": 0, "rss_kib": 0,
                                                      "swappss_kib": 0},
                      "complete": False, "errors": [retryable]}
        with patch.object(sampler, "sample_once", return_value=incomplete):
            sample = sampler.sample("retry-limit", max_attempts=2)
        self.assertFalse(sample["complete"])
        self.assertEqual(sample["attempt_count"], 2)
        self.assertEqual([attempt["number"] for attempt in sample["attempts"]], [1, 2])

    def test_memory_read_failure_cannot_report_a_complete_sample(self):
        directory = self.process(10)
        (directory / "smaps_rollup").write_text("Rss: 10 kB\n")
        sampler = memory.Sampler([10], self.proc)
        sample = sampler.sample("unreadable")
        self.assertFalse(sample["complete"])
        self.assertEqual(sample["totals"]["pss_kib"], 0)
        self.assertEqual({error["operation"] for error in sample["errors"]},
                         {"memory", "scope"})

    def test_unreadable_tracked_identity_retains_scope_for_retry(self):
        directory = self.process(10)
        sampler = memory.Sampler([10], self.proc)
        (directory / "stat").write_text("malformed")
        sample = sampler.sample("identity-unreadable")
        self.assertFalse(sample["complete"])
        self.assertEqual(sample["processes"], [])
        self.assertEqual(len(sampler.tracked), 1)
        (directory / "stat").write_text(stat(10))
        sample = sampler.sample("identity-restored")
        self.assertTrue(sample["complete"])
        self.assertEqual([entry["pid"] for entry in sample["processes"]], [10])

    def test_summary_uses_aggregate_bytes_and_median(self):
        samples = [{"totals": {"pss_kib": value}, "complete": value != 3}
                   for value in (1, 3, 9, 11)]
        result = memory.summarize(samples)
        self.assertEqual(result["max_pss_bytes"], 11 * 1024)
        self.assertEqual(result["median_pss_bytes"], 9 * 1024)
        self.assertEqual(result["threshold_bytes"], 1_500_000_000)
        self.assertEqual(result["incomplete_sample_count"], 1)
        self.assertIsNone(memory.summarize([])["max_pss_bytes"])

    def test_incomplete_samples_are_excluded_from_threshold_statistics(self):
        samples = [
            {"totals": {"pss_kib": 10}, "complete": True},
            {"totals": {"pss_kib": 99_999_999}, "complete": False, "processes": [{}]},
        ]
        result = memory.summarize(samples)
        self.assertEqual(result["max_pss_bytes"], 10 * 1024)
        self.assertEqual(result["complete_sample_count"], 1)
        self.assertEqual(result["incomplete_sample_count"], 1)
        self.assertEqual(result["max_incomplete_attempt_lower_bound_pss_bytes"],
                         99_999_999 * 1024)
        self.assertTrue(result["threshold_exceeded"])

    def test_old_jsonl_incomplete_subtotal_is_still_a_threshold_lower_bound(self):
        sample = {"totals": {"pss_kib": 2_000_000}, "complete": False, "processes": [{}]}
        summary = memory.summarize([sample])
        self.assertEqual(summary["max_incomplete_attempt_lower_bound_pss_bytes"],
                         2_000_000 * 1024)
        self.assertTrue(summary["threshold_exceeded"])
        self.assertEqual(memory.result_code(summary), 2)

    def test_interrupted_run_with_known_threshold_breach_reports_breach(self):
        summary = {"threshold_exceeded": True, "sample_count": 0,
                   "complete_sample_count": 0, "incomplete_sample_count": 1}
        self.assertEqual(memory.result_code(summary, interrupted=True), 2)

    def test_incomplete_retry_subtotal_above_threshold_still_trips_cap(self):
        sample = {"totals": {"pss_kib": 20}, "complete": True,
                  "attempts": [{"number": 1, "complete": False, "process_count": 1,
                                "totals": {"pss_kib": 2_000_000}, "errors": ["exit race"]},
                               {"number": 2, "complete": True,
                                "totals": {"pss_kib": 20}, "errors": []}]}
        result = memory.summarize([sample])
        self.assertEqual(result["max_pss_bytes"], 20 * 1024)
        self.assertEqual(result["max_incomplete_attempt_lower_bound_pss_bytes"],
                         2_000_000 * 1024)
        self.assertTrue(result["threshold_exceeded"])
        self.assertEqual(memory.result_code(result), 2)

    def test_exit_status_requires_complete_under_threshold_run(self):
        base = {"sample_count": 3, "complete_sample_count": 3,
                "incomplete_sample_count": 0, "threshold_exceeded": False}
        self.assertEqual(memory.result_code(base), 0)
        self.assertEqual(memory.result_code({**base, "threshold_exceeded": True}), 2)
        self.assertEqual(memory.result_code({**base, "incomplete_sample_count": 1}), 1)
        self.assertEqual(memory.result_code({**base, "sample_count": 0,
                                             "complete_sample_count": 0}), 1)
        self.assertEqual(memory.result_code(base, interrupted=True), 130)

    def test_cli_refuses_to_overwrite_existing_output(self):
        output = self.proc / "existing.jsonl"
        output.write_text("keep")
        args = ["measure-process-tree-memory.py", "--root-pid", str(os.getpid()),
                "--scenario", "test", "--output", str(output)]
        with patch.object(sys, "argv", args), contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as error:
                memory.main()
        self.assertEqual(error.exception.code, 2)
        self.assertEqual(output.read_text(), "keep")

    def test_cli_incomplete_run_returns_failure(self):
        output = self.proc / "incomplete.jsonl"

        class FakeRoot:
            def as_dict(self):
                return {"pid": 42, "starttime_ticks": 1, "boot_id": "fixture"}

        class FakeSampler:
            roots = [FakeRoot()]

            def __init__(self, _roots):
                pass

            def sample(self, scenario, max_attempts=3):
                return {"type": "sample", "timestamp": "fixture", "scenario": scenario,
                        "processes": [], "totals": {"pss_kib": 0, "rss_kib": 0,
                                                       "swappss_kib": 0},
                        "complete": False, "errors": [{"operation": "fixture"}]}

        args = ["measure-process-tree-memory.py", "--root-pid", "42", "--scenario", "test",
                "--interval", "1", "--duration", "0.01", "--output", str(output)]
        with patch.object(memory, "Sampler", FakeSampler), patch.object(sys, "argv", args), \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(memory.main(), 1)
        lines = output.read_text().splitlines()
        self.assertEqual(len(lines), 3)
        summary = json.loads(lines[-1])
        self.assertEqual(summary["incomplete_sample_count"], 1)
        self.assertIsNone(summary["threshold_exceeded"])


if __name__ == "__main__":
    unittest.main()
