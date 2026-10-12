#!/usr/bin/env python3
"""Sample Linux PSS for explicitly selected process trees into JSONL.

Scope: roots, descendants observed through every thread's /proc children file,
and explicit extra roots. An observed process stays in scope after reparenting
while its identity (boot ID, PID, start time) remains alive. GPU/kernel memory
is excluded. This measures the selected scenario, not every project workload.
Run as a user allowed to read the selected processes' smaps files.
"""

import argparse
import datetime
import json
import errno
import math
import signal
import statistics
import time
from dataclasses import dataclass
from pathlib import Path


THRESHOLD_BYTES = 1_500_000_000


def parse_stat(text):
    """Return PID, command, PPID and starttime; comm can contain spaces and ')'."""
    pid, command, _state, ppid, starttime = parse_stat_record(text)
    return pid, command, ppid, starttime


def parse_stat_record(text):
    """Return PID, command, state, PPID and starttime from one stat snapshot."""
    left, right = text.find("("), text.rfind(")")
    if left < 0 or right <= left:
        raise ValueError("malformed /proc stat")
    fields = text[right + 1:].split()
    if len(fields) < 20:
        raise ValueError("truncated /proc stat")
    return (int(text[:left].strip()), text[left + 1:right], fields[0],
            int(fields[1]), int(fields[19]))


@dataclass(frozen=True)
class Identity:
    boot_id: str
    pid: int
    starttime: int

    def as_dict(self):
        return {"boot_id": self.boot_id, "pid": self.pid,
                "starttime_ticks": self.starttime}


def parse_memory(text):
    totals = {key: 0 for key in ("Pss", "Rss", "SwapPss")}
    found = set()
    for line in text.splitlines():
        fields = line.split()
        if fields and fields[0].rstrip(":") in totals:
            key = fields[0].rstrip(":")
            if len(fields) != 3 or fields[2] != "kB":
                raise ValueError("invalid smaps memory units")
            value = int(fields[1])
            if value < 0:
                raise ValueError("negative smaps memory")
            totals[key] += value
            found.add(key)
    if found != set(totals):
        raise ValueError("missing Pss/Rss/SwapPss fields")
    return {key.lower() + "_kib": value for key, value in totals.items()}


class Sampler:
    def __init__(self, root_pids, proc_root=Path("/proc")):
        self.proc = Path(proc_root)
        self.boot_id = (self.proc / "sys/kernel/random/boot_id").read_text().strip()
        if not self.boot_id:
            raise ValueError("empty boot ID")
        self.tracked = set()
        self.roots = []
        for pid in dict.fromkeys(root_pids):
            identity, _, _ = self.identity(pid)
            self.roots.append(identity)
            self.tracked.add(identity)

    def identity(self, pid):
        actual_pid, command, ppid, starttime = parse_stat(
            (self.proc / str(pid) / "stat").read_text())
        if actual_pid != pid:
            raise ValueError("stat PID mismatch")
        return Identity(self.boot_id, pid, starttime), command, ppid

    def status(self, identity):
        """Distinguish exit/reuse from unreadable identity data."""
        try:
            actual_pid, _command, state, _ppid, starttime = parse_stat_record(
                (self.proc / str(identity.pid) / "stat").read_text())
            if actual_pid != identity.pid:
                return "unreadable"
            current = Identity(self.boot_id, identity.pid, starttime)
        except OSError as error:
            # During exit, the numeric /proc directory can remain observable
            # briefly after stat itself stops resolving. ENOENT/ESRCH on stat
            # is direct kernel confirmation that this identity has ended.
            if error.errno in (errno.ENOENT, errno.ESRCH):
                return "gone"
            return "unreadable"
        except ValueError:
            return "unreadable"
        if current != identity:
            return "replaced"
        # Zombie/dead tasks retain a PID and stat file until reaped, but no
        # longer own an address space or resident memory.
        return "gone" if state in ("Z", "X", "x") else "same"

    def alive(self, identity):
        return self.status(identity) == "same"

    def discover(self):
        """Discover descendants, retaining prior identities after reparenting."""
        errors = []
        surviving = set()
        for identity in self.tracked:
            status = self.status(identity)
            if status == "same":
                surviving.add(identity)
            elif status == "unreadable":
                errors.append({**identity.as_dict(), "operation": "identity",
                               "error": "process identity is unreadable; retained for retry"})
                # Retry on the next sample instead of silently losing scope.
                surviving.add(identity)
        self.tracked = surviving
        queue = list(self.tracked)
        visited = set()
        while queue:
            parent = queue.pop()
            if parent in visited:
                continue
            parent_status = self.status(parent)
            if parent_status == "unreadable":
                errors.append({**parent.as_dict(), "operation": "parent_identity",
                               "error": "cannot validate parent identity during discovery"})
                continue
            if parent_status != "same":
                continue
            visited.add(parent)
            try:
                tasks = list((self.proc / str(parent.pid) / "task").iterdir())
                children = {}
                for task in tasks:
                    try:
                        for child_pid in (int(pid) for pid in (task / "children").read_text().split()):
                            children.setdefault(child_pid, set()).add(int(task.name))
                    except (OSError, ValueError) as error:
                        # A thread can disappear while enumerating tasks.
                        if task.exists():
                            errors.append({"pid": parent.pid, "operation": "children",
                                           "error": str(error)})
                parent_status = self.status(parent)
                if parent_status == "unreadable":
                    errors.append({**parent.as_dict(), "operation": "parent_identity",
                                   "error": "cannot validate parent identity after children scan"})
                    continue
                if parent_status != "same":
                    if parent_status in ("gone", "replaced"):
                        errors.append({**parent.as_dict(), "operation": "lifecycle",
                                       "event": "process exited during descendant discovery",
                                       "error": "parent exited or changed identity during children scan",
                                       "retryable_lifecycle": True})
                    else:
                        errors.append({**parent.as_dict(), "operation": "parent_identity",
                                       "error": "parent identity became unreadable during children scan"})
                    continue
                for pid, task_ids in children.items():
                    try:
                        child, _command, child_ppid = self.identity(pid)
                    except (OSError, ValueError) as error:
                        if isinstance(error, OSError) and error.errno in (errno.ENOENT, errno.ESRCH):
                            errors.append({"pid": pid, "operation": "lifecycle",
                                           "event": "child exited during descendant discovery",
                                           "error": str(error), "retryable_lifecycle": True})
                        elif (self.proc / str(pid)).exists():
                            errors.append({"pid": pid, "operation": "child_identity",
                                           "error": str(error)})
                        continue  # Child may have exited during discovery.
                    child_status = self.status(child)
                    if child_status == "gone":
                        continue  # Zombie/dead tasks own no resident address space.
                    if child_status == "unreadable":
                        errors.append({**child.as_dict(), "operation": "child_identity",
                                       "error": "cannot validate child identity state"})
                        continue
                    # `/proc/.../children` is a PID snapshot. The PID may have
                    # exited and been recycled before stat is read. Adopt only
                    # when the current process still names this process/thread
                    # as its parent; otherwise the relationship is uncertain.
                    if child_ppid not in task_ids and child_ppid != parent.pid:
                        errors.append({"pid": pid, "operation": "child_parent",
                                       "error": "current process is not parented by the scanned process"})
                        continue
                    if self.status(parent) != "same":
                        errors.append({**parent.as_dict(), "operation": "parent_identity",
                                       "error": "parent identity changed while validating a child"})
                        continue
                    self.tracked.add(child)
                    queue.append(child)
            except OSError as error:
                if self.status(parent) != "gone":
                    errors.append({"pid": parent.pid, "operation": "tasks",
                                   "error": str(error)})
        return errors

    def memory(self, identity):
        before, command, _ppid = self.identity(identity.pid)
        if before != identity:
            raise ValueError("process identity changed before memory read")
        failures = []
        for filename in ("smaps_rollup", "smaps"):
            try:
                memory = parse_memory((self.proc / str(identity.pid) / filename).read_text())
                break
            except (OSError, ValueError) as error:
                failures.append(str(error))
        else:
            raise ValueError("cannot read memory: " + "; ".join(failures))
        if self.identity(identity.pid)[0] != identity:
            raise ValueError("process identity changed during memory read")
        return {**identity.as_dict(), "command": command, "source": filename, **memory}

    def sample_once(self, scenario):
        started_at = datetime.datetime.now(datetime.timezone.utc)
        started_clock = time.monotonic()
        errors = self.discover()
        inventory = []
        for identity in sorted(self.tracked, key=lambda entry: (entry.pid, entry.starttime)):
            try:
                inventory.append(self.memory(identity))
            except (OSError, ValueError) as error:
                errors.append(self.memory_failure(identity, error))
        if not inventory:
            errors.append({"operation": "scope", "error": "no selected processes could be measured"})
        totals = {key: sum(process[key] for process in inventory)
                  for key in ("pss_kib", "rss_kib", "swappss_kib")}
        ended_at = datetime.datetime.now(datetime.timezone.utc)
        return {"type": "sample", "timestamp": ended_at.isoformat(),
                "started_at": started_at.isoformat(),
                "duration_ms": round((time.monotonic() - started_clock) * 1000, 3),
                "scenario": scenario, "roots": [{**root.as_dict(), "alive": self.alive(root)}
                                                 for root in self.roots],
                "processes": inventory, "totals": totals,
                "complete": not errors, "errors": errors}

    def sample(self, scenario, max_attempts=3):
        """Retry a whole sample only when every failure confirms process exit/reuse."""
        attempts = []
        for number in range(1, max_attempts + 1):
            result = self.sample_once(scenario)
            attempts.append({"number": number, "timestamp": result["timestamp"],
                             "complete": result["complete"], "totals": result["totals"],
                             "process_count": len(result["processes"]),
                             "errors": result["errors"]})
            if result["complete"] or not result["errors"] or not all(
                    error.get("retryable_lifecycle", False) for error in result["errors"]):
                break
        result["attempts"] = attempts
        result["attempt_count"] = len(attempts)
        return result

    def memory_failure(self, identity, error):
        """Classify a missing memory read only after rechecking PID identity."""
        status = self.status(identity)
        if status in ("gone", "replaced"):
            return {**identity.as_dict(), "operation": "lifecycle",
                    "event": "process exited during sample" if status == "gone"
                    else "PID identity was replaced during sample",
                    "error": str(error), "retryable_lifecycle": True}
        return {**identity.as_dict(), "operation": "memory", "error": str(error),
                "retryable_lifecycle": False}


def summarize(samples):
    complete_values = [sample["totals"]["pss_kib"] * 1024
                       for sample in samples if sample["complete"]]
    incomplete_attempt_values = []
    for sample in samples:
        attempts = sample.get("attempts")
        if attempts is None:
            # Preserve evidence from JSONL generated before attempt histories
            # were added; its sample subtotal is still a lower bound.
            attempts = [sample]
        for attempt in attempts:
            if not attempt.get("complete", sample.get("complete", False)) \
                    and attempt.get("process_count", len(sample.get("processes", []))) > 0:
                totals = attempt.get("totals", sample.get("totals", {}))
                incomplete_attempt_values.append(totals["pss_kib"] * 1024)
    incomplete_count = sum(not sample["complete"] for sample in samples)
    maximum = max(complete_values) if complete_values else None
    partial_maximum = max(incomplete_attempt_values) if incomplete_attempt_values else None
    threshold_exceeded = (
        maximum > THRESHOLD_BYTES if maximum is not None else False
    ) or (partial_maximum is not None and partial_maximum > THRESHOLD_BYTES)
    return {"type": "summary", "sample_count": len(samples),
            "complete_sample_count": len(complete_values),
            "incomplete_sample_count": incomplete_count,
            "max_pss_bytes": maximum,
            "median_pss_bytes": statistics.median(complete_values) if complete_values else None,
            "max_incomplete_attempt_lower_bound_pss_bytes": partial_maximum,
            "threshold_bytes": THRESHOLD_BYTES,
            "threshold_exceeded": (threshold_exceeded if maximum is not None
                                   or partial_maximum is not None else None),
            "scope": "explicit roots + observed descendants + explicit extra roots; GPU/kernel memory excluded"}


def result_code(summary, interrupted=False):
    if summary["threshold_exceeded"] is True:
        return 2
    if interrupted:
        return 130
    if (summary["sample_count"] == 0 or summary["incomplete_sample_count"]
            or not summary["complete_sample_count"]):
        return 1
    return 0


def positive_pid(value):
    pid = int(value)
    if pid <= 0:
        raise argparse.ArgumentTypeError("PID must be positive")
    return pid


def positive_seconds(value):
    number = float(value)
    if not math.isfinite(number) or number <= 0:
        raise argparse.ArgumentTypeError("seconds must be finite and positive")
    return number


def main():
    parser = argparse.ArgumentParser(
        description=__doc__,
        epilog="Exit codes: 0 = complete below threshold; 1 = incomplete/no samples; "
               "2 = complete but above threshold; 130 = interrupted (no pass).")
    parser.add_argument("--root-pid", type=positive_pid, required=True,
                        help="PID you have verified belongs to the project")
    parser.add_argument("--extra-root-pid", type=positive_pid, action="append", default=[])
    parser.add_argument("--scenario", required=True, help="workload being measured")
    parser.add_argument("--interval", type=positive_seconds, default=1.0)
    parser.add_argument("--duration", type=positive_seconds, default=60.0)
    parser.add_argument("--max-attempts", type=int, choices=range(1, 11), default=3,
                        help="whole-sample retries after confirmed lifecycle races (default: 3 attempts)")
    parser.add_argument("--output", type=Path, required=True, help="new JSONL file (never overwritten)")
    args = parser.parse_args()
    if not args.scenario.strip():
        parser.error("scenario must not be blank")
    try:
        sampler = Sampler([args.root_pid, *args.extra_root_pid])
        output = args.output.open("x", encoding="utf-8")
    except (OSError, ValueError) as error:
        parser.error(str(error))
    interrupted = False

    def stop(_signum, _frame):
        nonlocal interrupted
        interrupted = True

    old_handler = signal.signal(signal.SIGINT, stop)
    samples = []
    started = time.monotonic()
    deadline = started + args.duration
    next_sample = started
    try:
        with output:
            metadata = {"type": "metadata", "scenario": args.scenario,
                        "roots": [root.as_dict() for root in sampler.roots],
                        "interval_seconds": args.interval, "duration_seconds": args.duration,
                        "max_attempts": args.max_attempts,
                        "threshold_bytes": THRESHOLD_BYTES,
                        "scope": summarize([])["scope"]}
            output.write(json.dumps(metadata) + "\n")
            while not interrupted and time.monotonic() < deadline:
                sample = sampler.sample(args.scenario, args.max_attempts)
                samples.append(sample)
                output.write(json.dumps(sample) + "\n")
                output.flush()
                next_sample += args.interval
                # Bound sleeps so SIGINT is handled without a long delay.
                while not interrupted and time.monotonic() < min(next_sample, deadline):
                    time.sleep(min(0.1, max(0, min(next_sample, deadline) - time.monotonic())))
                if next_sample < time.monotonic():
                    next_sample = time.monotonic()
            summary = summarize(samples)
            summary["interrupted"] = interrupted
            output.write(json.dumps(summary) + "\n")
    finally:
        signal.signal(signal.SIGINT, old_handler)
    print(f"Aggregate PSS: max {summary['max_pss_bytes']} bytes; "
          f"median {summary['median_pss_bytes']} bytes; threshold {THRESHOLD_BYTES} bytes")
    print(f"Threshold exceeded: {summary['threshold_exceeded']}")
    print(f"Samples: {summary['sample_count']}; incomplete: {summary['incomplete_sample_count']}")
    print(summary["scope"])
    return result_code(summary, interrupted)


if __name__ == "__main__":
    raise SystemExit(main())
