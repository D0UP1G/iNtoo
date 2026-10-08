#!/usr/bin/env python3
"""Behavioral regressions for PR05–PR16; all lifecycle writes use temp fixtures."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('audit_fixture', ROOT / 'scripts/test-audit-regressions.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)
spec = importlib.util.spec_from_file_location('package_warning', ROOT / 'scripts/package-manager-running.py')
warning = importlib.util.module_from_spec(spec)
spec.loader.exec_module(warning)


def function(path, name):
    source = (ROOT / path).read_text()
    match = re.search(r'^(?:function )?' + re.escape(name) + r'\(\)\s*\{\n.*?^\}', source, re.M | re.S)
    if not match:
        raise AssertionError(f'Missing function {name}')
    return match.group()


class ProductionRound2(unittest.TestCase):
    setUp = audit.AuditRegressions.setUp
    stub = audit.AuditRegressions.stub
    bash = audit.AuditRegressions.bash
    snapshot_fixture = audit.AuditRegressions.snapshot_fixture
    snapshot_prelude = audit.AuditRegressions.snapshot_prelude

    def success(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_pr05_uninstall_treats_metacharacters_as_literal_paths(self):
        for suffix in ('config[1]', 'config with space', 'config$(touch sentinel)'):
            with self.subTest(suffix=suffix):
                config = self.base / suffix
                intended = config / 'inir'
                intended.mkdir(parents=True)
                (intended / 'valuable.json').write_text('selected config')
                neighbor = self.base / 'config1/inir'
                neighbor.mkdir(parents=True, exist_ok=True)
                (neighbor / 'valuable.json').write_text('unrelated')
                env = dict(self.env, XDG_CONFIG_HOME=str(config), DOTS_CORE_CONFDIR=str(intended))
                result = self.bash('''cd "$HOME" || exit 10
source "$1/sdata/lib/uninstall.sh"
tui_info(){ :; }; tui_success(){ :; }; tui_error(){ :; }
has_other_quickshell_configs(){ return 1; }
backup=$(uninstall_create_backup) || exit 11
[[ -f "$backup/inir/valuable.json" ]] || exit 12
uninstall_remove_inir_only
''', env=env)
                self.success(result)
                self.assertFalse(intended.exists())
                self.assertEqual((neighbor / 'valuable.json').read_text(), 'unrelated')
                self.assertFalse((self.base / 'sentinel').exists())
        self.assertNotIn('eval echo "$path"', (ROOT / 'sdata/lib/uninstall.sh').read_text())

    def backup_code(self):
        return 'source "$1/sdata/lib/functions.sh"\n' + function('sdata/subcmd-install/3.files.sh', 'auto_backup_configs') + '''
cd "$1" || exit 10
ask=false; quiet=true
BACKUP_DIR="$2"
INSTALLED_LISTFILE="$HOME/installed.list"
log_success(){ :; }; log_error(){ :; }
'''

    def test_pr06_space_xdg_backup_precedes_real_directory_install(self):
        config = self.base / 'config with space'
        target = config / 'niri/config.kdl'
        target.parent.mkdir(parents=True)
        target.write_text('valuable old config')
        backup = self.base / 'backup with space'
        result = self.bash(self.backup_code() + '''
auto_backup_configs || exit 11
install_dir__sync "$1/defaults/niri" "$XDG_CONFIG_HOME/niri"
''', backup, env=dict(self.env, XDG_CONFIG_HOME=str(config)))
        self.success(result)
        self.assertEqual((backup / '.config/niri/config.kdl').read_text(), 'valuable old config')
        self.assertEqual(target.read_text(), (ROOT / 'defaults/niri/config.kdl').read_text())

    def test_pr06_failed_backup_stops_install_and_can_be_retried(self):
        target = self.config / 'niri/config.kdl'
        target.write_text('original')
        backup = self.base / 'failed backup'
        source = (ROOT / 'sdata/subcmd-install/3.files.sh').read_text()
        start = source.index('if [[ ! "${SKIP_BACKUP}" == true ]]; then')
        end = source.index('\n#####################################################################################', start)
        gate = source[start:end]
        code = self.backup_code() + '''
SKIP_BACKUP=false
rsync(){ return 23; }
probe(){
''' + gate + '''
printf overwritten > "$XDG_CONFIG_HOME/niri/config.kdl"
}
if probe; then exit 12; fi
'''
        self.success(self.bash(code, backup))
        self.assertEqual(target.read_text(), 'original')
        self.assertFalse(backup.exists())
        self.assertFalse(list(self.base.glob('failed backup.tmp.*')))
        self.success(self.bash(self.backup_code() + '\nauto_backup_configs\n', backup))
        self.assertEqual((backup / '.config/niri/config.kdl').read_text(), 'original')

    def test_pr06_backup_preserves_literal_names_dotfiles_and_ignores(self):
        source, target, backup = (self.base / name for name in ('source', 'target', 'backup'))
        source.mkdir(); target.mkdir()
        for name in ('file [x]', '.hidden', 'ignored'):
            (source / name).write_text('new')
            (target / name).write_text('old ' + name)
        self.success(self.bash('source "$1/sdata/lib/functions.sh"; backup_clashing_targets "$2" "$3" "$4" ignored', source, target, backup))
        self.assertEqual(sorted(p.name for p in backup.iterdir()), ['.hidden', 'file [x]'])
        self.assertEqual((backup / 'file [x]').read_text(), 'old file [x]')

    def update_code(self):
        code = 'REPO_ROOT="$1"\nsource "$1/sdata/lib/versioning.sh"\n'
        code += '\n'.join(function('setup', name) for name in ('_write_update_status', '_report_update_failure', 'run_update'))
        code += '''
_update_status_file="$XDG_STATE_HOME/quickshell/user/update-status"
ask=false; quiet=true; FIRSTRUN_FILE="$XDG_CONFIG_HOME/inir/firstrun"
get_update_tracking_branch(){ echo main; }
check_remote_updates(){ return 1; }
get_repo_commit(){ echo newcommit; }
_create_update_snapshot_or_fail(){ UPDATE_SNAPSHOT_ID=fixture; }
count_pending_migrations(){ echo 0; }
find_runtime_manifest_file(){ return 1; }
detect_user_modifications_by_source_comparison(){ :; }
inir_user_service_is_masked(){ return 1; }
inir_uses_systemd(){ return 1; }
check_dependencies(){ doctor_missing_deps=(); }
python3(){ if [[ "$*" == *'runtime-payload.py copy'* ]]; then return 0; fi; command python3 "$@"; }
'''
        for name in ('generate_manifest', 'cleanup_orphans', 'repair_update_owned_state',
                     'sync_launcher_from_repo', 'sync_user_desktop_integration_from_repo',
                     'refresh_sddm_theme_on_update', 'extras_refresh_yamis_icons_on_update',
                     'install_dir', 'show_session_impact_notices', 'show_update_completion',
                     '_step_phase_start', '_step_phase_header', '_step_phase_done',
                     'tui_success', 'tui_error', 'tui_title', 'tui_warn', 'tui_info', 'tui_dim'):
            code += '\n' + name + '(){ :; }\n'
        code += '''
mkdir -p "$XDG_CONFIG_HOME/quickshell/inir"
printf '// fixture\\n' > "$XDG_CONFIG_HOME/quickshell/inir/shell.qml"
install-python-packages(){ :; }
ensure-ytmusic-js-runtime(){ :; }
set_installed_version 1.0 oldcommit fixture || exit 10
'''
        return code

    def test_pr07_runtime_failure_does_not_advance_version_and_retry_runs_step(self):
        for stage in ('install-python-packages', 'ensure-ytmusic-js-runtime', 'restart_shell_and_verify'):
            with self.subTest(stage=stage):
                code = self.update_code() + '''
NIRI_SOCKET=fixture
restart_shell_and_verify(){ return 0; }
attempt=0
''' + stage + '''(){ attempt=$((attempt+1)); (( attempt > 1 )); }
if run_update; then exit 11; fi
[[ "$(get_installed_commit)" == oldcommit ]] || exit 12
[[ -f "$XDG_STATE_HOME/quickshell/user/update-incomplete" ]] || exit 13
run_update || exit 14
[[ "$attempt" -eq 2 && "$(get_installed_commit)" == newcommit ]] || exit 15
[[ "$(cat "$_update_status_file")" == success ]] || exit 16
[[ ! -e "$XDG_STATE_HOME/quickshell/user/update-incomplete" ]]
'''
                self.success(self.bash(code))

    def test_pr07_retry_handles_equal_commit_and_legacy_failed_status(self):
        for mode in ('repo-copy', 'repo-link'):
            with self.subTest(mode=mode):
                code = self.update_code() + '''
set_installed_version 1.0 newcommit fixture
_write_update_status 'failed:37:old incomplete update'
''' + f'get_installed_install_mode(){{ echo {mode}; }}\n' + '''
ensure-ytmusic-js-runtime(){ touch "$HOME/runtime-retried"; return 1; }
if run_update; then exit 11; fi
[[ -e "$HOME/runtime-retried" && -f "$XDG_STATE_HOME/quickshell/user/update-incomplete" ]]
'''
                self.success(self.bash(code))
                (self.base / 'runtime-retried').unlink()

    def test_pr07_linked_live_head_does_not_replace_completion_revision(self):
        code = self.update_code() + '''
get_installed_install_mode(){ echo repo-link; }
get_installed_commit(){ echo newcommit; }
_write_update_status success
ensure-ytmusic-js-runtime(){ touch "$HOME/runtime-retried"; return 1; }
if run_update; then exit 11; fi
[[ -e "$HOME/runtime-retried" ]]
'''
        self.success(self.bash(code))

    def test_pr07_metadata_failure_keeps_incomplete_marker(self):
        code = self.update_code() + '''
set_installed_version(){ return 1; }
if run_update; then exit 11; fi
[[ -f "$XDG_STATE_HOME/quickshell/user/update-incomplete" ]] || exit 12
[[ "$(cat "$_update_status_file")" == failed:45:* ]]
'''
        self.success(self.bash(code))

    def test_pr08_repository_module_is_bootstrapped_before_enable(self):
        code = function('sdata/dist-gentoo/install-deps.sh', 'gentoo_prepare_guru') + '''
SKIP_SYSUPDATE=true
eselect(){
 [[ -e "$HOME/module" ]] || return 1
 [[ "$*" != 'repository enable guru' ]] || printf enable >> "$HOME/order"
}
gentoo_emerge(){ printf emerge > "$HOME/order"; touch "$HOME/module"; }
v(){ "$@"; }; pkg_sudo(){ "$@"; }; log_error(){ :; }
gentoo_prepare_guru || exit 11
[[ "$(cat "$HOME/order")" == emergeenable ]]
'''
        self.success(self.bash(code))

    def test_pr08_failed_bootstrap_does_not_try_enable(self):
        code = function('sdata/dist-gentoo/install-deps.sh', 'gentoo_prepare_guru') + '''
eselect(){ [[ "$*" != 'repository enable guru' ]] || touch "$HOME/enable"; return 1; }
gentoo_emerge(){ return 1; }
log_error(){ :; }
if gentoo_prepare_guru; then exit 11; fi
[[ ! -e "$HOME/enable" ]]
'''
        self.success(self.bash(code))

    def test_pr09_rejected_systemd_restart_is_a_confirmed_failure(self):
        code = '''source "$1/sdata/lib/robust-update.sh"
inir_uses_systemd(){ return 0; }
systemctl(){ [[ "$*" != *'restart inir.service'* ]]; }
restart_shell_and_verify 1
[[ "$?" -eq 1 ]]
'''
        self.success(self.bash(code))

    def test_pr09_openrc_launcher_failure_is_a_confirmed_failure(self):
        self.stub('inir', '#!/usr/bin/env bash\nexit 1\n')
        (self.repo / 'scripts/inir').unlink()
        self.success(self.bash('source "$1/sdata/lib/robust-update.sh"; inir_uses_systemd(){ return 1; }; restart_shell_and_verify 1; [[ "$?" -eq 1 ]]'))

    def test_pr10_snapshot_restores_payload_revision_and_preserves_checkout_revision(self):
        (self.bin / 'git').unlink()
        env = dict(self.env, GIT_AUTHOR_NAME='Audit', GIT_AUTHOR_EMAIL='audit@example.invalid',
                   GIT_COMMITTER_NAME='Audit', GIT_COMMITTER_EMAIL='audit@example.invalid')
        def git(*args):
            return subprocess.check_output(['git', '-C', str(self.repo), *args], env=env, text=True).strip()
        git('init', '-q')
        for name, content in (('setup', 'fixture'), ('shell.qml', 'old payload'), ('VERSION', '1.0')):
            (self.repo / name).write_text(content)
        git('add', '.'); git('commit', '-qm', 'old')
        old = git('rev-parse', '--short', 'HEAD')
        (self.repo / 'shell.qml').write_text('new payload')
        (self.repo / 'VERSION').write_text('2.0')
        git('add', '.'); git('commit', '-qm', 'new')
        new = git('rev-parse', '--short', 'HEAD')
        runtime = self.config / 'quickshell/inir'
        runtime.mkdir(parents=True)
        (runtime / 'shell.qml').write_text('old payload')
        os.utime(runtime / 'shell.qml', (1000, 1000))
        self.stub('inir', '#!/usr/bin/env bash\n[[ "$*" == "service status" ]] && exit 1\nexit 0\n')
        code = '''source "$1/sdata/lib/versioning.sh"
source "$1/sdata/lib/snapshots.sh"
log_info(){ :; }; log_error(){ printf '%s\\n' "$*" >&2; }; log_warning(){ :; }; tui_success(){ :; }
set_installed_version 1.0 "$2" fixture || exit 10
snapshot=$(create_snapshot update local-sync "$3") || exit 11
jq -e --arg old "$2" --arg new "$3" '.installed_commit_before == $old and .commit_before == $new' "$SNAPSHOTS_DIR/$snapshot/snapshot.json" || exit 12
printf changed > "$XDG_CONFIG_HOME/quickshell/inir/shell.qml"
restore_snapshot "$snapshot" || exit 13
[[ "$(get_installed_commit)" == "$2" ]] || exit 14
[[ "$(git -C "$REPO_ROOT" rev-parse --short HEAD)" == "$3" ]]
'''
        self.success(self.bash(code, old, new, env=env))
        self.assertEqual((runtime / 'shell.qml').read_text(), 'old payload')

    def test_pr10_legacy_snapshots_use_payload_metadata_or_unknown(self):
        self.snapshot_fixture()
        for known in (False, True):
            with self.subTest(known=known):
                runtime = self.config / 'quickshell/inir'
                if known:
                    (runtime / 'version.json').write_text(json.dumps({'commit': 'payload-revision'}))
                code = self.snapshot_prelude() + '''
get_installed_commit(){ echo installed-revision; }
set_installed_version(){ printf '%s' "$2" > "$HOME/restored-revision"; }
snapshot=$(create_snapshot update) || exit 11
metadata="$SNAPSHOTS_DIR/$snapshot/snapshot.json"
jq 'del(.installed_commit_before)' "$metadata" > "$metadata.tmp" || exit 12
mv "$metadata.tmp" "$metadata"
restore_snapshot "$snapshot"
'''
                self.success(self.bash(code))
                self.assertEqual((self.base / 'restored-revision').read_text(), 'payload-revision' if known else 'unknown')

    def test_pr10_corrupt_metadata_is_rejected_before_stopping_shell(self):
        self.snapshot_fixture()
        snapshot = self.base / 'state/quickshell/snapshots/corrupt'
        snapshot.mkdir(parents=True)
        (snapshot / 'snapshot.json').write_text('{broken')
        code = self.snapshot_prelude() + '''
if restore_snapshot corrupt; then exit 11; fi
[[ ! -e "$INIR_AUDIT_CALLS" ]]
'''
        self.success(self.bash(code))
        self.assertEqual((self.config / 'quickshell/inir/shell.qml').read_text(), 'original payload\n')

    def test_pr11_new_backup_survives_future_mtimes_in_both_modes(self):
        source = self.base / 'modified runtime'
        source.mkdir(); (source / 'shell.qml').write_text('modified')
        for mode in ('partial', 'full'):
            with self.subTest(mode=mode):
                backups = self.base / mode
                backups.mkdir()
                for index in range(5):
                    old = backups / f'20990101-{index}'
                    old.mkdir(); os.utime(old, (time.time() + 3600 + index,) * 2)
                call = 'preserve_user_modifications "$3" shell.qml ""' if mode == 'partial' else 'preserve_runtime_tree "$3" runtime'
                code = '''source "$1/sdata/lib/user-modifications.sh"
USER_MODS_DIR="$2"; REPO_ROOT="$1"
''' + 'saved=$(' + call + ') || exit 11\nprintf "%s" "$saved"\n'
                result = self.bash(code, backups, source)
                self.success(result)
                saved = Path(result.stdout.strip())
                self.assertTrue(saved.is_dir())
                self.assertTrue((saved / ('shell.qml' if mode == 'partial' else 'runtime/shell.qml')).is_file())
                self.assertEqual(len(list(backups.iterdir())), 5)

    def test_pr12_flatpak_removal_is_blocked_during_refresh(self):
        source = (ROOT / 'services/AppCatalog.qml').read_text()
        def js(name):
            start = source.index('    function ' + name + '(')
            end = source.index('\n    }', start) + 6
            return re.sub(r':\s*(?:string|bool|void|var|list<var>)', '', source[start:end])
        firefox = next(app for app in json.loads((ROOT / 'defaults/app-catalog.json').read_text()) if app['id'] == 'firefox')
        script = js('_refreshInstalled') + '\n' + js('removeApp') + '''
const calls=[];
const root={catalog:[APP], _detectedPm:'gentoo', _flatpakAvailable:true,
 installedPackages:{firefox:true}, _installedSources:{firefox:'flatpak'},
 _runTerminalScript:(script,args)=>calls.push({script,args})};
const _installedProc={running:false}, _refreshTimer={restart(){}};
_refreshInstalled();
if(root._installedSources.firefox!=='flatpak') throw Error('source lost');
if(removeApp('firefox') || calls.length) throw Error('action allowed during refresh');
root.checkingInstalled=false;
if(!removeApp('firefox')) throw Error('completed refresh prevents action');
console.log(JSON.stringify(calls));
'''.replace('APP', json.dumps(firefox))
        result = subprocess.run(['node'], input=script, capture_output=True, text=True, timeout=10)
        self.success(result)
        self.assertEqual(json.loads(result.stdout), [{'script': 'flatpak uninstall -y "$1"', 'args': ['org.mozilla.firefox']}])
        self.assertIn('enabled: !AppCatalog.checkingInstalled', (ROOT / 'modules/sidebarLeft/SoftwareView.qml').read_text())

    def test_pr13_locale_and_pactl_failure_preserve_pulseaudio(self):
        for failure in (False, True):
            with self.subTest(failure=failure):
                code = function('sdata/subcmd-install/2.setups.sh', 'setup_systemd_audio_services') + '''
OS_GROUP_ID=gentoo
pipewire(){ :; }
pactl(){ ''' + ('return 1;' if failure else '''[[ "$LC_ALL" == C ]] && echo 'Server Name: pulseaudio' || echo 'Имя сервера: pulseaudio';''') + ''' }
pgrep(){ return 0; }
systemctl(){ printf '%s\\n' "$*" >> "$HOME/audio-calls"; }
log_info(){ :; }; log_warning(){ :; }; v(){ "$@"; }
setup_systemd_audio_services
[[ ! -e "$HOME/audio-calls" ]]
'''
                self.success(self.bash(code))

    def test_pr14_cleanup_only_signals_service_or_matching_scopes(self):
        launcher = function('scripts/inir', 'cleanup_orphans')
        phase = launcher[launcher.index('    # Phase 6:'):launcher.rindex('\n}')]
        process = subprocess.Popen(['sleep', '30'])
        self.addCleanup(lambda: (process.terminate(), process.wait()) if process.poll() is None else None)
        prefix = 'inir-thumbnails-' + hashlib.sha256(str(self.repo.resolve()).encode()).hexdigest() + '-'
        for group, expected in (('/user.slice/inir.service', True),
                                ('/user.slice/' + prefix + '123.scope', True),
                                ('/user.slice/inir-thumbnails-other-123.scope', False),
                                ('/user.slice/unrelated.scope', False)):
            with self.subTest(group=group):
                log = self.base / 'signals'; log.unlink(missing_ok=True)
                code = '''source "$1/scripts/lib/session-backend.sh"
config_dir="$REPO_ROOT"
cat(){ if [[ "$1" == /proc/*/cgroup ]]; then printf '%s' "$GROUP"; else /bin/cat "$@"; fi; }
pgrep(){ printf '%s\\n' "$PID"; }
kill(){ printf '%s\\n' "$*" >> "$HOME/signals"; }
sleep(){ :; }
probe(){
''' + phase + '\n}\nprobe\n'
                self.success(self.bash(code, env=dict(self.env, PID=str(process.pid), GROUP=group)))
                calls = log.read_text().splitlines() if log.exists() else []
                self.assertEqual(calls, [str(process.pid), '-KILL ' + str(process.pid)] if expected else [])

    def test_pr14_wrapper_uses_systemd_only_on_systemd(self):
        venv = self.base / 'venv with space'; (venv / 'bin').mkdir(parents=True)
        python = venv / 'bin/python3'
        python.write_text('#!/usr/bin/env bash\nprintf direct-python\n'); python.chmod(0o755)
        self.stub('systemd-run', '#!/usr/bin/env bash\nprintf "scope %s" "$*"\n')
        for backend in ('openrc', 'systemd'):
            result = subprocess.run(['bash', str(ROOT / 'scripts/thumbnails/thumbgen-venv.sh'), '--help'],
                                    env=dict(self.env, INIR_INIT_SYSTEM=backend, INIR_VENV=str(venv)), capture_output=True, text=True, timeout=10)
            self.success(result)
            self.assertIn('direct-python' if backend == 'openrc' else 'scope --user', result.stdout)

    def test_pr14_cleanup_does_not_kill_a_reused_pid(self):
        launcher = function('scripts/inir', 'cleanup_orphans')
        phase = launcher[launcher.index('    # Phase 6:'):launcher.rindex('\n}')]
        process = subprocess.Popen(['sleep', '30'])
        self.addCleanup(lambda: (process.terminate(), process.wait()) if process.poll() is None else None)
        code = '''source "$1/scripts/lib/session-backend.sh"
config_dir="$REPO_ROOT"
cat(){ printf '0::/user.slice/inir.service'; }
pgrep(){ printf '%s\\n' "$PID"; }
awk(){ if [[ -f "$HOME/pid-observed" ]]; then echo 200; else touch "$HOME/pid-observed"; echo 100; fi; }
kill(){ printf '%s\\n' "$*" >> "$HOME/signals"; }
sleep(){ :; }
probe(){
''' + phase + '\n}\nprobe\n'
        self.success(self.bash(code, env=dict(self.env, PID=str(process.pid))))
        self.assertEqual((self.base / 'signals').read_text().splitlines(), [str(process.pid)])

    def test_pr14_service_wrapper_assigns_a_config_specific_scope(self):
        venv = self.base / 'scope-venv'; (venv / 'bin').mkdir(parents=True)
        self.stub('cat', '#!/usr/bin/env bash\nprintf "0::/user.slice/inir.service"\n')
        self.stub('systemd-run', '#!/usr/bin/env bash\nprintf "%s\\n" "$@"\n')
        result = subprocess.run(['bash', str(ROOT / 'scripts/thumbnails/thumbgen-venv.sh'), '--help'],
                                env=dict(self.env, INIR_INIT_SYSTEM='systemd', INIR_VENV=str(venv)),
                                capture_output=True, text=True, timeout=10)
        self.success(result)
        digest = hashlib.sha256(str(ROOT.resolve()).encode()).hexdigest()
        self.assertRegex(result.stdout, '--unit=inir-thumbnails-' + digest + r'-[0-9]+')

    def test_pr10_incomplete_update_snapshot_has_unknown_payload_revision(self):
        self.snapshot_fixture()
        marker = self.base / 'state/quickshell/user/update-incomplete'
        marker.parent.mkdir(parents=True); marker.write_text('candidate')
        code = self.snapshot_prelude() + '''
get_installed_commit(){ echo last-completed; }
snapshot=$(create_snapshot update) || exit 11
jq -e '.installed_commit_before == "unknown"' "$SNAPSHOTS_DIR/$snapshot/snapshot.json"
'''
        self.success(self.bash(code))

    def test_pr15_manual_use_flags_match_automatic_runtime_flags(self):
        manual = re.search(r'^gui-apps/quickshell (sockets .+)$', (ROOT / 'docs/GENTOO.md').read_text(), re.M)[1]
        automatic = re.search(r"'gui-apps/quickshell ([^']+)'", (ROOT / 'sdata/dist-gentoo/install-deps.sh').read_text())[1]
        self.assertEqual(set(manual.split()), set(automatic.split()))
        self.assertIn('screencopy', manual.split())

    def test_pr16_portage_merge_detected_with_python_entrypoint(self):
        for argv in (['emerge', '--update', '@world'],
                     ['/usr/bin/python3.14', '/usr/lib/python-exec/python3.14/emerge', '-avuDN', '@world'],
                     ['/usr/bin/python3', '/usr/bin/emerge', '--depclean'],
                     ['emerge', '--pretend=n', 'category/package']):
            with self.subTest(argv=argv):
                self.assertTrue(warning.package_operation(argv))

    def test_pr16_portage_queries_and_pretend_are_not_merge_warnings(self):
        for arg in ('--search', '--searchdesc', '--info', '--version', '--help', '--list-sets', '--pretend', '-pv', '-s'):
            with self.subTest(arg=arg):
                self.assertFalse(warning.package_operation(['/usr/bin/python3', '/usr/bin/emerge', arg, 'firefox']))
        self.assertFalse(warning.package_operation(['python3', '/tmp/unrelated.py', 'emerge']))
        self.assertTrue(warning.package_operation(['/usr/bin/pacman', '-Syu']))

    def test_pr16_proc_scan_handles_exited_processes_and_qml_uses_helper(self):
        proc = self.base / 'proc'; proc.mkdir()
        (proc / '12').mkdir(); (proc / 'self').mkdir()
        (proc / '13').mkdir(); (proc / '13/cmdline').write_bytes(b'/usr/bin/python3\0/usr/bin/emerge\0--search\0firefox\0')
        self.assertFalse(warning.package_manager_running(proc))
        (proc / '14').mkdir(); (proc / '14/cmdline').write_bytes(b'/usr/bin/python3\0/usr/bin/emerge\0--update\0@world\0')
        self.assertTrue(warning.package_manager_running(proc))
        self.assertIn('Quickshell.shellPath("scripts/package-manager-running.py")', (ROOT / 'services/deferred/SessionWarnings.qml').read_text())


if __name__ == '__main__':
    unittest.main(verbosity=2)
