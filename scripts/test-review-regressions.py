#!/usr/bin/env python3
"""Regressions for previous reviews and docs/SENIOR_REVIEW.ru.md.

All filesystem writes and Git operations use temporary fixtures. Service
commands are stubbed; niri validate checks fixture syntax when installed.
"""

import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("audit_fixtures", ROOT / "scripts/test-audit-regressions.py")
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)
snapshot_spec = importlib.util.spec_from_file_location("review_snapshot", ROOT / "sdata/lib/niri-snapshot.py")
snapshot_module = importlib.util.module_from_spec(snapshot_spec)
snapshot_spec.loader.exec_module(snapshot_module)
startup_spec = importlib.util.spec_from_file_location("review_shell_startup", ROOT / "scripts/test-shell-startup.py")
startup_module = importlib.util.module_from_spec(startup_spec)
startup_spec.loader.exec_module(startup_module)


class ReviewRegressions(unittest.TestCase):
    setUp = audit.AuditRegressions.setUp
    stub = audit.AuditRegressions.stub
    bash = audit.AuditRegressions.bash
    validate_niri = audit.AuditRegressions.validate_niri
    snapshot_fixture = audit.AuditRegressions.snapshot_fixture
    snapshot_prelude = audit.AuditRegressions.snapshot_prelude

    def test_sr01_quick_uninstall_stops_when_backup_fails(self):
        payload = self.config / "inir"
        payload.mkdir(parents=True)
        (payload / "config.json").write_text("user config\n")
        result = self.bash('source "$1/sdata/lib/uninstall.sh"; tui_title(){ :; }; tui_warn(){ :; }; tui_error(){ :; }; tui_confirm(){ return 0; }; uninstall_create_backup(){ return 1; }; uninstall_stop_services(){ touch "$HOME/stop-services"; }; ask=false; run_uninstall_quick; rc=$?; [[ "$rc" -ne 0 && -f "$INIR_CONFIG_DIR/config.json" && ! -e "$HOME/stop-services" ]]')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_pr01_cleanup_does_not_split_paths_or_delete_neighbor_data(self):
        backup_root = self.base / "state victim"
        backup_root.mkdir()
        for index in range(6):
            backup = backup_root / f"{index:03}"
            backup.mkdir()
            os.utime(backup, (100 + index, 100 + index))
        working = self.base / "work"
        unrelated = working / "victim/000/valuable.txt"
        unrelated.parent.mkdir(parents=True)
        unrelated.write_text("unrelated data\n")

        result = self.bash('''source "$1/sdata/lib/user-modifications.sh"
USER_MODS_DIR="$2"
MAX_USER_MODS=5
cd "$3" || exit 10
cleanup_old_user_mods
''', backup_root, working)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(unrelated.exists(), "cleanup must not delete a path outside USER_MODS_DIR")
        self.assertFalse((backup_root / "000").exists(), "the oldest backup should be pruned")
        self.assertEqual(len(list(backup_root.iterdir())), 5)

    def test_pr03_failed_user_mod_copy_aborts_and_discards_partial_backup(self):
        source = self.base / "runtime-source"
        source.mkdir()
        (source / "shell.qml").write_text("// changed user runtime\n")
        backup_root = self.base / "mod-backups"
        result = self.bash('''source "$1/sdata/lib/user-modifications.sh"
USER_MODS_DIR="$2"
REPO_ROOT="$1"
ask=false
tui_warn() { :; }; tui_subtitle() { :; }; tui_dim() { :; }; tui_info() { :; }
tui_success() { printf 'SUCCESS %s\\\\n' "$*"; }
tui_error() { printf 'ERROR %s\\\\n' "$*"; }
cp() { return 28; }
if handle_user_modifications shell.qml "" "$3"; then exit 11; fi
[[ -z "$(find "$USER_MODS_DIR" -mindepth 1 -maxdepth 1 -type d -print -quit)" ]]
''', backup_root, source)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("ERROR Could not preserve local modifications", result.stdout)
        self.assertNotIn("SUCCESS Auto-preserved", result.stdout)

    def test_pr03_failed_full_runtime_rsync_does_not_publish_backup(self):
        source = self.base / "runtime-tree"
        source.mkdir()
        (source / "shell.qml").write_text("// legacy shell\n")
        backup_root = self.base / "full-runtime-backups"
        result = self.bash('''source "$1/sdata/lib/user-modifications.sh"
USER_MODS_DIR="$2"
REPO_ROOT="$1"
tui_error() { :; }
log_warning() { :; }
rsync() { return 23; }
if preserve_runtime_tree "$3" legacy-runtime; then exit 11; fi
[[ -z "$(find "$USER_MODS_DIR" -mindepth 1 -maxdepth 1 -type d -name '[0-9]*' -print -quit)" ]]
''', backup_root, source)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_pr03_successful_backup_modes_publish_complete_json_metadata(self):
        source = self.base / 'runtime "quoted"'
        source.mkdir()
        (source / "shell.qml").write_text("// complete runtime\n")
        backup_root = self.base / "complete-backups"
        result = self.bash('''source "$1/sdata/lib/user-modifications.sh"
USER_MODS_DIR="$2"
REPO_ROOT="$1"
log_warning() { printf '%s\\\\n' "$*" >&2; }
partial=$(preserve_user_modifications "$3" shell.qml "")
full=$(preserve_runtime_tree "$3" 'runtime "quoted"')
''', backup_root, source)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        backups = list(backup_root.iterdir())
        self.assertEqual(len(backups), 2)
        partial = next(path for path in backups if not json.loads((path / "metadata.json").read_text()).get("full_runtime_copy"))
        full = next(path for path in backups if json.loads((path / "metadata.json").read_text()).get("full_runtime_copy"))
        partial_meta = json.loads((partial / "metadata.json").read_text())
        full_meta = json.loads((full / "metadata.json").read_text())
        self.assertEqual((partial / "shell.qml").read_text(), "// complete runtime\n")
        self.assertEqual(partial_meta["modified_count"], 1)
        self.assertEqual((full / full_meta["dir_name"] / "shell.qml").read_text(), "// complete runtime\n")
        self.assertTrue(full_meta["full_runtime_copy"])
        self.assertEqual(full_meta["modified_count"], 1)

    def test_sr02_migration_does_not_run_without_a_complete_backup(self):
        migration_dir = self.base / "migrations"
        migration_dir.mkdir()
        target = self.config / "inir/config.json"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("original\n")
        (migration_dir / "900-audit-backup-failure.sh").write_text('MIGRATION_ID="900-audit-backup-failure"\nMIGRATION_TITLE="backup failure fixture"\nMIGRATION_TARGET_FILE="$XDG_CONFIG_HOME/inir/config.json"\nMIGRATION_REQUIRED=true\nmigration_apply() { printf partial > "$MIGRATION_TARGET_FILE"; touch "$HOME/migration-ran"; }\n')
        result = self.bash('source "$1/sdata/lib/environment-variables.sh"; source "$1/sdata/lib/migrations.sh"; MIGRATIONS_DIR="$2"; MIGRATIONS_BACKUP_DIR="$HOME/backups"; tui_check_fail(){ :; }; tui_check_skip(){ :; }; tui_check_ok(){ :; }; tui_dim(){ :; }; cp(){ return 28; }; apply_migration 900-audit-backup-failure true; rc=$?; [[ "$rc" -ne 0 && ! -e "$HOME/migration-ran" && "$(cat "$XDG_CONFIG_HOME/inir/config.json")" == original ]]', migration_dir)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_sr02_failed_migration_keeps_recovery_copy_when_rollback_fails(self):
        migration_dir = self.base / "migration-rollback"
        migration_dir.mkdir()
        target = self.config / "inir/config.json"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("original\n")
        (migration_dir / "901-audit-rollback-failure.sh").write_text('MIGRATION_ID="901-audit-rollback-failure"\nMIGRATION_TITLE="rollback failure fixture"\nMIGRATION_TARGET_FILE="$XDG_CONFIG_HOME/inir/config.json"\nMIGRATION_REQUIRED=true\nmigration_apply() { printf partial > "$MIGRATION_TARGET_FILE"; return 1; }\n')
        result = self.bash('source "$1/sdata/lib/environment-variables.sh"; source "$1/sdata/lib/migrations.sh"; MIGRATIONS_DIR="$2"; MIGRATIONS_BACKUP_DIR="$HOME/backups"; tui_check_fail(){ printf "%s\\n" "$1"; }; tui_info(){ :; }; tui_dim(){ :; }; cp(){ local n=0; [[ ! -f "$HOME/cp-count" ]] || n=$(cat "$HOME/cp-count"); n=$((n+1)); printf "%s" "$n" > "$HOME/cp-count"; [[ "$n" -eq 2 ]] && return 28; /bin/cp "$@"; }; apply_migration 901-audit-rollback-failure true; rc=$?; backup=$(find "$HOME/backups" -mindepth 2 -maxdepth 2 -type f -name config.json -print -quit); [[ "$rc" -ne 0 && -n "$backup" && "$(cat "$backup")" == original && "$(cat "$XDG_CONFIG_HOME/inir/config.json")" == partial ]]', migration_dir)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("recovery copy kept", result.stdout)

    def test_sr02_failed_migration_restores_symlink_target_and_topology(self):
        migration_dir = self.base / "migration-symlink-rollback"
        migration_dir.mkdir()
        target = self.config / "inir/config.json"
        target.parent.mkdir(parents=True, exist_ok=True)
        external = self.base / "external-config.json"
        external.write_text("original external config\n")
        target.symlink_to(external)
        (migration_dir / "902-audit-symlink-rollback.sh").write_text(
            'MIGRATION_ID="902-audit-symlink-rollback"\n'
            'MIGRATION_TITLE="symlink rollback fixture"\n'
            'MIGRATION_TARGET_FILE="$XDG_CONFIG_HOME/inir/config.json"\n'
            'MIGRATION_REQUIRED=true\n'
            'migration_apply() { printf partial > "$MIGRATION_TARGET_FILE"; return 1; }\n'
        )
        result = self.bash('source "$1/sdata/lib/environment-variables.sh"; source "$1/sdata/lib/migrations.sh"; MIGRATIONS_DIR="$2"; MIGRATIONS_BACKUP_DIR="$HOME/backups"; tui_check_fail(){ :; }; tui_info(){ :; }; tui_dim(){ :; }; apply_migration 902-audit-symlink-rollback true; rc=$?; [[ "$rc" -ne 0 && -L "$XDG_CONFIG_HOME/inir/config.json" && "$(readlink "$XDG_CONFIG_HOME/inir/config.json")" == "$3" && "$(cat "$3")" == "original external config" ]]', migration_dir, external)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


    def test_sr03_profile_cleanup_preserves_non_utf8_bytes(self):
        profile = self.base / ".bashrc"
        profile.write_bytes(b"# user comment \xff\n# iNtoo environment\nexport PATH=/old\n# end iNtoo\nexport IMPORTANT=yes\n")
        result = self.bash('source "$1/sdata/lib/uninstall.sh"; tui_success(){ :; }; uninstall_remove_shell_integration')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(profile.read_bytes(), b"# user comment \xff\nexport IMPORTANT=yes\n")

    def test_sr03_failed_profile_transform_leaves_original_bytes_intact(self):
        profile = self.base / ".bashrc"
        original = b"# iNtoo environment\nexport PATH=/old\n# end iNtoo\n\xff\n"
        profile.write_bytes(original)
        result = self.bash('source "$1/sdata/lib/uninstall.sh"; python3(){ return 1; }; tui_success(){ :; }; uninstall_remove_shell_integration >/dev/null 2>&1; rc=$?; [[ "$rc" -ne 0 ]]')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(profile.read_bytes(), original)

    def test_sr08_full_runtime_backup_restores_payload_root_for_both_modes(self):
        setup = (ROOT / "setup").read_text()

        def shell_function(name):
            match = re.search(rf"^{name}\(\) \{{\n.*?^\}}", setup, re.M | re.S)
            self.assertIsNotNone(match, f"missing {name} in setup")
            return match.group()

        backup = self.base / "backup"
        snapshot = backup / "legacy-runtime"
        snapshot.mkdir(parents=True)
        (backup / "metadata.json").write_text(json.dumps({
            "full_runtime_copy": True,
            "label": "legacy-runtime",
        }))
        (snapshot / "shell.qml").write_text("// preserved runtime\n")

        for mode in ("all", "single"):
            with self.subTest(mode=mode):
                target = self.base / f"runtime-{mode}"
                target.mkdir()
                (target / "shell.qml").write_text("// shipped runtime\n")
                function_name = "restore_all_user_mods" if mode == "all" else "restore_single_user_mod"
                code = '''source "$1/sdata/lib/user-modifications.sh"
RUNTIME_TARGET="$3"
get_runtime_shell_dir() { printf '%s\\n' "$RUNTIME_TARGET"; }
tui_dim() { :; }
tui_success() { :; }
tui_info() { :; }
tui_choose() { printf shell.qml; }
''' + shell_function("user_mods_restore_copy") + "\n" + shell_function(function_name) + "\n" + function_name + ' "$2"\n'
                result = self.bash(code, backup, target)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual((target / "shell.qml").read_text(), "// preserved runtime\n")
                self.assertFalse((target / "legacy-runtime").exists())

    def test_sr08_restore_all_and_single_report_copy_failures(self):
        setup = (ROOT / "setup").read_text()

        def shell_function(name):
            match = re.search(rf"^{name}\(\) \{{\n.*?^\}}", setup, re.M | re.S)
            self.assertIsNotNone(match, f"missing {name} in setup")
            return match.group()

        backup = self.base / "backup-copy-failure"
        snapshot = backup / "legacy-runtime"
        snapshot.mkdir(parents=True)
        (backup / "metadata.json").write_text(json.dumps({"full_runtime_copy": True, "label": "legacy-runtime"}))
        (snapshot / "shell.qml").write_text("// preserved runtime\n")

        for mode in ("all", "single"):
            with self.subTest(mode=mode):
                target = self.base / f"runtime-failed-{mode}"
                target.mkdir()
                (target / "shell.qml").write_text("// shipped runtime\n")
                function_name = "restore_all_user_mods" if mode == "all" else "restore_single_user_mod"
                code = '''source "$1/sdata/lib/user-modifications.sh"
RUNTIME_TARGET="$3"
get_runtime_shell_dir() { printf '%s\\n' "$RUNTIME_TARGET"; }
tui_dim() { :; }
tui_success() { printf 'SUCCESS: %s\\n' "$*"; }
tui_info() { :; }
tui_error() { printf 'ERROR: %s\\n' "$*"; }
tui_choose() { printf shell.qml; }
cp() { return 28; }
''' + shell_function("user_mods_restore_copy") + "\n" + shell_function(function_name) + "\n" + function_name + ' "$2"\n'
                result = self.bash(code, backup, target)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual((target / "shell.qml").read_text(), "// shipped runtime\n")
                self.assertIn("ERROR:", result.stdout)
                self.assertNotIn("SUCCESS:", result.stdout)

    def test_sr08_restore_keeps_destination_symlink(self):
        setup = (ROOT / "setup").read_text()
        helper = re.search(r"^user_mods_restore_copy\(\) \{\n.*?^\}", setup, re.M | re.S)
        restore_all = re.search(r"^restore_all_user_mods\(\) \{\n.*?^\}", setup, re.M | re.S)
        self.assertIsNotNone(helper)
        self.assertIsNotNone(restore_all)

        backup = self.base / "backup-symlink"
        snapshot = backup / "legacy-runtime"
        snapshot.mkdir(parents=True)
        (backup / "metadata.json").write_text(json.dumps({"full_runtime_copy": True, "label": "legacy-runtime"}))
        (snapshot / "shell.qml").write_text("// restored runtime\n")
        target = self.base / "runtime-symlink"
        target.mkdir()
        external = self.base / "external-shell.qml"
        external.write_text("// external runtime\n")
        (target / "shell.qml").symlink_to(external)
        code = '''source "$1/sdata/lib/user-modifications.sh"
RUNTIME_TARGET="$3"
get_runtime_shell_dir() { printf '%s\\n' "$RUNTIME_TARGET"; }
tui_dim() { :; }
tui_success() { :; }
tui_info() { :; }
tui_error() { :; }
''' + helper.group() + "\n" + restore_all.group() + '\nrestore_all_user_mods "$2"\n'
        result = self.bash(code, backup, target)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((target / "shell.qml").is_symlink())
        self.assertEqual(external.read_text(), "// restored runtime\n")

    def test_release_gate_requires_native_startup_prerequisites(self):
        with patch.dict(startup_module.os.environ, {"INIR_REQUIRE_NATIVE_STARTUP": "1"}), \
                patch.object(startup_module.shutil, "which", return_value=None):
            with self.assertRaisesRegex(RuntimeError, "native startup prerequisites unavailable"):
                startup_module.ShellStartupTests.setUpClass()

    @unittest.skipUnless(shutil.which("node"), "AppCatalog action probe requires Node.js")
    def test_pr04_gentoo_flatpak_app_removes_from_its_install_source(self):
        source = (ROOT / "services/AppCatalog.qml").read_text()
        remove = re.search(r"^    function removeApp\(.*?\n    \}", source, re.M | re.S)
        self.assertIsNotNone(remove)
        function = re.sub(r"\(appId: string\): bool \{", "(appId) {", remove.group())
        steam = next(app for app in json.loads((ROOT / "defaults/app-catalog.json").read_text())
                     if app["id"] == "steam")
        script = """
const executed = [];
const _refreshTimer = { restart() {} };
const root = {
  _detectedPm: "gentoo",
  _flatpakAvailable: true,
  _installedSources: { steam: "flatpak" },
  catalog: [APP],
  _runTerminalScript: (command, args) => executed.push({ command, args })
};
FUNCTION
console.log(JSON.stringify({ result: removeApp("steam"), executed }));
""".replace("APP", json.dumps(steam)).replace("FUNCTION", function)
        result = subprocess.run(["node", "-e", script], text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        action = json.loads(result.stdout)
        self.assertTrue(action["result"])
        self.assertEqual(action["executed"], [{
            "command": 'flatpak uninstall -y "$1"',
            "args": ["com.valvesoftware.Steam"],
        }])
        self.assertIn('sources[app.id] = "flatpak"', source)
        software_view = (ROOT / "modules/sidebarLeft/SoftwareView.qml").read_text()
        self.assertIn('if (!AppCatalog.removeApp(card.app?.id ?? ""))', software_view)
        self.assertIn('Translation.tr("Unable to determine how to remove this app")', software_view)

    def test_sr04_sr06_gentoo_audio_and_screencopy_are_explicit(self):
        setup_services = (ROOT / "sdata/subcmd-install/2.setups.sh").read_text()
        gentoo = (ROOT / "sdata/dist-gentoo/install-deps.sh").read_text()
        doctor = (ROOT / "sdata/lib/doctor.sh").read_text()
        for unit in ("pipewire.socket", "pipewire-pulse.socket", "wireplumber.service"):
            self.assertIn(unit, setup_services)
        self.assertIn("screencopy'", gentoo)
        self.assertIn("ScreencopyView {}", doctor)
        self.assertIn("Component { WlSessionLock {} }", doctor)
        self.assertIn("Component { WlSessionLockSurface {} }", doctor)

    def test_sr04_audio_units_enable_and_honor_a_user_mask(self):
        self.stub("pipewire", "#!/usr/bin/env bash\nexit 0\n")
        self.stub("pactl", "#!/usr/bin/env bash\necho 'Server Name: PulseAudio (on PipeWire 1.6)'\n")
        self.calls.unlink(missing_ok=True)
        result = self.bash('''REPO_ROOT="$1"
tui_title() { :; }
v() { "$@"; }
log_warning() { :; }
log_info() { :; }
source "$1/sdata/subcmd-install/2.setups.sh"
OS_GROUP_ID=gentoo
DBUS_SESSION_BUS_ADDRESS=audit-bus
setup_systemd_audio_services
''')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        enabled = [line for line in self.calls.read_text().splitlines() if "enable --now" in line]
        self.assertEqual(len(enabled), 3, enabled)

        mask = self.config / "systemd/user/pipewire.socket"
        mask.parent.mkdir(parents=True, exist_ok=True)
        mask.symlink_to("/dev/null")
        self.calls.unlink(missing_ok=True)
        result = self.bash('''REPO_ROOT="$1"
tui_title() { :; }
v() { "$@"; }
log_warning() { :; }
log_info() { :; }
source "$1/sdata/subcmd-install/2.setups.sh"
OS_GROUP_ID=gentoo
DBUS_SESSION_BUS_ADDRESS=audit-bus
setup_systemd_audio_services
''')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        enabled = [line for line in self.calls.read_text().splitlines() if "enable --now" in line]
        self.assertEqual(len(enabled), 2, enabled)
        self.assertFalse(any("pipewire.socket" in line for line in enabled))

        mask.unlink()
        runtime_mask = self.runtime / "systemd/user/pipewire.socket"
        runtime_mask.parent.mkdir(parents=True)
        runtime_mask.symlink_to("/dev/null")
        self.calls.unlink(missing_ok=True)
        result = self.bash('''REPO_ROOT="$1"
tui_title() { :; }
v() { "$@"; }
log_warning() { :; }
log_info() { :; }
source "$1/sdata/subcmd-install/2.setups.sh"
OS_GROUP_ID=gentoo
DBUS_SESSION_BUS_ADDRESS=audit-bus
setup_systemd_audio_services
''')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        enabled = [line for line in self.calls.read_text().splitlines() if "enable --now" in line]
        self.assertEqual(len(enabled), 2, enabled)
        self.assertFalse(any("pipewire.socket" in line for line in enabled))

        runtime_mask.unlink()
        self.stub("systemctl", '''#!/usr/bin/env bash
printf "%s %s\\n" "${0##*/}" "$*" >> "$INIR_AUDIT_CALLS"
case "$*" in *"is-enabled pipewire.socket"*) echo masked-runtime; exit 1 ;; esac
''')
        self.calls.unlink(missing_ok=True)
        result = self.bash('''REPO_ROOT="$1"
tui_title() { :; }
v() { "$@"; }
log_warning() { :; }
log_info() { :; }
source "$1/sdata/subcmd-install/2.setups.sh"
OS_GROUP_ID=gentoo
DBUS_SESSION_BUS_ADDRESS=audit-bus
setup_systemd_audio_services
''')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        enabled = [line for line in self.calls.read_text().splitlines() if "enable --now" in line]
        self.assertEqual(len(enabled), 2, enabled)
        self.assertFalse(any("pipewire.socket" in line for line in enabled))

    def test_sr05_update_propagates_required_runtime_failures(self):
        setup = (ROOT / "setup").read_text()
        self.assertRegex(setup, r"if ! install-python-packages; then\s+_report_update_failure 36")
        self.assertRegex(setup, r"if ! ensure-ytmusic-js-runtime; then\s+_report_update_failure 37")
        doctor = (ROOT / "sdata/lib/doctor.sh").read_text()
        self.assertIn("if source ./sdata/subcmd-install/1.deps-router.sh; then", doctor)

    def test_pr02_failed_shell_restart_marks_update_failed_and_returns_error(self):
        setup = (ROOT / "setup").read_text()
        writer = re.search(r"^_write_update_status\(\) \{\n.*?^\}", setup, re.M | re.S)
        reporter = re.search(r"^_report_update_failure\(\) \{\n.*?^\}", setup, re.M | re.S)
        self.assertIsNotNone(writer)
        self.assertIsNotNone(reporter)
        start = setup.index("    # Restart shell (only if we have access to the session)")
        end = setup.index("    show_session_impact_notices", start)
        restart_flow = setup[start:end]
        code = writer.group() + "\n" + reporter.group() + '''
_update_status_file="$XDG_STATE_HOME/quickshell/user/update-status"
NIRI_SOCKET=fixture
WAYLAND_DISPLAY=
_step_phase_start() { :; }
_step_phase_header() { :; }
_step_phase_done() { :; }
tui_warn() { :; }
tui_error() { :; }
inir_user_service_is_masked() { return 1; }
inir_uses_systemd() { return 1; }
systemctl() { return 0; }
restart_shell_and_verify() { return 1; }
show_session_impact_notices() { :; }
show_update_completion() { touch "$HOME/update-completed"; }
probe_update_restart() {
''' + restart_flow + '''
}
if probe_update_restart; then exit 11; fi
[[ "$(cat "$_update_status_file")" == "failed:43:The updated shell failed to load" ]]
[[ ! -e "$HOME/update-completed" ]]
'''
        result = self.bash(code)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_sr07_sr09_portage_actions_and_release_gate_are_wired(self):
        package_search = (ROOT / "services/deferred/PackageSearch.qml").read_text()
        app_catalog = (ROOT / "services/AppCatalog.qml").read_text()
        catalog = json.loads((ROOT / "defaults/app-catalog.json").read_text())
        self.assertIn(r"(?:\/[A-Za-z0-9@._+-]+", package_search)
        self.assertIn("emerge --ask --verbose --update --deep --newuse @world", package_search)
        self.assertIn('if (pm === "gentoo" && targets.gentoo)', app_catalog)
        self.assertTrue(any(app.get("targets", {}).get("gentoo") == "www-client/firefox" for app in catalog))
        makefile = (ROOT / "Makefile").read_text()
        release = (ROOT / "scripts/release.sh").read_text()
        self.assertIn("scripts/test-review-regressions.py", makefile)
        self.assertIn('INIR_REQUIRE_NATIVE_STARTUP=1 make test-audit >/tmp/inir-release-audit.log', release)

        entries = {app["id"]: app for app in catalog}
        self.assertEqual(entries["qt6ct"]["targets"]["gentoo"], "gui-apps/qt6ct")
        self.assertEqual(entries["kvantum"]["targets"]["gentoo"], "x11-themes/kvantum")
        self.assertNotIn("gentoo", entries["deno"]["targets"])
        self.assertNotIn("gentoo", entries["steam"]["targets"])
        self.assertIn("/^[0-9]/.test(pkg.slice(targets.gentoo.length + 1))", app_catalog)

    def test_r01_single_legacy_migration_keeps_config_visible_to_cli(self):
        legacy = self.config / "illogical-impulse"
        legacy.mkdir()
        original = '{"panelFamily":"iris"}\n'
        (legacy / "config.json").write_text(original)
        result = self.bash('''unset DOTS_CORE_CONFDIR
source "$1/sdata/lib/environment-variables.sh"
source "$1/sdata/lib/migrations.sh"
MIGRATIONS_DIR="$1/sdata/migrations"
tui_check_ok() { :; }
tui_dim() { :; }
apply_migration 019-config-dir-rename-compat true || exit 1
cat "$(resolve_inir_config_dir)/config.json"
''')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout, original)
        self.assertTrue(legacy.is_symlink(), "legacy path must remain a compatibility alias")

    def test_r01_followup_repairs_install_where_019_was_already_applied(self):
        legacy = self.config / "illogical-impulse"
        canonical = self.config / "inir"
        legacy.mkdir()
        canonical.mkdir()
        original = '{"panelFamily":"iris"}\n'
        (canonical / "config.json").write_text(original)
        (legacy / "migrations.json").write_text(
            '{"applied":["019-config-dir-rename-compat"],"skipped":[]}\n')
        result = self.bash('''unset DOTS_CORE_CONFDIR
source "$1/sdata/lib/environment-variables.sh"
source "$1/sdata/lib/migrations.sh"
MIGRATIONS_DIR="$1/sdata/migrations"
tui_check_ok() { :; }
tui_dim() { :; }
apply_migration 044-repair-config-dir-compat true || exit 1
apply_migration 044-repair-config-dir-compat true || exit 1
cat "$(resolve_inir_config_dir)/config.json"
''')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout, original)
        self.assertTrue(legacy.is_symlink(), "follow-up migration did not restore the alias")
        self.assertTrue(list(self.config.glob("illogical-impulse.pre-inir-*")),
                        "follow-up migration did not retain the former state directory")

    def test_r01_failed_legacy_link_restores_the_original_directory(self):
        legacy = self.config / "illogical-impulse"
        legacy.mkdir()
        original = '{"panelFamily":"iris"}\n'
        (legacy / "config.json").write_text(original)
        result = self.bash('''ln() { return 1; }
source "$1/sdata/migrations/019-config-dir-rename-compat.sh"
migration_apply
''')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue((legacy / "config.json").is_file())
        self.assertEqual((legacy / "config.json").read_text(), original)
        self.assertFalse((self.config / "inir").exists())

    def test_r02_startup_enable_is_idempotent_for_continued_nodes(self):
        root = self.niri / "config.kdl"
        root.write_text('spawn-at-startup r"inir" \\\n    "run" "--session"\n')
        audit.startup.sync(self.config, self.bin / "inir", "enable")
        first = root.read_text()
        audit.startup.sync(self.config, self.bin / "inir", "enable")
        self.assertEqual(root.read_text(), first)
        self.assertEqual(sum(audit.startup.SHELL.match(node)
                             for node in audit.startup._active_nodes(root.read_text())), 1)

    def test_r02_active_includes_accept_raw_strings_and_continuations(self):
        fragment = self.niri / "50-startup.kdl"
        fragment.write_text('spawn-at-startup "inir" "run" "--session"\n')
        root = self.niri / "config.kdl"
        for text in ('include r"50-startup.kdl"\n', 'include \\\n    "50-startup.kdl"\n'):
            with self.subTest(config=text):
                root.write_text(text)
                self.validate_niri(root)
                self.assertIn(fragment.resolve(), audit.startup.config_files(root))

    def test_r02_startup_status_accepts_valid_kdl_command_forms(self):
        root = self.niri / "config.kdl"
        for text in ('spawn-at-startup r"inir" "run" "--session"\n',
                     'spawn-at-startup "inir" \\\n    "run" "--session"\n'):
            with self.subTest(config=text):
                root.write_text(text)
                self.validate_niri(root)
                self.assertTrue(audit.startup.sync(self.config, self.bin / "inir", "status"),
                                "active supervised startup was not detected")

    def test_r02_disable_removes_a_continued_startup_command(self):
        root = self.niri / "config.kdl"
        root.write_text('spawn-at-startup "inir" \\\n    "run" "--session"\n')
        self.validate_niri(root)
        audit.startup.sync(self.config, self.bin / "inir", "disable")
        self.validate_niri(root)
        self.assertNotIn('"inir"', root.read_text(), "disable left the active command intact")

    def check_external_include_restore(self, symlink):
        self.snapshot_fixture()
        external = self.base / "external-startup.kdl"
        original = 'spawn-at-startup "echo" "original"\n'
        external.write_text(original)
        if symlink:
            link = self.niri / "external-startup.kdl"
            link.symlink_to(external)
            include = link.name
        else:
            include = str(external)
        root = self.niri / "config.kdl"
        root.write_text(f'include "{include}"\n')
        self.validate_niri(root)
        result = self.bash(self.snapshot_prelude() + '''snapshot=$(create_snapshot audit) || exit 1
printf 'spawn-at-startup "echo" "changed"\\n' > "$2"
restore_snapshot "$snapshot"
''', external)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.validate_niri(root)
        self.assertEqual(external.read_text(), original,
                         "rollback reported success but an active include kept new content")

    def test_r03_rollback_restores_absolute_external_include(self):
        self.check_external_include_restore(False)

    def test_r03_rollback_restores_symlinked_external_include(self):
        self.check_external_include_restore(True)

    def test_r03_snapshot_handles_include_cycles(self):
        self.snapshot_fixture()
        root = self.niri / "config.kdl"
        child = self.niri / "config.d/50-startup.kdl"
        root.write_text('include "config.d/50-startup.kdl"\n')
        child.write_text('include "../config.kdl"\n')
        result = self.bash(self.snapshot_prelude() + 'create_snapshot audit\n')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_r03_snapshot_refuses_non_file_active_include(self):
        self.snapshot_fixture()
        bad_include = self.niri / "not-a-file"
        bad_include.mkdir()
        (self.niri / "config.kdl").write_text('include "not-a-file"\n')
        result = self.bash(self.snapshot_prelude() + 'create_snapshot audit\n')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(list((self.base / "state/quickshell/snapshots").glob("*/snapshot.json")))

    def test_r03_old_snapshot_with_external_include_refuses_partial_restore(self):
        self.snapshot_fixture()
        external = self.base / "external-startup.kdl"
        external.write_text('spawn-at-startup "echo" "original"\n')
        (self.niri / "config.kdl").write_text(f'include "{external}"\n')
        result = self.bash(self.snapshot_prelude() + '''snapshot=$(create_snapshot audit) || exit 1
rm -f "$XDG_STATE_HOME/quickshell/snapshots/$snapshot/niri-include-manifest.json"
printf 'spawn-at-startup "echo" "changed"\\n' > "$2"
restore_snapshot "$snapshot"
''', external)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(external.read_text(), 'spawn-at-startup "echo" "changed"\n')

    @unittest.skipUnless(shutil.which("git"), "realignment probe requires git")
    def test_r04_realign_preserves_ignored_user_data(self):
        real_git = shutil.which("git")
        env = dict(self.env, INIR_AUDIT_REAL_GIT=real_git,
                   PATH=f"{Path(real_git).parent}:{self.env['PATH']}")
        result = self.bash(self.snapshot_prelude() + '''git() { "$INIR_AUDIT_REAL_GIT" "$@"; }
git -C "$REPO_ROOT" init -q -b main || exit 1
git -C "$REPO_ROOT" config user.name 'Review fixture'
git -C "$REPO_ROOT" config user.email review@example.invalid
printf '/user-data\\n' > "$REPO_ROOT/.gitignore"
git -C "$REPO_ROOT" add .gitignore scripts/inir
git -C "$REPO_ROOT" commit -qm old || exit 1
old=$(git -C "$REPO_ROOT" rev-parse HEAD)
git -C "$REPO_ROOT" update-ref --create-reflog refs/remotes/origin/main "$old"
git -C "$REPO_ROOT" checkout -q --orphan rewrite
printf 'upstream data\\n' > "$REPO_ROOT/user-data"
git -C "$REPO_ROOT" add -f user-data
git -C "$REPO_ROOT" commit -qm rewrite || exit 1
new=$(git -C "$REPO_ROOT" rev-parse HEAD)
git -C "$REPO_ROOT" checkout -q main
printf 'private local data\\n' > "$REPO_ROOT/user-data"
git -C "$REPO_ROOT" update-ref -m forced-update refs/remotes/origin/main "$new"
is_upstream_rewrite_divergence main || exit 9
printf 'eligible-rewrite\n'
realign_repo_to_remote main
''', env=env)
        self.assertIn("eligible-rewrite", result.stdout,
                      "fixture failed upstream rewrite eligibility: " + result.stderr)
        self.assertEqual((self.repo / "user-data").read_text(), "private local data\n",
                         "automatic rewrite recovery destroyed ignored user data")
        self.assertNotEqual(result.returncode, 0, "colliding realignment must be refused")

    @unittest.skipUnless(shutil.which("git"), "realignment probe requires git")
    def test_r04_realign_refuses_ignored_path_collision_and_reports_update_status(self):
        real_git = shutil.which("git")
        env = dict(self.env, INIR_AUDIT_REAL_GIT=real_git,
                   PATH=f"{Path(real_git).parent}:{self.env['PATH']}")
        result = self.bash(self.snapshot_prelude() + '''git() { "$INIR_AUDIT_REAL_GIT" "$@"; }
git -C "$REPO_ROOT" init -q -b main || exit 1
git -C "$REPO_ROOT" config user.name 'Review fixture'
git -C "$REPO_ROOT" config user.email review@example.invalid
printf '/ignored-dir/\\n' > "$REPO_ROOT/.gitignore"
git -C "$REPO_ROOT" add .gitignore scripts/inir
git -C "$REPO_ROOT" commit -qm old || exit 1
old=$(git -C "$REPO_ROOT" rev-parse HEAD)
git -C "$REPO_ROOT" update-ref --create-reflog refs/remotes/origin/main "$old"
git -C "$REPO_ROOT" checkout -q --orphan rewrite
mkdir -p "$REPO_ROOT/ignored-dir"
printf 'upstream tracked file\\n' > "$REPO_ROOT/ignored-dir/item"
git -C "$REPO_ROOT" add -f ignored-dir/item
git -C "$REPO_ROOT" commit -qm rewrite || exit 1
new=$(git -C "$REPO_ROOT" rev-parse HEAD)
git -C "$REPO_ROOT" checkout -q main
mkdir -p "$REPO_ROOT/ignored-dir"
printf 'private data\\n' > "$REPO_ROOT/ignored-dir/item"
git -C "$REPO_ROOT" update-ref -m forced-update refs/remotes/origin/main "$new"
realign_repo_to_remote main
''', env=env)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.repo / "ignored-dir/item").read_text(), "private data\n")

        setup = (ROOT / "setup").read_text()
        branch = re.search(r'elif \[\[ "\$realign_rc" -eq 7 \]\]; then\n(.*?)\n\s*elif \[\[ "\$realign_rc" -ne 0 \]\]; then',
                           setup, re.S)
        self.assertIsNotNone(branch, "setup does not handle ignored-data realignment refusal")
        report = re.search(r"^_report_update_failure\(\) \{\n.*?^\}", setup, re.M | re.S)
        self.assertIsNotNone(report)
        write_status = re.search(r"^_write_update_status\(\) \{\n.*?^\}", setup, re.M | re.S)
        self.assertIsNotNone(write_status)
        result = self.bash(write_status.group() + "\n" + report.group() + '''
_update_status_file="$XDG_STATE_HOME/quickshell/user/update-status"
tracked_branch=main
handle_realign_failure() {
    local realign_rc=7
    if [[ "$realign_rc" -eq 7 ]]; then
''' + branch.group(1) + '''
    fi
}
handle_realign_failure
''')
        self.assertEqual(result.returncode, 24, result.stdout + result.stderr)
        self.assertEqual((self.base / "state/quickshell/user/update-status").read_text(),
                         "failed:24:Realignment refused because origin/main overlaps ignored user data. Nothing was changed\n")

    @unittest.skipUnless(shutil.which("git"), "realignment probe requires git")
    def test_r04_actual_file_directory_collisions_preserve_head_and_recovery_refs(self):
        real_git = shutil.which("git")
        for mode in ("file-to-directory", "directory-to-file"):
            with self.subTest(mode=mode):
                repo = self.base / mode
                repo.mkdir()
                env = dict(self.env, REPO_ROOT=str(repo), INIR_AUDIT_REAL_GIT=real_git,
                           PATH=f"{Path(real_git).parent}:{self.env['PATH']}")
                result = self.bash(self.snapshot_prelude() + '''git() { "$INIR_AUDIT_REAL_GIT" "$@"; }
git -C "$REPO_ROOT" init -q -b main || exit 1
git -C "$REPO_ROOT" config user.name 'Review fixture'
git -C "$REPO_ROOT" config user.email review@example.invalid
printf '/user-data\\n' > "$REPO_ROOT/.gitignore"
git -C "$REPO_ROOT" add .gitignore
git -C "$REPO_ROOT" commit -qm old || exit 1
old=$(git -C "$REPO_ROOT" rev-parse HEAD)
git -C "$REPO_ROOT" update-ref --create-reflog refs/remotes/origin/main "$old"
git -C "$REPO_ROOT" checkout -q --orphan rewrite
if [[ "$2" == file-to-directory ]]; then
    mkdir "$REPO_ROOT/user-data"
    printf upstream > "$REPO_ROOT/user-data/item"
else
    printf upstream > "$REPO_ROOT/user-data"
fi
git -C "$REPO_ROOT" add -f user-data
git -C "$REPO_ROOT" commit -qm rewrite || exit 1
new=$(git -C "$REPO_ROOT" rev-parse HEAD)
git -C "$REPO_ROOT" checkout -q main
rm -rf -- "$REPO_ROOT/user-data"
if [[ "$2" == file-to-directory ]]; then
    printf private > "$REPO_ROOT/user-data"
else
    mkdir "$REPO_ROOT/user-data"
    printf private > "$REPO_ROOT/user-data/item"
fi
git -C "$REPO_ROOT" update-ref -m forced-update refs/remotes/origin/main "$new"
realign_repo_to_remote main
rc=$?
[[ "$rc" -eq 7 ]] || exit 11
[[ "$(git -C "$REPO_ROOT" rev-parse HEAD)" == "$old" ]] || exit 12
[[ -z "$(git -C "$REPO_ROOT" for-each-ref refs/inir/recovery)" ]] || exit 13
''', mode, env=env)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                private = repo / "user-data"
                if mode == "directory-to-file":
                    private /= "item"
                self.assertEqual(private.read_text(), "private")

    def test_r05_startup_accepts_quoted_nodes_and_comment_whitespace(self):
        root = self.niri / "config.kdl"
        cases = (
            '"spawn-at-startup" "inir" "run" "--session"\n',
            'spawn-at-startup /* multiline\n comment */ "inir" "run" "--session"\n',
            'spawn-at-startup \\ // continuation comment\n "inir" "run" "--session"\n',
        )
        for text in cases:
            with self.subTest(config=text):
                root.write_text(text)
                self.validate_niri(root)
                self.assertTrue(audit.startup.sync(self.config, self.bin / "inir", "status"),
                                "Niri accepts the command, but startup status cannot find it")
                audit.startup.sync(self.config, self.bin / "inir", "disable")
                self.validate_niri(root)
                self.assertNotIn('"--session"', root.read_text())

    def test_r05_startup_matches_active_args_when_prior_argument_is_disabled(self):
        root = self.niri / "config.kdl"
        root.write_text('spawn-at-startup /- "obsolete" "inir" "run" "--session"\n')
        self.validate_niri(root)
        self.assertTrue(audit.startup.sync(self.config, self.bin / "inir", "status"))
        audit.startup.sync(self.config, self.bin / "inir", "disable")
        self.assertFalse(audit.startup.sync(self.config, self.bin / "inir", "status"))
        self.validate_niri(root)

        audio = 'spawn-at-startup /- "obsolete" "gentoo-pipewire-launcher"\n'
        node = audit.startup._active_nodes(audio)[0]
        self.assertTrue(audit.startup.AUDIO.match(node),
                        "disabled arguments must not hide the active audio startup command")

    def test_r05_symlink_aliases_keep_distinct_relative_include_contexts(self):
        root = self.niri / "config.kdl"
        (self.niri / "a").mkdir()
        (self.niri / "b").mkdir()
        (self.niri / "shared").mkdir()
        external = self.base / "external-child.kdl"
        external.write_text('spawn-at-startup "inir" "run" "--session"\n')
        (self.niri / "a" / "link.kdl").symlink_to("../shared/main.kdl")
        (self.niri / "b" / "link.kdl").symlink_to("../shared/main.kdl")
        (self.niri / "a" / "child.kdl").write_text('// no startup here\n')
        (self.niri / "b" / "child.kdl").symlink_to(external)
        (self.niri / "shared" / "main.kdl").write_text('include "child.kdl"\n')
        root.write_text('include "a/link.kdl"\ninclude "b/link.kdl"\n')
        self.assertTrue(audit.startup.sync(self.config, self.bin / "inir", "status"),
                        "the second alias's relative include context was skipped")
        self.assertIn(external.resolve(), audit.startup.config_files(root))
        self.assertIn(external.resolve(), snapshot_module.include_graph(self.niri))

    def test_r05_parent_component_after_symlink_keeps_filesystem_path_semantics(self):
        root = self.niri / "config.kdl"
        external_dir = self.base / "external" / "subdir"
        external_dir.mkdir(parents=True)
        (self.niri / "alias").symlink_to(external_dir, target_is_directory=True)
        external_child = self.base / "external" / "child.kdl"
        external_child.write_text('spawn-at-startup "inir" "run" "--session"\n')
        (self.niri / "child.kdl").write_text("// local decoy\n")
        root.write_text('include "alias/../child.kdl"\n')
        self.assertTrue(audit.startup.sync(self.config, self.bin / "inir", "status"))
        self.assertIn(external_child.resolve(), audit.startup.config_files(root))
        self.assertIn(external_child.resolve(), snapshot_module.include_graph(self.niri))

    def test_r05_repeated_physical_include_can_continue_in_a_new_logical_context(self):
        root = self.niri / "config.kdl"
        (self.niri / "a/nested/nested").mkdir(parents=True)
        (self.niri / "shared").mkdir()
        external = self.base / "external-child.kdl"
        external.write_text('spawn-at-startup "inir" "run" "--session"\n')
        (self.niri / "a/link.kdl").symlink_to("../shared/main.kdl")
        (self.niri / "a/nested/link.kdl").symlink_to("../../shared/main.kdl")
        (self.niri / "a/nested/nested/link.kdl").symlink_to(external)
        (self.niri / "shared/main.kdl").write_text('include "nested/link.kdl"\n')
        root.write_text('include "a/link.kdl"\n')
        self.assertTrue(audit.startup.sync(self.config, self.bin / "inir", "status"))
        self.assertIn(external.resolve(), audit.startup.config_files(root))
        self.assertIn(external.resolve(), snapshot_module.include_graph(self.niri))

    def test_r05_disabled_subtrees_do_not_count_as_active_startup(self):
        root = self.niri / "config.kdl"
        child = self.niri / "child.kdl"
        child.write_text('spawn-at-startup "inir" "run" "--session"\n')
        for body in ('spawn-at-startup "inir" "run" "--session"', 'include "child.kdl"'):
            with self.subTest(body=body):
                root.write_text('/- unused {\n    ' + body + '\n}\n')
                self.validate_niri(root)
                self.assertFalse(audit.startup.sync(self.config, self.bin / "inir", "status"),
                                 "a slash-dash disabled subtree cannot own active startup")

    def test_r05_enable_preserves_a_disabled_subtree(self):
        root = self.niri / "config.kdl"
        disabled = '/- unused {\n    spawn-at-startup "inir" "run" "--session"\n}\n'
        root.write_text(disabled)
        self.validate_niri(root)
        audit.startup.sync(self.config, self.bin / "inir", "enable")
        self.validate_niri(root)
        self.assertIn(disabled, root.read_text(), "enable modified deliberately disabled config")

    def test_r05_snapshot_restores_quoted_include_and_skips_disabled_argument(self):
        self.snapshot_fixture()
        external = self.base / "active.kdl"
        original = 'spawn-at-startup "echo" "original"\n'
        for include in ('"include" ' + json.dumps(str(external)),
                        'include /- "missing.kdl" ' + json.dumps(str(external))):
            with self.subTest(include=include):
                external.write_text(original)
                (self.niri / "config.kdl").write_text(include + '\n')
                self.validate_niri(self.niri / "config.kdl")
                result = self.bash(self.snapshot_prelude() + '''snapshot=$(create_snapshot review) || exit 1
printf 'spawn-at-startup "echo" "changed"\\n' > "$2"
restore_snapshot "$snapshot"
''', external)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(external.read_text(), original,
                                 "successful rollback omitted an active include accepted by Niri")

    def test_r05_snapshot_ignores_includes_in_a_disabled_subtree(self):
        self.snapshot_fixture()
        not_a_file = self.base / "disabled-directory"
        not_a_file.mkdir()
        root = self.niri / "config.kdl"
        root.write_text('/- unused {\n    include ' + json.dumps(str(not_a_file)) + '\n}\n')
        self.validate_niri(root)
        result = self.bash(self.snapshot_prelude() + 'create_snapshot review\n')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_r05_snapshot_does_not_archive_includes_in_disabled_child_block(self):
        self.snapshot_fixture()
        external = self.base / "unrelated.kdl"
        old_data = 'spawn-at-startup "echo" "old-data"\n'
        external.write_text(old_data)
        root = self.niri / "config.kdl"
        root.write_text('binds /- {\n    include ' + json.dumps(str(external)) + '\n}\n')
        self.validate_niri(root)
        result = self.bash(self.snapshot_prelude() + '''snapshot=$(create_snapshot review) || exit 1
printf 'spawn-at-startup "echo" "new-user-data"\\n' > "$2"
restore_snapshot "$snapshot"
''', external)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(external.read_text(), 'spawn-at-startup "echo" "new-user-data"\n',
                         "disabled child include must not become rollback-owned")

    def test_r04_startup_follows_symlink_logical_parent(self):
        external = self.config / "external"
        external.mkdir()
        root = self.niri / "config.kdl"
        root.write_text('include "link.kdl"\n')
        (self.niri / "link.kdl").symlink_to("../external/main.kdl")
        (external / "main.kdl").write_text('include "child.kdl"\n')
        expected = 'spawn-at-startup "inir" "run" "--session"\n'
        child = self.niri / "child.kdl"
        child.write_text(expected)
        (external / "child.kdl").write_text('// not part of Niri logical graph\n')
        self.validate_niri(root)
        self.assertTrue(audit.startup.sync(self.config, self.bin / "inir", "status"))
        audit.startup.sync(self.config, self.bin / "inir", "disable")
        self.assertNotIn('"--session"', child.read_text())
        self.assertFalse(audit.startup.sync(self.config, self.bin / "inir", "status"))

    def test_r06_absolute_root_symlink_is_never_validated_against_live_file(self):
        actual = self.niri / "actual.kdl"
        actual.write_text('// known valid original\n')
        root = self.niri / "config.kdl"
        root.symlink_to(actual)
        self.validate_niri(root)
        with self.assertRaisesRegex(ValueError, r"absolute symlink"):
            audit.startup._validate_candidate(self.config, root,
                                              {actual.resolve(): 'definitely-invalid-node "candidate"\n'})
        self.assertEqual(actual.read_text(), '// known valid original\n')

    @unittest.skipUnless(shutil.which("niri"), "candidate validation requires real niri")
    def test_r06_enable_supports_relative_external_include(self):
        external = self.config / "shared/startup.kdl"
        external.parent.mkdir()
        external.write_text('spawn-at-startup "inir" "run" "--session"\n')
        root = self.niri / "config.kdl"
        original_root = 'include "../shared/startup.kdl"\n'
        root.write_text(original_root)
        self.validate_niri(root)
        try:
            audit.startup.sync(self.config, self.bin / "inir", "enable")
        except ValueError as error:
            message = str(error)
            self.assertRegex(message.lower(), r"(refus|unsupported|cannot safely).*(include|external|relative)|(include|external|relative).*(refus|unsupported|safe)")
            self.assertEqual(root.read_text(), original_root)
            self.assertEqual(external.read_text(), 'spawn-at-startup "inir" "run" "--session"\n')
            return
        self.validate_niri(root)
        files = audit.startup.config_files(root)
        self.assertEqual(sum(path.read_text().count('"--session"') for path in files), 1)

    @unittest.skipUnless(shutil.which("niri"), "candidate validation requires real niri")
    def test_r06_validator_sees_updated_absolute_include(self):
        external = self.base / "external.kdl"
        external.write_text('spawn-at-startup "inir" "run" "--session"\n')
        root = self.niri / "config.kdl"
        root.write_text('include ' + json.dumps(str(external)) + '\n')
        self.validate_niri(root)
        real_run = subprocess.run
        validated_command_counts = []

        def observe_validator(args, **kwargs):
            if len(args) == 4 and args[1:3] == ["validate", "-c"]:
                files = audit.startup.config_files(args[3])
                validated_command_counts.append(sum(
                    path.read_text().count('"--session"') for path in files))
            return real_run(args, **kwargs)

        root_before = root.read_text()
        external_before = external.read_text()
        try:
            with patch.object(audit.startup.subprocess, "run", side_effect=observe_validator):
                audit.startup.sync(self.config, self.bin / "inir", "enable")
        except ValueError as error:
            message = str(error)
            self.assertRegex(message.lower(), r"(refus|unsupported|cannot safely).*(include|external|absolute)|(include|external|absolute).*(refus|unsupported|safe)")
            self.assertEqual(root.read_text(), root_before)
            self.assertEqual(external.read_text(), external_before)
            return
        self.assertTrue(validated_command_counts, "candidate was not validated")
        self.assertEqual(validated_command_counts[0], 1,
                         "validator read the live external include instead of its pending contents")

    def test_r07_legacy_symlink_preserves_visible_external_config(self):
        external = self.base / "user-config"
        external.mkdir()
        original = '{"panelFamily":"iris"}\n'
        (external / "config.json").write_text(original)
        legacy = self.config / "illogical-impulse"
        legacy.symlink_to(external, target_is_directory=True)
        migration_status = self.base / "migration-status"
        result = self.bash('''source "$1/sdata/migrations/019-config-dir-rename-compat.sh"
migration_apply
migration_rc=$?
source "$1/scripts/lib/config-path.sh"
cat "$(inir_config_file)" || exit 20
printf '%s\\n' "$migration_rc" > "$2"
''', migration_status)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout, original)
        self.assertEqual((external / "config.json").read_text(), original)
        self.assertIn(migration_status.read_text().strip(), {"0", "1"})
        if migration_status.read_text().strip() == "1":
            self.assertTrue(result.stderr.strip(), "safe migration refusal must explain the reason")

    def test_r07_rejected_migration_preserves_existing_legacy_link(self):
        external = self.base / "user-config"
        external.mkdir()
        (external / "config.json").write_text('{"panelFamily":"iris"}\n')
        legacy = self.config / "illogical-impulse"
        legacy.symlink_to(external, target_is_directory=True)
        (self.config / "inir").write_text("canonical path obstruction\n")
        result = self.bash('''source "$1/sdata/migrations/019-config-dir-rename-compat.sh"
migration_apply
''')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(legacy.is_symlink(), "a rejected migration removed the original config link")
        self.assertEqual(legacy.resolve(), external.resolve())

    def test_r07_failed_external_copy_keeps_legacy_config_visible(self):
        external = self.base / "user-config"
        external.mkdir()
        original = '{"panelFamily":"iris"}\n'
        (external / "config.json").write_text(original)
        legacy = self.config / "illogical-impulse"
        legacy.symlink_to(external, target_is_directory=True)
        (self.config / "inir").mkdir()
        result = self.bash('''cp() { return 23; }
source "$1/sdata/migrations/019-config-dir-rename-compat.sh"
migration_apply && exit 20
source "$1/scripts/lib/config-path.sh"
cat "$(inir_config_file)"
''')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout, original)
        self.assertTrue(legacy.is_symlink())

    def test_r08_restore_refuses_redirected_external_parent(self):
        self.snapshot_fixture()
        external_dir = self.base / "external"
        external_dir.mkdir()
        external = external_dir / "startup.kdl"
        external.write_text('spawn-at-startup "echo" "original"\n')
        (self.niri / "config.kdl").write_text('include ' + json.dumps(str(external)) + '\n')
        self.validate_niri(self.niri / "config.kdl")
        created = self.bash(self.snapshot_prelude() + 'create_snapshot review\n')
        self.assertEqual(created.returncode, 0, created.stdout + created.stderr)
        snapshot = created.stdout.strip()
        external_dir.rename(self.base / "saved-external")
        unrelated = self.base / "unrelated"
        unrelated.mkdir()
        private = 'spawn-at-startup "echo" "unrelated-private-data"\n'
        (unrelated / "startup.kdl").write_text(private)
        external_dir.symlink_to(unrelated, target_is_directory=True)
        result = self.bash(self.snapshot_prelude() + 'restore_snapshot "$2"\n', snapshot)
        self.assertEqual((unrelated / "startup.kdl").read_text(), private,
                         "rollback followed a newly redirected parent and overwrote another file")
        self.assertNotEqual(result.returncode, 0, "changed external target identity must be refused")
        self.assertFalse(self.calls.exists(), "unsafe rollback reached the session owner")

    def test_r08_restore_retains_original_data_when_rollback_also_fails(self):
        self.snapshot_fixture()
        first = self.base / "first.kdl"
        second = self.base / "second.kdl"
        first_current = 'spawn-at-startup "echo" "current-first"\n'
        second_current = 'spawn-at-startup "echo" "current-second"\n'
        first.write_text('spawn-at-startup "echo" "snapshot-first"\n')
        second.write_text('spawn-at-startup "echo" "snapshot-second"\n')
        root = self.niri / "config.kdl"
        root.write_text('include ' + json.dumps(str(first)) + '\ninclude ' + json.dumps(str(second)) + '\n')
        created = self.bash(self.snapshot_prelude() + 'create_snapshot review\n')
        self.assertEqual(created.returncode, 0, created.stdout + created.stderr)
        snapshot_id = created.stdout.strip()
        first.write_text(first_current)
        second.write_text(second_current)
        real_replace = snapshot_module.os.replace
        calls = 0

        def fail_second_replace(source, target):
            nonlocal calls
            calls += 1
            if calls > 1:
                raise OSError("injected restore and rollback replace failure")
            return real_replace(source, target)

        snapshot_dir = self.base / "state/quickshell/snapshots" / snapshot_id
        with patch.object(snapshot_module.os, "replace", side_effect=fail_second_replace):
            with self.assertRaisesRegex(OSError, "retained backup"):
                snapshot_module.restore(snapshot_dir / "niri-include-manifest.json",
                                        snapshot_dir / "niri-external")
        backups = list(self.base.glob(".inir-niri-backup-*"))
        self.assertTrue(any(path.read_text() == first_current for path in backups),
                        "rollback failure deleted the only copy of current external data")
        self.assertEqual(second.read_text(), second_current)

    def test_r09_failed_startup_publish_restores_previously_written_files(self):
        root = self.niri / "config.kdl"
        child = self.niri / "50-startup.kdl"
        original_root = 'spawn-at-startup "inir" "run" "--session"\ninclude "50-startup.kdl"\n'
        original_child = '// user startup\n'
        root.write_text(original_root)
        child.write_text(original_child)
        self.validate_niri(root)
        real_write = audit.startup.atomic_write

        def fail_second_file(path, text):
            if path == child:
                raise OSError("injected failure publishing the second startup file")
            return real_write(path, text)

        with patch.object(audit.startup, "atomic_write", side_effect=fail_second_file):
            with self.assertRaises(OSError):
                audit.startup.sync(self.config, self.bin / "inir", "enable")
        self.assertEqual(root.read_text(), original_root,
                         "failed enable removed the working autostart before publishing its replacement")
        self.assertEqual(child.read_text(), original_child)

    def test_r09_startup_keeps_durable_backup_if_rollback_write_fails(self):
        root = self.niri / "config.kdl"
        child = self.niri / "50-startup.kdl"
        original_root = 'spawn-at-startup "inir" "run" "--session"\ninclude "50-startup.kdl"\n'
        original_child = '// original child data\n'
        root.write_text(original_root)
        child.write_text(original_child)
        real_write = audit.startup.atomic_write
        calls = 0

        def fail_publish_and_rollback(path, text):
            nonlocal calls
            calls += 1
            if calls > 1:
                raise OSError("injected second publish and rollback failure")
            return real_write(path, text)

        real_replace = audit.startup.os.replace
        replace_calls = 0

        def fail_rollback(source, target):
            nonlocal replace_calls
            replace_calls += 1
            if replace_calls > 1:
                raise OSError("injected rollback failure")
            return real_replace(source, target)

        with patch.object(audit.startup, "atomic_write", side_effect=fail_publish_and_rollback), \
                patch.object(audit.startup.os, "replace", side_effect=fail_rollback):
            with self.assertRaisesRegex(OSError, "retained backups"):
                audit.startup.publish_transaction({root: 'include "50-startup.kdl"\n',
                                                   child: 'spawn-at-startup "inir" "run" "--session"\n'})
        backups = list(self.niri.glob(".inir-startup-backup-*"))
        self.assertIn(original_root, [backup.read_text() for backup in backups])


if __name__ == "__main__":
    unittest.main(verbosity=2)
