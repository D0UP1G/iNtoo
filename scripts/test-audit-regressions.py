#!/usr/bin/env python3
"""Behavioral regressions from docs/AUDIT_AND_FIX_PLAN.ru.md.

The audit baseline intentionally fails these assertions. All writes use temporary
directories; session and package commands are stubbed. Native QML cases use a
private D-Bus session and the offscreen Qt platform.
"""

import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("audit_startup", ROOT / "scripts/lib/session-startup.py")
startup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(startup)


class AuditRegressions(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="inir audit ")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.config = self.base / "config"
        self.niri = self.config / "niri"
        self.runtime = self.base / "runtime"
        self.bin = self.base / "bin"
        self.repo = self.base / "repo"
        for directory in (self.niri, self.runtime, self.bin, self.repo / "scripts"):
            directory.mkdir(parents=True)
        self.runtime.chmod(0o700)
        self.calls = self.base / "calls"
        self.env = dict(os.environ, HOME=str(self.base), XDG_CONFIG_HOME=str(self.config),
                        XDG_STATE_HOME=str(self.base / "state"), XDG_CACHE_HOME=str(self.base / "cache"),
                        XDG_RUNTIME_DIR=str(self.runtime), XDG_BIN_HOME=str(self.bin),
                        DOTS_CORE_CONFDIR=str(self.config / "inir"), REPO_ROOT=str(self.repo),
                        PATH=f"{self.bin}:{os.environ['PATH']}", INIR_INIT_SYSTEM="openrc",
                        INIR_AUDIT_CALLS=str(self.calls), NIRI_SOCKET="", WAYLAND_DISPLAY="",
                        DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent-audit-bus")
        self.stub("git", '#!/usr/bin/env bash\ncase "$*" in *rev-parse*) echo unknown ;; esac\n')
        for name in ("qs", "inir", "systemctl", "journalctl"):
            self.stub(name, '#!/usr/bin/env bash\nprintf "%s %s\\n" "${0##*/}" "$*" >> "$INIR_AUDIT_CALLS"\n')
        shutil.copy(self.bin / "inir", self.repo / "scripts/inir")

    def stub(self, name, text):
        path = self.bin / name
        path.write_text(text)
        path.chmod(0o755)
        return path

    def bash(self, code, *args, env=None):
        return subprocess.run(["bash", "-c", code, "audit", str(ROOT), *map(str, args)],
                              env=env or self.env, text=True, capture_output=True, timeout=10)

    def validate_niri(self, path):
        if shutil.which("niri"):
            result = subprocess.run(["niri", "validate", "-c", str(path)],
                                    env=self.env, capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def snapshot_fixture(self):
        payload = self.config / "quickshell/inir"
        payload.mkdir(parents=True)
        (payload / "shell.qml").write_text("original payload\n")
        directory = self.niri / "config.d"
        directory.mkdir()
        (self.niri / "config.kdl").write_text('include "config.d/50-startup.kdl"\n')
        (directory / "50-startup.kdl").write_text("// original startup\n")

    def snapshot_prelude(self):
        return '''source "$1/sdata/lib/snapshots.sh"
get_installed_version() { printf audit; }
get_installed_update_strategy() { printf repo-setup; }
set_installed_version() { :; }
log_info() { :; }
log_error() { :; }
tui_success() { :; }
tui_warn() { :; }
tui_info() { :; }
'''

    def gentoo_install_fixture(self, backend, only_missing="", single_package_use_file=False):
        system = self.base / "portage-root"
        (system / "etc/portage").mkdir(parents=True, exist_ok=True)
        use_path = system / "etc/portage/package.use"
        if single_package_use_file:
            if use_path.is_dir():
                shutil.rmtree(use_path)
            use_path.write_text("# preserve this Portage setting\n")
        env = dict(self.env, INIR_INIT_SYSTEM=backend, INIR_AUDIT_PORTAGE_ROOT=str(system),
                   ONLY_MISSING_DEPS=only_missing)
        self.calls.unlink(missing_ok=True)
        self.stub("portageq", '''#!/usr/bin/env bash
case "$*" in
    'envvar ARCH') echo amd64 ;;
    'envvar PORTAGE_CONFIGROOT') echo "$INIR_AUDIT_PORTAGE_ROOT" ;;
    *) exit 1 ;;
esac
''')
        self.stub("eselect", '#!/usr/bin/env bash\nprintf "[1] guru\\n"\n')
        self.stub("emerge", '''#!/usr/bin/env python3
import json, os, sys
with open(os.environ["INIR_AUDIT_CALLS"], "a") as file:
    file.write(json.dumps(sys.argv[1:]) + "\\n")
''')
        result = self.bash('''source "$1/scripts/lib/session-backend.sh"
ask=false
SKIP_SYSUPDATE=true
INSTALL_AUDIO=false
INSTALL_TOOLKIT=false
INSTALL_SCREENCAPTURE=false
INSTALL_FONTS=false
v() { "$@"; }
pkg_sudo() { "$@"; }
log_warning() { :; }
log_error() { printf '%s\\n' "$*" >&2; }
log_success() { :; }
source "$1/sdata/dist-gentoo/install-deps.sh"
''', env=env)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        files = list((system / "etc/portage").rglob("*"))
        use = "\n".join(path.read_text() for path in files if path.is_file())
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        return use, calls

    def test_a00_gentoo_enables_quickshell_runtime_features(self):
        # These flags provide the modules/types used by the real shell, even
        # when its optional panels are closed. Hyprland imports are currently
        # unconditional in the Niri entrypoint's component dependency tree.
        required = {"sockets", "wayland", "layer-shell", "session-lock",
                    "toplevel-management", "hyprland", "screencopy", "tray", "pipewire",
                    "mpris", "pam", "upower", "notifications", "bluetooth",
                    "networkmanager"}
        for backend in ("openrc", "systemd"):
            with self.subTest(backend=backend):
                use, calls = self.gentoo_install_fixture(backend)
                flags = set()
                for line in use.splitlines():
                    words = line.split()
                    if words and words[0].split("::")[0] == "gui-apps/quickshell":
                        flags.update(words[1:])
                self.assertFalse(required - flags,
                                 f"missing Quickshell USE flags: {sorted(required - flags)}")
                qs_calls = [call for call in calls if "gui-apps/quickshell::guru" in call]
                self.assertTrue(qs_calls, "Quickshell was not installed/rebuilt")
                self.assertTrue(all("--changed-use" in call for call in qs_calls),
                                "existing Quickshell must be rebuilt when its USE flags change")

                # Doctor repairs a partial Quickshell build through the same
                # profile and forces Portage to rebuild its changed USE set.
                repair_use, repair_calls = self.gentoo_install_fixture(
                    backend, only_missing="quickshell-qml-modules",
                    single_package_use_file=True)
                repair_flags = {flag for line in repair_use.splitlines()
                                if line.startswith("gui-apps/quickshell ")
                                for flag in line.split()[1:]}
                self.assertFalse(required - repair_flags)
                self.assertIn("# preserve this Portage setting", repair_use)
                repair_qs_calls = [call for call in repair_calls
                                   if "gui-apps/quickshell::guru" in call]
                self.assertTrue(repair_qs_calls)
                self.assertTrue(all("--changed-use" in call for call in repair_qs_calls))
                repair_atoms = {word for call in repair_calls for word in call}
                self.assertTrue({"dev-qt/qt5compat", "dev-qt/qtmultimedia"} <= repair_atoms)

    def test_a00_gentoo_installs_qt_import_dependencies(self):
        _, calls = self.gentoo_install_fixture("openrc")
        installed = {word.split("::")[0] for call in calls for word in call}
        required = {"dev-qt/qt5compat", "dev-qt/qtmultimedia"}
        self.assertFalse(required - installed,
                         f"QML imports need explicit dependencies: {sorted(required - installed)}")

    def test_a00_gentoo_enables_qt_qml_features(self):
        # Both Qt packages may be installed without their QML plugins.
        # Installation and Doctor must enable them for both Portage layouts.
        required = {"dev-qt/qt5compat": {"gui", "qml"},
                    "dev-qt/qtmultimedia": {"qml"}}
        for backend in ("openrc", "systemd"):
            for missing in ("", "quickshell-qml-modules"):
                for single_file in (False, True):
                    with self.subTest(backend=backend, missing=missing,
                                      single_file=single_file):
                        use, calls = self.gentoo_install_fixture(
                            backend, only_missing=missing,
                            single_package_use_file=single_file)
                        for atom, needed in required.items():
                            flags = {flag for line in use.splitlines()
                                     if line.startswith(atom + " ")
                                     for flag in line.split()[1:]}
                            self.assertFalse(needed - flags,
                                             f"missing {atom} USE flags: {sorted(needed - flags)}")
                            merges = [call for call in calls if atom in call]
                            self.assertTrue(merges, f"{atom} was not installed/rebuilt")
                            self.assertTrue(all("--changed-use" in call for call in merges))

    def test_a00_gentoo_doctor_repair_configures_ocr_use_and_package_atoms(self):
        use, calls = self.gentoo_install_fixture(
            "openrc", only_missing="ocr-eng ocr-spa ocr-rus ocr-jpn-vert ocr-chi-tra-vert "
            "awww awww-daemon kwriteconfig6 lsp-plugins-lv2 "
            "uv easyeffects qalc blueman-manager ddcutil missioncenter trans "
            "flock xdg-settings yt-dlp")
        tessdata_flags = {flag for line in use.splitlines()
                          if line.startswith("app-text/tessdata_fast ")
                          for flag in line.split()[1:]}
        self.assertTrue({"l10n_en", "l10n_es", "l10n_ru", "l10n_ja", "l10n_zh"}
                        <= tessdata_flags)
        lsp_flags = {flag for line in use.splitlines()
                     if line.startswith("media-libs/lsp-plugins ")
                     for flag in line.split()[1:]}
        self.assertIn("lv2", lsp_flags)
        atoms = {word for call in calls for word in call}
        self.assertIn("--changed-use", atoms,
                      "Doctor must rebuild packages whose Gentoo USE flags changed")
        self.assertTrue({"app-text/tessdata_fast", "gui-apps/awww::guru",
                        "media-libs/lsp-plugins", "app-misc/ddcutil",
                        "sys-apps/mission-center::guru", "x11-misc/xdg-utils",
                        "net-misc/yt-dlp", "sys-apps/util-linux", "dev-python/uv"} <= atoms)

    def test_a01_rename_preserves_conflicting_legacy_data(self):
        legacy = self.config / "illogical-impulse"
        canonical = self.config / "inir"
        legacy.mkdir()
        canonical.mkdir()
        (legacy / "config.json").write_text('{"sentinel":"legacy-unique-data"}')
        (canonical / "config.json").write_text('{"sentinel":"canonical"}')
        result = self.bash('source "$1/sdata/migrations/019-config-dir-rename-compat.sh"\nmigration_apply')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads((canonical / "config.json").read_text())["sentinel"], "canonical")
        self.assertTrue(any("legacy-unique-data" in p.read_text()
                            for p in self.config.rglob("*") if p.is_file()),
                        "conflicting legacy data was discarded")

    def test_a02_modularization_keeps_disabled_startup_disabled(self):
        config = self.niri / "config.kdl"
        config.write_text('''/*
spawn-at-startup "disabled-tool"
*/
spawn-at-startup "echo" "first
second"
spawn-at-startup \\
    "echo" "hello"
layout { gaps 8; } /*
spawn-at-startup "boundary-disabled"
*/
environment {
    TEST "}"
    TEST_RAW r"C:\\"
    OTHER "value"
}
''')
        self.validate_niri(config)
        result = self.bash('source "$1/sdata/migrations/018-modularize-niri-config.sh"\nmigration_apply')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.validate_niri(config)
        active = "\n".join(re.sub(r"/\*.*?\*/|//[^\n]*", "", p.read_text(), flags=re.S)
                           for p in [config, *self.niri.glob("config.d/*.kdl")])
        self.assertNotIn('spawn-at-startup "disabled-tool"', active)
        self.assertNotIn('spawn-at-startup "boundary-disabled"', active)
        startup_fragment = (self.niri / "config.d/50-startup.kdl").read_text()
        self.assertIn('spawn-at-startup "echo" "first\nsecond"', startup_fragment)
        self.assertIn('spawn-at-startup \\\n    "echo" "hello"', startup_fragment)
        environment = (self.niri / "config.d/40-environment.kdl").read_text()
        self.assertIn('TEST "}"', environment)
        self.assertIn('TEST_RAW r"C:\\"', environment)
        self.assertIn('OTHER "value"', environment)
        layout_fragment = (self.niri / "config.d/20-layout-and-overview.kdl").read_text()
        self.assertIn("layout", layout_fragment)
        self.assertIn('/*\nspawn-at-startup "boundary-disabled"\n*/', layout_fragment)

    def test_a03_startup_ignores_block_commented_include(self):
        directory = self.niri / "config.d"
        directory.mkdir()
        inactive = directory / "50-startup.kdl"
        inactive.write_text("// inactive user file\n")
        config = self.niri / "config.kdl"
        config.write_text('/*\ninclude "config.d/50-startup.kdl"\n*/\n')
        self.validate_niri(config)
        original = inactive.read_bytes()
        startup.sync(self.config, self.bin / "inir", "enable")
        self.assertEqual(inactive.read_bytes(), original, "inactive include was edited")
        active_root = re.sub(r"/\*.*?\*/", "", config.read_text(), flags=re.S)
        self.assertIn('"run" "--session"', active_root)

    def test_a04_required_migration_failure_reaches_caller(self):
        migrations = self.base / "migrations"
        migrations.mkdir()
        (migrations / "001-audit-fail.sh").write_text('''MIGRATION_REQUIRED=true
MIGRATION_TARGET_FILE=""
MIGRATION_TITLE=audit-failure
migration_check() { return 0; }
migration_apply() { return 1; }
''')
        (migrations / "002-audit-optional.sh").write_text('MIGRATION_REQUIRED=false\n')
        result = self.bash('''source "$1/sdata/lib/migrations.sh"
MIGRATIONS_DIR="$2"
tui_check_fail() { :; }
tui_check_ok() { :; }
tui_check_skip() { :; }
run_migrations_auto
''', migrations)
        self.assertNotEqual(result.returncode, 0, "required migration failed but batch returned success")

    def test_a04_setup_migrate_propagates_batch_failure(self):
        # Exercise the real caller too: fixing only run_migrations_auto still
        # leaves setup reporting success if it ignores the function's status.
        source = (ROOT / "setup").read_text()
        function = re.search(r"^run_migrate\(\) \{\n.*?^\}", source, re.M | re.S)
        self.assertIsNotNone(function, "run_migrate function not found")
        result = self.bash('''tui_title() { :; }
sync_launcher_from_repo() { :; }
run_migrations_auto() { return 7; }
count_pending_migrations() { printf '0\\n'; }
tui_success() { :; }
show_session_impact_notices() { :; }
''' + function.group() + "\nrun_migrate\n")
        self.assertNotEqual(result.returncode, 0, "setup migrate concealed a required migration failure")

    def test_a05_snapshot_restores_active_niri_include(self):
        self.snapshot_fixture()
        result = self.bash(self.snapshot_prelude() + '''snapshot=$(create_snapshot audit)
printf '// changed startup\n' > "$XDG_CONFIG_HOME/niri/config.d/50-startup.kdl"
restore_snapshot "$snapshot"
''')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.niri / "config.d/50-startup.kdl").read_text(), "// original startup\n")

    def test_a06_failed_snapshot_copy_returns_failure(self):
        self.snapshot_fixture()
        result = self.bash(self.snapshot_prelude() + 'rsync() { return 23; }\ncreate_snapshot audit\n')
        self.assertNotEqual(result.returncode, 0, "failed rsync published a successful snapshot")

    def test_a06_failed_snapshot_restore_does_not_publish_success(self):
        self.snapshot_fixture()
        result = self.bash(self.snapshot_prelude() + '''snapshot=$(create_snapshot audit)
set_installed_version() { touch "$XDG_STATE_HOME/incorrect-restore-success"; }
rsync() { return 23; }
restore_snapshot "$snapshot"
''')
        self.assertNotEqual(result.returncode, 0, "failed restore returned success")
        self.assertFalse((self.base / "state/incorrect-restore-success").exists(),
                         "version metadata was committed after restore failed")

    def test_a06_snapshot_metadata_roundtrips_quotes_and_newlines(self):
        self.snapshot_fixture()
        description = 'Before update: "quoted value"\nsecond line'
        result = self.bash(self.snapshot_prelude() + 'create_snapshot audit "$2"\n', description)
        self.assertEqual(result.returncode, 0, result.stderr)
        metadata = list((self.base / "state/quickshell/snapshots").glob("*/snapshot.json"))
        self.assertEqual(len(metadata), 1)
        try:
            data = json.loads(metadata[0].read_text())
        except json.JSONDecodeError as error:
            self.fail(f"snapshot metadata is invalid JSON: {error}")
        self.assertEqual(data["description"], description)

    def test_a06_update_snapshot_failure_is_reported_to_caller(self):
        source = (ROOT / "setup").read_text()
        function = re.search(r"^_create_update_snapshot_or_fail\(\) \{\n.*?^\}", source, re.M | re.S)
        self.assertIsNotNone(function, "update snapshot guard not found")
        write_status = re.search(r"^_write_update_status\(\) \{\n.*?^\}", source, re.M | re.S)
        self.assertIsNotNone(write_status, "update status writer not found")
        report_failure = re.search(r"^_report_update_failure\(\) \{\n.*?^\}", source, re.M | re.S)
        self.assertIsNotNone(report_failure, "update failure reporter not found")
        run_update = re.search(r"^run_update\(\) \{\n.*?^\}", source, re.M | re.S)
        self.assertIsNotNone(run_update, "run_update function not found")
        self.assertGreaterEqual(run_update.group().count("_create_update_snapshot_or_fail"), 3)
        self.assertNotRegex(run_update.group(), r"local\s+snapshot_id=\$\(create_snapshot")
        result = self.bash('''_update_status_file="$XDG_STATE_HOME/quickshell/user/update-status"
''' + write_status.group() + "\n" + report_failure.group() + "\n" + function.group() + '''
create_snapshot() { return 17; }
_create_update_snapshot_or_fail update-description audit-commit
''')
        self.assertNotEqual(result.returncode, 0, "update continued after its required snapshot failed")
        status = self.base / "state/quickshell/user/update-status"
        self.assertEqual(status.read_text(), "failed:42:Could not create a required update snapshot\n")

    def test_a07_rollback_uses_session_owner(self):
        self.snapshot_fixture()
        for backend in ("openrc", "systemd"):
            with self.subTest(backend=backend):
                self.calls.unlink(missing_ok=True)
                result = self.bash(self.snapshot_prelude() + '''snapshot=$(create_snapshot audit)
export WAYLAND_DISPLAY=audit-stub
restore_snapshot "$snapshot"
for attempt in 1 2 3 4 5; do
    [[ $(wc -l < "$INIR_AUDIT_CALLS") -ge 2 ]] && break
    sleep 0.05
done
''', env=dict(self.env, INIR_INIT_SYSTEM=backend))
                self.assertEqual(result.returncode, 0, result.stderr)
                calls = self.calls.read_text() if self.calls.exists() else ""
                self.assertNotIn("qs -p", calls, "rollback bypassed the session owner")
                if backend == "openrc":
                    self.assertIn("inir ", calls, "OpenRC rollback did not invoke the supervised launcher")
                else:
                    self.assertTrue("inir " in calls or "systemctl --user " in calls,
                                    "systemd rollback did not invoke its lifecycle owner")

    def test_a08_cliphist_migration_honors_custom_xdg(self):
        directory = self.niri / "config.d"
        directory.mkdir()
        file = directory / "50-startup.kdl"
        file.write_text('spawn-at-startup "sh" "-c" "wl-paste --type text --watch cliphist store"\n')
        result = self.bash('source "$1/sdata/migrations/042-cliphist-no-synthetic-newline.sh"\nmigration_check')
        self.assertEqual(result.returncode, 0, "migration ignored the active XDG_CONFIG_HOME")
        result = self.bash('source "$1/sdata/migrations/042-cliphist-no-synthetic-newline.sh"\nmigration_apply')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--no-newline", file.read_text())

    def native_qml(self, directory, config_path=None):
        qs = shutil.which("qs")
        if not qs or not shutil.which("dbus-run-session") or not shutil.which("timeout"):
            self.skipTest("native cases require qs, dbus-run-session and timeout")
        env = dict(self.env, QT_QPA_PLATFORM="offscreen", QS_DISABLE_CRASH_HANDLER="1",
                   PROBE_CONFIG_PATH=str(config_path or self.base / "probe-config.json"))
        result = subprocess.run(["dbus-run-session", "--", "timeout", "--kill-after=1s", "3s",
                                 qs, "-n", "-p", str(directory)], env=env,
                                text=True, capture_output=True, timeout=7)
        log = result.stdout + result.stderr
        self.assertIn("Configuration Loaded", log, log)
        self.assertEqual(result.returncode, 124, log)
        return log

    def test_a11_rollback_preserves_uncommitted_checkout_changes(self):
        self.snapshot_fixture()
        git = shutil.which("git")
        if not git:
            self.skipTest("checkout preservation probe requires git")
        # Every Git operation targets the temporary repository, never ROOT.
        env = dict(self.env, INIR_AUDIT_REAL_GIT=git,
                   PATH=f"{Path(git).parent}:{self.env['PATH']}")
        result = self.bash(self.snapshot_prelude() + '''git() { "$INIR_AUDIT_REAL_GIT" "$@"; }
git -C "$REPO_ROOT" init -q -b current || exit 1
git -C "$REPO_ROOT" config user.name 'Audit fixture'
git -C "$REPO_ROOT" config user.email 'audit@example.invalid'
printf 'committed content\\n' > "$REPO_ROOT/tracked.txt"
git -C "$REPO_ROOT" add tracked.txt
git -C "$REPO_ROOT" add -f scripts/inir
git -C "$REPO_ROOT" commit -qm fixture || exit 1
snapshot=$(create_snapshot audit)
printf 'unsaved user change\\n' > "$REPO_ROOT/tracked.txt"
restore_snapshot "$snapshot"
''', env=env)
        self.assertEqual((self.repo / "tracked.txt").read_text(), "unsaved user change\n",
                         "rollback discarded uncommitted user work")
        self.assertNotEqual(result.returncode, 0, "unsafe checkout rollback must stop before resetting")

    def test_a11_rollback_preserves_branch_and_ignored_user_data(self):
        self.snapshot_fixture()
        git = shutil.which("git")
        if not git:
            self.skipTest("checkout preservation probe requires git")
        env = dict(self.env, INIR_AUDIT_REAL_GIT=git,
                   PATH=f"{Path(git).parent}:{self.env['PATH']}")
        result = self.bash(self.snapshot_prelude() + '''git() { "$INIR_AUDIT_REAL_GIT" "$@"; }
git -C "$REPO_ROOT" init -q -b current || exit 1
git -C "$REPO_ROOT" config user.name 'Audit fixture'
git -C "$REPO_ROOT" config user.email 'audit@example.invalid'
printf 'tracked old content\\n' > "$REPO_ROOT/user-data"
git -C "$REPO_ROOT" add user-data
git -C "$REPO_ROOT" add -f scripts/inir
git -C "$REPO_ROOT" commit -qm old || exit 1
snapshot=$(create_snapshot audit) || exit 1
git -C "$REPO_ROOT" rm -q user-data
printf '/user-data\\n' > "$REPO_ROOT/.gitignore"
git -C "$REPO_ROOT" add .gitignore
git -C "$REPO_ROOT" commit -qm new || exit 1
new_commit=$(git -C "$REPO_ROOT" rev-parse HEAD)
git -C "$REPO_ROOT" branch a-alternate "$new_commit"
printf 'ignored user data\\n' > "$REPO_ROOT/user-data"
restore_snapshot "$snapshot"
''', env=env)
        self.assertNotEqual(result.returncode, 0,
                            "rollback should refuse a target commit that collides with ignored data: "
                            + result.stdout + result.stderr)
        self.assertEqual((self.repo / "user-data").read_text(), "ignored user data\n")
        successful = self.bash(self.snapshot_prelude() + '''git() { "$INIR_AUDIT_REAL_GIT" "$@"; }
rm -f "$REPO_ROOT/user-data"
snapshot=$(find "$XDG_STATE_HOME/quickshell/snapshots" -mindepth 2 -maxdepth 2 -name snapshot.json -printf '%h\\n' | head -n 1)
restore_snapshot "${snapshot##*/}"
''', env=env)
        self.assertEqual(successful.returncode, 0, successful.stderr)
        current = subprocess.run([git, "-C", str(self.repo), "rev-parse", "HEAD"],
                                 text=True, capture_output=True, check=True).stdout.strip()
        branch = subprocess.run([git, "-C", str(self.repo), "branch", "--show-current"],
                                text=True, capture_output=True, check=True).stdout.strip()
        alternate = subprocess.run([git, "-C", str(self.repo), "rev-parse", "a-alternate"],
                                   text=True, capture_output=True, check=True).stdout.strip()
        self.assertEqual(branch, "", "rollback should detach instead of moving a containing branch")
        self.assertNotEqual(current, alternate)
        self.assertEqual(alternate, subprocess.run(
            [git, "-C", str(self.repo), "rev-parse", "current"], text=True,
            capture_output=True, check=True).stdout.strip())

    def test_a09_removing_last_mascot_persists_empty_bucket(self):
        directory = self.base / "qml"
        common = directory / "common"
        common.mkdir(parents=True)
        shutil.copy(ROOT / "modules/common/Config.qml", common / "Config.qml")
        (common / "Directories.qml").write_text('pragma Singleton\nimport Quickshell\nSingleton { readonly property bool shellConfigResolved: true; readonly property string shellConfigPath: Quickshell.env("PROBE_CONFIG_PATH") }\n')
        (common / "qmldir").write_text('singleton Config 1.0 Config.qml\nsingleton Directories 1.0 Directories.qml\n')
        file = self.base / "probe-config.json"
        file.write_text(json.dumps({"panelFamily": "ii", "background": {"widgets": {"mascotInstances": {"one": {"enable": True}}}}}))
        (directory / "shell.qml").write_text('''import QtQuick
import Quickshell
import "common" as Common
ShellRoot {
 property string family: Common.Config.options.panelFamily
 Timer { interval: 500; running: true; onTriggered: {
  console.log("AUDIT_READY", Common.Config.ready);
  Common.Config.removeMascotInstance("one");
  Common.Config.flushWrites();
  console.log("AUDIT_MEMORY", JSON.stringify(Common.Config.mascotInstances));
 } }
}
''')
        log = self.native_qml(directory, file)
        self.assertIn("AUDIT_READY true", log, log)
        self.assertIn("AUDIT_MEMORY {}", log, log)
        self.assertEqual(json.loads(file.read_text())["background"]["widgets"].get("mascotInstances", {}), {})

    @unittest.skipUnless(shutil.which("node"), "updater probe requires node")
    def test_a12_updater_accepts_apostrophe_in_checkout_path(self):
        script = r'''
const fs = require("fs"), vm = require("vm"), cp = require("child_process");
const source = fs.readFileSync(process.argv[1], "utf8");
const body = source.match(/^    function performUpdate\(\): void \{\n([\s\S]*?)^    \}/m);
if (!body) throw new Error("performUpdate function not found");
let captured;
const root = {isUpdating:false, hasUpdate:true, available:true, managedExternally:false, repoPath:"/tmp/user's checkout",
  shellQuote(value){ return "'" + String(value).replace(/'/g, "'\"'\"'") + "'"; }};
const timer = {restart(){}};
vm.runInNewContext("(function(){" + body[1] + "})()", {
 root, ...root, Directories:{updateLogPath:"/tmp/audit.log", updateStatusPath:"/tmp/audit.status"},
 Config:{options:{shellUpdates:{openTerminalOnUpdate:false}}, setNestedValue(){}},
 Quickshell:{execDetached(args){captured=args;}}, updateWatchdog:timer, updateProgressPoller:timer, print(){}
});
const result = cp.spawnSync("bash", ["-n", "-c", captured[2]], {encoding:"utf8"});
console.log(JSON.stringify({code:result.status, error:result.stderr}));
'''
        result = subprocess.run(["node", "-e", script, str(ROOT / "services/ShellUpdates.qml")],
                                env=self.env, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        output = json.loads(result.stdout)
        self.assertEqual(output["code"], 0, output["error"])

    def test_a13_notification_group_handles_prototype_names(self):
        directory = self.base / "qml"
        pill = directory / "pill"
        services = directory / "services"
        common = directory / "modules/common"
        for folder in (pill, services, common):
            folder.mkdir(parents=True)
        shutil.copy(ROOT / "modules/pill/PillNotifs.qml", pill / "PillNotifs.qml")
        (pill / "qmldir").write_text("singleton PillNotifs 1.0 PillNotifs.qml\n")
        names = ["Demo", "constructor", "__proto__", "toString"]
        notifications = [dict(appName=name, notificationId=i, summary="audit", body="body", time=1)
                         for i, name in enumerate(names)]
        (services / "Notifications.qml").write_text('pragma Singleton\nimport Quickshell\nSingleton { property var list: ' + json.dumps(notifications) + '\nproperty var popupList: [] }\n')
        (services / "CompositorService.qml").write_text('pragma Singleton\nimport Quickshell\nSingleton { property bool isNiri: false }\n')
        (services / "NiriService.qml").write_text('pragma Singleton\nimport Quickshell\nSingleton { property var windows: [] }\n')
        (services / "qmldir").write_text('singleton Notifications 1.0 Notifications.qml\nsingleton CompositorService 1.0 CompositorService.qml\nsingleton NiriService 1.0 NiriService.qml\n')
        (common / "Translation.qml").write_text('pragma Singleton\nimport Quickshell\nSingleton { function tr(s) { return s } }\n')
        (common / "qmldir").write_text('singleton Translation 1.0 Translation.qml\n')
        (directory / "shell.qml").write_text('''import QtQuick
import Quickshell
import "pill" as Pill
ShellRoot { property var p: Pill.PillNotifs
 Timer { interval: 500; running: true; onTriggered: console.log("AUDIT_GROUPS", JSON.stringify(Pill.PillNotifs.groups)) }
}
''')
        log = self.native_qml(directory)
        self.assertNotIn("TypeError", log, log)
        line = next(line for line in log.splitlines() if "AUDIT_GROUPS " in line)
        groups = json.loads(line.split("AUDIT_GROUPS ", 1)[1])
        self.assertEqual(sorted(group["app"] for group in groups), sorted(names))

    def test_control_launcher_model_loads_in_native_qml(self):
        directory = self.base / "qml"
        directory.mkdir()
        shutil.copy(ROOT / "modules/common/models/LauncherSearchResult.qml", directory / "LauncherSearchResult.qml")
        (directory / "shell.qml").write_text('''import QtQuick
import Quickshell
ShellRoot { property LauncherSearchResult probe: LauncherSearchResult {}
 Timer { interval: 500; running: true; onTriggered: console.log("AUDIT_MODEL", probe.iconType, probe.fontType) }
}
''')
        log = self.native_qml(directory)
        self.assertIn("AUDIT_MODEL 3 0", log, log)
        self.assertNotIn("TypeError", log, log)


if __name__ == "__main__":
    unittest.main(verbosity=2)
