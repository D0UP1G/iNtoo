#!/usr/bin/env python3
"""Maintain compositor-owned startup without replacing the user's Niri config."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


_KDL_PATH = Path(__file__).resolve().with_name("niri_kdl.py")
_KDL_SPEC = importlib.util.spec_from_file_location("inir_niri_kdl", _KDL_PATH)
kdl = importlib.util.module_from_spec(_KDL_SPEC)
sys.modules[_KDL_SPEC.name] = kdl
_KDL_SPEC.loader.exec_module(kdl)

SHELL_MARKER = "// iNToo OpenRC startup (managed by setup)"
AUDIO_MARKER = "// iNToo OpenRC audio (managed by setup)"
NIRI_INCLUDE_DEPTH_LIMIT = 10


class CommandMatcher:
    def __init__(self, executable, argument_sets):
        self.executable = executable
        self.argument_sets = argument_sets

    def match(self, node):
        if node.disabled or node.name not in ("spawn-at-startup", "spawn-sh-at-startup"):
            return False
        active_args = [token for token in node.args if not token.disabled]
        if any(token.kind != "string" for token in active_args):
            return False
        words = [token.value for token in active_args]
        return bool(words) and Path(words[0]).name == self.executable and words[1:] in self.argument_sets


SHELL = CommandMatcher("inir", [["start"], ["run"], ["run", "--session"]])
AUDIO = CommandMatcher("gentoo-pipewire-launcher", [[]])


def _active_nodes(text):
    return list(kdl.nodes(text))


def _include_values(node):
    value = kdl.include_path(node)
    return [] if value is None else [value]


def config_files(root):
    files = []
    seen_physical = set()
    seen_logical = {}

    def visit(path, depth=0):
        if depth >= NIRI_INCLUDE_DEPTH_LIMIT:
            return
        # Keep `..` components intact: the OS resolves them after traversing
        # symlink directories, and normalizing them early changes the target.
        logical_path = path.expanduser().absolute()
        logical_key = str(logical_path)
        if seen_logical.get(logical_key, NIRI_INCLUDE_DEPTH_LIMIT) <= depth:
            return
        try:
            physical_path = logical_path.resolve(strict=True)
        except (OSError, RuntimeError):
            return
        if not physical_path.is_file():
            return
        seen_logical[logical_key] = depth
        if physical_path not in seen_physical:
            seen_physical.add(physical_path)
            files.append(physical_path)
        for node in _active_nodes(physical_path.read_text(encoding="utf-8")):
            for include in _include_values(node):
                candidate = Path(include).expanduser()
                if not candidate.is_absolute():
                    # Niri resolves a nested relative include beside the path
                    # used to include this file, even when that path is a
                    # symlink. Keep that logical parent for graph traversal.
                    candidate = logical_path.parent / candidate
                # Niri accepts globbed include paths. Sorting keeps management
                # behavior deterministic while preserving the active graph.
                import glob
                matches = glob.glob(str(candidate))
                for match in matches or [str(candidate)]:
                    visit(Path(match), depth + 1)

    visit(Path(root))
    return files


def _replace_node(text, node):
    """Remove a whole statement line when possible, otherwise only its span."""
    line_start = text.rfind("\n", 0, node.start) + 1
    line_end = text.find("\n", node.end)
    line_end = len(text) if line_end < 0 else line_end + 1
    prefix = text[line_start:node.start]
    suffix = text[node.end:line_end]
    if not prefix.strip() and not kdl.mask_comments(suffix).strip(" \t\r\n"):
        return text[:line_start] + text[line_end:]
    return text[:node.start] + kdl.clear_span(text, node.start, node.end) + text[node.end:]


def _remove_nodes(text, remove):
    for node in reversed(remove):
        text = _replace_node(text, node)
    return text


def _validate_candidate(config_home, root, updated):
    validator = shutil.which("niri")
    if not validator:
        return
    niri_dir = Path(config_home) / "niri"
    # Niri resolves relative includes relative to each source file. Staging only
    # the config tree changes that meaning for external files, so reject edits
    # whose active graph leaves the staged tree. This is safer than validating
    # a graph different from the one that would be published.
    base = niri_dir.resolve()
    config_base = Path(config_home).resolve()
    # Absolute symlinks would keep resolving into the live tree after copying.
    # Relative symlinks are safe only when their targets are part of the active
    # graph and can be copied at the same relative location below.
    for candidate in [niri_dir, *niri_dir.rglob("*")]:
        if candidate.is_symlink() and os.path.isabs(os.readlink(candidate)):
            raise ValueError(f"Cannot safely validate startup changes with absolute symlink: {candidate}")
    for path in updated:
        try:
            path.resolve().relative_to(base)
        except ValueError as error:
            raise ValueError(f"Cannot safely validate startup changes with external include: {path}") from error
    with tempfile.TemporaryDirectory(prefix="inir-niri-candidate-", dir=config_home) as temp:
        stage_home = Path(temp) / "config"
        stage_home.mkdir()
        candidate_root = stage_home / "niri"
        shutil.copytree(niri_dir, candidate_root, symlinks=True)
        # Preserve the active graph's relative directories. In particular, a
        # symlinked include may point to ../shared/file.kdl and Niri resolves
        # that file's children relative to its logical include path.
        for path in config_files(root):
            try:
                relative = path.resolve().relative_to(config_base)
            except ValueError as error:
                raise ValueError(f"Cannot safely validate startup changes with external include: {path}") from error
            if path.resolve().is_relative_to(base):
                continue
            staged_source = stage_home / relative
            staged_source.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, staged_source)
        for path, contents in updated.items():
            relative = path.resolve().relative_to(base)
            staged = candidate_root / relative
            staged.parent.mkdir(parents=True, exist_ok=True)
            staged.write_text(contents, encoding="utf-8")
        # Absolute paths into the live config tree would bypass staging. Refuse
        # them explicitly; relative paths preserve their resolution naturally.
        for path in config_files(root):
            text = updated.get(path, path.read_text(encoding="utf-8"))
            for node in _active_nodes(text):
                value = kdl.include_path(node)
                if value and Path(value).is_absolute():
                    try:
                        Path(value).resolve(strict=False).relative_to(base)
                    except ValueError:
                        raise ValueError(f"Cannot safely validate startup changes with external include: {value}")
                    raise ValueError(f"Cannot safely validate absolute include during staged startup update: {value}")
        result = subprocess.run([validator, "validate", "-c", str(candidate_root / Path(root).name)],
                                capture_output=True, text=True)
        if result.returncode:
            original = subprocess.run([validator, "validate", "-c", str(root)],
                                      capture_output=True, text=True)
            if original.returncode:
                print("Niri config already fails validation; startup edit was not independently validated", file=sys.stderr)
                return
            raise ValueError((result.stderr or result.stdout).strip() or "niri rejected the candidate configuration")


def atomic_write(path, text):
    name = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, prefix=".inir-startup-",
                                         encoding="utf-8", delete=False) as file:
            name = Path(file.name)
            file.write(text)
            file.flush()
            os.fsync(file.fileno())
        name.chmod(path.stat().st_mode & 0o777)
        name.replace(path)
    finally:
        if name is not None:
            name.unlink(missing_ok=True)


def publish_transaction(changed):
    """Publish a set of files with durable backups for failed rollback."""
    backups = {}
    published = []
    rollback_failures = []
    retained_backups = set()
    try:
        for path in changed:
            mode = path.stat().st_mode & 0o777
            fd, backup_name = tempfile.mkstemp(dir=path.parent, prefix=".inir-startup-backup-")
            backup = Path(backup_name)
            try:
                with os.fdopen(fd, "wb") as stream:
                    stream.write(path.read_bytes())
                    stream.flush()
                    os.fsync(stream.fileno())
                backup.chmod(mode)
            except Exception:
                backup.unlink(missing_ok=True)
                raise
            backups[path] = (backup, mode)
        for path, text in changed.items():
            atomic_write(path, text)
            published.append(path)
    except Exception as error:
        for path in reversed(published):
            backup, _mode = backups[path]
            try:
                os.replace(backup, path)
            except Exception as rollback_error:
                retained_backups.add(backup)
                rollback_failures.append(f"{path} (backup: {backup}): {rollback_error}")
        if rollback_failures:
            raise OSError(f"startup publish failed ({error}); rollback also failed; retained backups: {'; '.join(rollback_failures)}") from error
        raise
    finally:
        for backup, _mode in backups.values():
            if backup not in retained_backups:
                backup.unlink(missing_ok=True)


def sync(config_home, launcher, action, pipewire=False):
    config_home = Path(config_home)
    root = config_home / "niri/config.kdl"
    files = config_files(root)
    if not files:
        raise FileNotFoundError(f"Niri config missing or unreadable: {root}")
    contents = {path: path.read_text(encoding="utf-8") for path in files}
    parsed = {path: _active_nodes(text) for path, text in contents.items()}

    if action == "status":
        return any(SHELL.match(node) for nodes in parsed.values() for node in nodes)

    updated = {}
    audio_present = False
    for path, text in contents.items():
        nodes = parsed[path]
        remove = [node for node in nodes if SHELL.match(node)]
        managed_audio_nodes = []
        for node in nodes:
            if not AUDIO.match(node):
                continue
            previous = text[:node.start].rstrip()
            marker_line = previous.splitlines()[-1].strip() if previous else ""
            if marker_line == AUDIO_MARKER:
                managed_audio_nodes.append(node)
        if any(AUDIO.match(node) and node not in managed_audio_nodes for node in nodes):
            audio_present = True
        remove.extend(managed_audio_nodes)
        text = _remove_nodes(text, sorted(remove, key=lambda item: item.start))

        # Remove only the markers that setup owns. Their matching command has
        # already been removed above; hand-written startup/audio stays intact.
        kept_lines = []
        for line in text.splitlines(keepends=True):
            if line.strip() in (SHELL_MARKER, AUDIO_MARKER):
                if kept_lines and not kept_lines[-1].strip():
                    kept_lines.pop()
                continue
            kept_lines.append(line)
        updated[path] = "".join(kept_lines)

    if action == "enable":
        target = next((path for path in files if path.name == "50-startup.kdl"), files[0])
        if updated[target] and not updated[target].endswith("\n"):
            updated[target] += "\n"
        updated[target] += (f'\n{SHELL_MARKER}\n'
                            f'spawn-at-startup {json.dumps(str(launcher), ensure_ascii=False)} '
                            '"run" "--session"\n')
        if pipewire and not audio_present:
            updated[target] += (f'\n{AUDIO_MARKER}\n'
                                'spawn-at-startup "gentoo-pipewire-launcher"\n')

    changed = {path: text for path, text in updated.items() if text != contents[path]}
    if changed:
        _validate_candidate(config_home, root, changed)
        publish_transaction(changed)
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("enable", "disable", "status"))
    parser.add_argument("--config-home", required=True)
    parser.add_argument("--launcher", required=True)
    parser.add_argument("--pipewire", action="store_true")
    args = parser.parse_args()
    try:
        return 0 if sync(args.config_home, args.launcher, args.action, args.pipewire) else 1
    except (OSError, UnicodeError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
