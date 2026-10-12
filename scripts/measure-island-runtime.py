#!/usr/bin/env python3
"""Measure the real iSland shell in a private headless Wayland session.

The outer driver uses Bubblewrap and temporary XDG directories. Inside the
namespace it launches the checkout's shell.qml, drives the production `iris`
IPC handler, and samples only Quickshell plus descendants observed from it.
The private compositor, D-Bus, controller and sampler are outside that scope.
"""

import argparse
import datetime
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path("/tmp/audit-source")
WORK = Path("/tmp/audit-work")
MAX_SAMPLE_ATTEMPTS = 10


def module_from_file(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def inner(duration, output_path):
    if ROOT != SOURCE or os.environ.get("XDG_RUNTIME_DIR") != str(WORK / "runtime"):
        raise RuntimeError("sandbox runner must execute inside its private Bubblewrap session")
    sampler_module = module_from_file("process_tree_memory",
                                      SOURCE / "scripts/measure-process-tree-memory.py")
    config = WORK / "sway.conf"
    config.write_text("xwayland disable\noutput HEADLESS-1 mode 1280x720\n")
    environment = dict(os.environ, WLR_BACKENDS="headless", WLR_RENDERER="pixman",
                       WLR_HEADLESS_OUTPUTS="1")
    sway_log = WORK / "sway.log"
    qs_log = WORK / "quickshell.log"
    sway_output = sway_log.open("w")
    qs_output = None
    compositor = subprocess.Popen(["sway", "-c", str(config)], env=environment,
                                  stdout=sway_output, stderr=subprocess.STDOUT)
    quickshell = None
    sampler = None
    output = None
    samples = []
    phase = "starting"
    output_path = Path(output_path)
    boot_deadline = time.monotonic() + 45
    try:
        while time.monotonic() < boot_deadline:
            if compositor.poll() is not None:
                raise RuntimeError("private Sway exited:\n" + sway_log.read_text())
            sockets = [path for path in (WORK / "runtime").glob("wayland-*") if path.is_socket()]
            if sockets:
                environment["WAYLAND_DISPLAY"] = sockets[0].name
                break
            time.sleep(0.05)
        else:
            raise RuntimeError("private Wayland socket did not appear")

        qs_output = qs_log.open("w")
        quickshell = subprocess.Popen(["qs", "-n", "-p", str(SOURCE / "shell.qml")],
                                      env=environment, stdout=qs_output,
                                      stderr=subprocess.STDOUT)
        sampler = sampler_module.Sampler([quickshell.pid])
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output = output_path.open("x", encoding="utf-8")
        metadata = {"type": "metadata", "scenario": "production-island-runtime",
                    "started_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    "checkout": str(SOURCE), "entrypoint": str(SOURCE / "shell.qml"),
                    "root_pids": [root.as_dict() for root in sampler.roots],
                    "interval_seconds": 0.5, "max_attempts": MAX_SAMPLE_ATTEMPTS,
                    "threshold_bytes": sampler_module.THRESHOLD_BYTES,
                    "scope": sampler_module.summarize([])["scope"],
                    "excluded_processes": ["private Sway", "private D-Bus", "benchmark controller", "sampler"],
                    "renderer": "Sway headless pixman; Qt Quick software",
                    "outputs": ["HEADLESS-1 1280x720"],
                    "service_limits": ["network namespace isolated", "no Niri socket",
                                       "no live PipeWire/MPRIS/tray providers or user artwork",
                                       "NetworkManager data unavailable; private nmcli monitor stub holds the subscription open"]}
        output.write(json.dumps(metadata) + "\n")
        output.flush()

        def read_log():
            qs_output.flush()
            try:
                return qs_log.read_text(errors="replace")
            except OSError:
                return ""

        def assert_qml_log_clean():
            patterns = (
                r"ERROR qml:", r"ERROR scene: QML",
                r"WARN scene:.*(?:File not found|Syntax error|Failed to load|Cannot load|"
                r"Required property .* not initialized|Component is not ready)",
                r"QQmlComponent: Component is not ready",
                r"(?:^|\b)TypeError:", r"(?:^|\b)ReferenceError:",
                r"Type \w+ unavailable", r"\w+ is not a type",
                r"Failed to create LazyLoader component", r"Could not compile(?: QML)?",
                r"Cannot assign to non-existent property", r"Property .* not found",
                r"\[PanelLoader\] Could not compile", r"\[IslandPanel\] Could not load",
                r"Failed to load configuration")
            failures = [line for line in read_log().splitlines()
                        if any(re.search(pattern, line) for pattern in patterns)]
            if failures:
                raise RuntimeError("production QML/runtime failure:\n" + "\n".join(failures[:20]))

        def log_sample(sample):
            samples.append(sample)
            output.write(json.dumps(sample) + "\n")
            output.flush()

        def run_for(name, seconds):
            nonlocal phase
            phase = name
            deadline = time.monotonic() + seconds
            next_sample = time.monotonic()
            while time.monotonic() < deadline:
                if quickshell.poll() is not None:
                    raise RuntimeError("Quickshell exited during " + name + ":\n" + read_log())
                log_sample(sampler.sample(phase, max_attempts=MAX_SAMPLE_ATTEMPTS))
                next_sample += 0.5
                while time.monotonic() < min(next_sample, deadline):
                    time.sleep(min(0.05, max(0, min(next_sample, deadline) - time.monotonic())))
            assert_qml_log_clean()

        def ipc(*arguments):
            result = subprocess.run(["qs", "ipc", "--pid", str(quickshell.pid),
                                     "call", "iris", *arguments],
                                    env=environment, text=True, capture_output=True, timeout=5)
            if result.returncode != 0:
                raise RuntimeError("iris IPC failed: " + " ".join(arguments) + "\n"
                                   + result.stdout + result.stderr)
            return result.stdout.strip()

        def card_diagnostics():
            return json.loads(ipc("cardDiagnostics"))

        def wait_card_ready(kind, timeout=12):
            deadline = time.monotonic() + timeout
            latest = None
            phase = "card-load-" + kind
            while time.monotonic() < deadline:
                if quickshell.poll() is not None:
                    raise RuntimeError("Quickshell exited while loading card " + kind + ": " + read_log())
                log_sample(sampler.sample(phase, max_attempts=MAX_SAMPLE_ATTEMPTS))
                snapshot = card_diagnostics()
                matches = [stage for stage in snapshot.get("stages", [])
                           if stage.get("requestedKind") == kind]
                latest = matches[-1] if matches else None
                if latest:
                    payload = latest.get("payload") or {}
                    surface = latest.get("surface") or {}
                    loaders = payload.get("loaders") or []
                    if (latest.get("retainedKind") == kind
                            and latest.get("actualKind") == kind
                            and latest.get("cardOpen") and not latest.get("stageSuppressed")
                            and latest.get("contentLoader", {}).get("status") == "ready"
                            and latest.get("contentLoader", {}).get("itemPresent")
                            and payload.get("bodyStatus") == "ready"
                            and payload.get("bodyItemPresent")
                            and payload.get("contentHeight", 0) > 0
                            and loaders and all(loader.get("status") == "ready"
                                                and loader.get("itemPresent") for loader in loaders)
                            and surface.get("contentReady") and surface.get("canArm")
                            and surface.get("armed") and surface.get("settled")
                            and surface.get("width", 0) > 0 and surface.get("height", 0) > 0):
                        output.write(json.dumps({"type": "card-diagnostic", "kind": kind,
                                                "timestamp": datetime.datetime.now(
                                                    datetime.timezone.utc).isoformat(),
                                                "snapshot": latest}) + "\n")
                        output.flush()
                        return
                assert_qml_log_clean()
                time.sleep(0.25)
            raise RuntimeError("card payload/surface did not become ready: " + kind + "; latest="
                               + json.dumps(latest, sort_keys=True))

        def assert_cards_closed():
            snapshot = card_diagnostics()
            still_present = [stage for stage in snapshot.get("stages", [])
                             if stage.get("cardOpen") or stage.get("cardPresent")
                             or stage.get("surface", {}).get("armed")
                             or stage.get("surface", {}).get("progress", 0) > 0.001]
            if still_present:
                raise RuntimeError("card remained presented after close: "
                                   + json.dumps(still_present))

        # Capture cold startup immediately, then wait for the real family and
        # both boot gates before testing any surface.
        phase = "cold-boot"
        ready_needles = ("Config.ready", "shellEntryReady", "Tier 4 (background features)",
                         "[FamilyLoader] ready iris", "[PanelLoader] ready modules/iris/bar/IrisBar.qml",
                         "[PanelLoader] ready modules/iris/frame/IrisReservations.qml")
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if quickshell.poll() is not None:
                raise RuntimeError("Quickshell exited during cold boot:\n" + read_log())
            log_sample(sampler.sample(phase, max_attempts=MAX_SAMPLE_ATTEMPTS))
            log = read_log()
            assert_qml_log_clean()
            if all(needle in log for needle in ready_needles):
                break
            time.sleep(0.5)
        else:
            raise RuntimeError("iSland production readiness was not reached; missing "
                               + repr([needle for needle in ready_needles if needle not in read_log()])
                               + "\n" + read_log())

        startup_status = ipc("status")
        try:
            status = json.loads(startup_status)
        except json.JSONDecodeError as error:
            raise RuntimeError("invalid iris status response: " + startup_status) from error
        if not isinstance(status, dict):
            raise RuntimeError("iris status did not return an object")

        run_for("idle-initial", 30)
        ipc("open")
        if not json.loads(ipc("status")).get("islandExpanded"):
            raise RuntimeError("production Island did not enter its expanded state")
        run_for("island-open", 5)
        for page in ("media", "activity", "desktop", "tray", "tools"):
            # The production page handler is void; verify its observable state.
            ipc("page", page)
            page_status = json.loads(ipc("status"))
            active_screen = page_status.get("focusedScreen", "")
            if (not page_status.get("islandExpanded")
                    or page_status.get("islandPages", {}).get(active_screen) != page):
                raise RuntimeError("production Island page did not activate: " + page)
            run_for("island-page-" + page, 3)
        ipc("close")
        run_for("island-close-grace", 5)

        ipc("focus", "open")
        if not json.loads(ipc("status")).get("focus", {}).get("open"):
            raise RuntimeError("left sidebar did not enter its open state")
        run_for("sidebar-left-open", 5)
        ipc("focus", "close")
        ipc("today", "open")
        if not json.loads(ipc("status")).get("today", {}).get("open"):
            raise RuntimeError("right sidebar did not enter its open state")
        run_for("sidebar-right-open", 5)
        ipc("today", "close")
        ipc("controlCenter", "open")
        if not json.loads(ipc("status")).get("controlCenter"):
            raise RuntimeError("Control Center did not enter its open state")
        run_for("control-center-open", 8)
        ipc("controlCenter", "close")

        cards = ("weather", "notifications", "calendar", "sound", "mic", "tools", "tray",
                 "media", "visualizer", "network", "bluetooth", "vitals", "workspaces",
                 "updates", "anime", "watching", "shellUpdate", "vpn")
        for cycle in (1, 2):
            for kind in cards:
                ipc("bubbleCard", "close")
                run_for(f"card-reset-{cycle}-{kind}", 1)
                assert_cards_closed()
                ipc("open")
                if not json.loads(ipc("status")).get("islandExpanded"):
                    raise RuntimeError("production Island did not open before card " + kind)
                response = ipc("bubbleCard", kind)
                if response != kind:
                    raise RuntimeError("card unavailable in production IPC: " + kind + ": " + response)
                status_text = ipc("status")
                status = json.loads(status_text)
                if status.get("bubbleCard") != kind:
                    raise RuntimeError("production stage did not activate card " + kind + ": " + status_text)
                wait_card_ready(kind)
                run_for(f"card-cycle-{cycle}-{kind}", 2)
            ipc("bubbleCard", "close")
            ipc("close")
            run_for(f"card-cycle-{cycle}-close-grace", 3)
            assert_cards_closed()

        run_for("idle-30-second-settle", 30)
        ipc("open")
        ipc("page", "activity")
        run_for("repeat-open-activity", 5)
        ipc("close")
        run_for("idle-120-second-settle", 120)
        assert_qml_log_clean()

        summary = sampler_module.summarize(samples)
        summary.update({"scenario": "production-island-runtime", "finished_at":
                        datetime.datetime.now(datetime.timezone.utc).isoformat(),
                        "primary_root_alive_throughout": all(
                            sample["roots"][0]["alive"] for sample in samples),
                        "phase_counts": {name: sum(sample["scenario"] == name for sample in samples)
                                         for name in sorted({sample["scenario"] for sample in samples})},
                        "longest_sample_gap_seconds": max((
                            time_delta(samples[index - 1]["timestamp"], sample["timestamp"])
                            for index, sample in enumerate(samples) if index), default=0)})
        output.write(json.dumps(summary) + "\n")
        output.flush()
        if not summary["primary_root_alive_throughout"]:
            raise RuntimeError("primary Quickshell process exited during measured interval")
        if summary["incomplete_sample_count"]:
            raise RuntimeError(f"{summary['incomplete_sample_count']} incomplete samples; "
                               "see JSONL attempt diagnostics")
        if summary["threshold_exceeded"]:
            raise RuntimeError("aggregate observed PSS exceeds 1.5 GB")
        print("Aggregate Quickshell process-tree PSS: max", summary["max_pss_bytes"],
              "bytes; median", summary["median_pss_bytes"], "bytes")
        print("Samples:", summary["sample_count"], "; incomplete:",
              summary["incomplete_sample_count"])
        print("Phase counts:", json.dumps(summary["phase_counts"], sort_keys=True))
        return 0
    finally:
        if output:
            output.flush()
            output.close()
        if qs_output:
            qs_output.flush()
            qs_output.close()
        if quickshell and quickshell.poll() is None:
            quickshell.terminate()
            try:
                quickshell.wait(timeout=3)
            except subprocess.TimeoutExpired:
                quickshell.kill()
                quickshell.wait(timeout=3)
        if compositor.poll() is None:
            compositor.terminate()
            try:
                compositor.wait(timeout=3)
            except subprocess.TimeoutExpired:
                compositor.kill()
                compositor.wait(timeout=3)
        sway_output.close()


def time_delta(start, end):
    from datetime import datetime
    return (datetime.fromisoformat(end) - datetime.fromisoformat(start)).total_seconds()


def outer(duration, output_path):
    required = ("bwrap", "dbus-run-session", "qs", "sway", "timeout")
    missing = [name for name in required if not shutil.which(name)]
    if missing:
        raise RuntimeError("missing benchmark prerequisites: " + ", ".join(missing))
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="intoo island runtime ") as temporary:
        base = Path(temporary)
        for relative in ("home", "config/inir", "state/quickshell/user", "cache", "runtime", "data", "tools"):
            (base / relative).mkdir(parents=True, exist_ok=True)
        (base / "runtime").chmod(0o700)
        config = json.loads((ROOT / "defaults/config.json").read_text())
        config["panelFamily"] = "iris"
        config["enabledPanels"] = [
            "irisBackground", "irisBar", "irisOnScreenDisplay", "irisNotificationPopup",
            "irisSessionScreen", "irisLock", "irisPolkit", "irisOverlay", "irisCheatsheet",
            "irisOnScreenKeyboard", "irisRegionSelector", "irisLockRehearsal",
            "irisWallpaperLauncher", "irisCoverflowSelector", "irisRecordingOsd",
            "irisTilingOverlay"]
        # Isolated network namespaces cannot query upstream. Preserve default
        # update polling interval and behavior except disabling the remote check.
        config.setdefault("shellUpdates", {})["enabled"] = False
        (base / "config/inir/config.json").write_text(json.dumps(config))
        (base / "state/quickshell/user/first_run.txt").write_text("isolated iSland PSS benchmark\n")
        nmcli_stub = base / "tools/nmcli"
        nmcli_stub.write_text("#!/bin/sh\n"
                              "if [ \"${1-}\" = monitor ]; then exec /usr/bin/sleep infinity; fi\n"
                              "exec /usr/bin/nmcli \"$@\"\n")
        nmcli_stub.chmod(0o755)
        metadata_path = base / "benchmark-config.json"
        versions = {}
        for name, command in (("quickshell", ["qs", "--version"]),
                              ("sway", ["sway", "--version"])):
            version = subprocess.run(command, capture_output=True, text=True, timeout=5)
            versions[name] = (version.stdout + version.stderr).strip()
        metadata_path.write_text(json.dumps({
            "source_revision": subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD"],
                                               capture_output=True, text=True).stdout.strip(),
            "working_tree": "current checkout; source bind-mounted read-only",
            "panelFamily": config["panelFamily"],
            "enabledPanels": config["enabledPanels"],
            "config_source": "defaults/config.json plus panelFamily/enabledPanels and shellUpdates.enabled=false",
            "renderer": "Sway headless pixman; Qt Quick software",
            "runtime_versions": versions,
            "outputs": ["HEADLESS-1 1280x720"],
            "service_limits": ["network namespace isolated", "no Niri socket",
                               "no live PipeWire/MPRIS/tray providers or user artwork",
                               "NetworkManager data unavailable; private nmcli monitor stub holds the subscription open"]}, indent=2))
        env = {
            "HOME": "/tmp/audit-work/home",
            "XDG_CONFIG_HOME": "/tmp/audit-work/config",
            "XDG_STATE_HOME": "/tmp/audit-work/state",
            "XDG_CACHE_HOME": "/tmp/audit-work/cache",
            "XDG_DATA_HOME": "/tmp/audit-work/data",
            "XDG_RUNTIME_DIR": "/tmp/audit-work/runtime",
            "QT_QPA_PLATFORM": "wayland",
            "QT_QUICK_BACKEND": "software",
            "QS_DEBUG": "1",
            "QS_DISABLE_CRASH_HANDLER": "1",
            "INIR_DISABLE_HOT_RELOAD": "1",
            "INIR_STANDALONE_WINDOW": "1",
            "PATH": "/tmp/audit-work/tools:/usr/bin:/bin:/usr/sbin:/sbin",
        }
        command = ["bwrap", "--ro-bind", "/", "/", "--unshare-net", "--unshare-pid",
                   "--unshare-ipc", "--unshare-uts", "--die-with-parent", "--proc", "/proc",
                   "--dev", "/dev", "--tmpfs", "/tmp", "--tmpfs", "/run", "--tmpfs", "/home",
                   "--ro-bind", str(ROOT), str(SOURCE), "--bind", str(base), str(WORK),
                   "--chdir", str(WORK)]
        for name, value in env.items():
            command.extend(("--setenv", name, value))
        for name in ("DBUS_SESSION_BUS_ADDRESS", "DBUS_SYSTEM_BUS_ADDRESS", "NIRI_SOCKET",
                     "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "SWAYSOCK", "DISPLAY",
                     "HYPRLAND_INSTANCE_SIGNATURE", "QS_CONFIG_PATH", "QS_CONFIG_NAME",
                     "QS_MANIFEST", "QML_IMPORT_PATH", "QML2_IMPORT_PATH",
                     "INIR_SESSION_SHELL_PGID"):
            command.extend(("--unsetenv", name))
        command.extend(("--", "dbus-run-session", "--", "timeout", "--kill-after=5s",
                        f"{duration}s", "python3", str(SOURCE / "scripts/measure-island-runtime.py"),
                        "--sandbox", str(duration), "/tmp/audit-work/island-pss.jsonl"))
        result = subprocess.run(command, text=True, capture_output=True, timeout=duration + 30)
        log_path = Path(str(output_path) + ".runtime.log")
        log_path.write_text(result.stdout + result.stderr)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        source_jsonl = base / "island-pss.jsonl"
        if source_jsonl.exists():
            shutil.copyfile(source_jsonl, output_path)
        else:
            output_path.write_text("")
        config_output = Path(str(output_path) + ".config.json")
        config_output.write_text(metadata_path.read_text())
        for name in ("quickshell.log", "sway.log"):
            source_log = base / name
            if source_log.exists():
                shutil.copyfile(source_log, Path(str(output_path) + "." + name))
        if result.stdout:
            print(result.stdout, end="")
        if result.stderr:
            print(result.stderr, end="", file=sys.stderr)
        return result.returncode


def positive_int(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("duration must be positive")
    return number


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--duration", type=positive_int, default=480,
                        help="outer timeout for the full scenario; default 480 seconds")
    parser.add_argument("--output", type=Path, default=Path("/tmp/intoo-island-runtime.jsonl"),
                        help="new JSONL output path; runtime log/config are written beside it")
    parser.add_argument("--sandbox", nargs=2, metavar=("SECONDS", "JSONL"))
    args = parser.parse_args()
    if args.sandbox:
        try:
            return inner(int(args.sandbox[0]), args.sandbox[1])
        except Exception as error:
            print("ISLAND_RUNTIME_BENCHMARK_FAILED:", error, file=sys.stderr)
            return 1
    if args.output.exists():
        parser.error("output file already exists; choose a new path")
    try:
        return outer(args.duration, args.output)
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    raise SystemExit(main())
