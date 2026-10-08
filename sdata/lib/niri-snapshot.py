#!/usr/bin/env python3
"""Capture and restore the active Niri include files outside config/niri."""
import argparse
import glob
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[2]
KDL_PATH = ROOT / "scripts/lib/niri_kdl.py"
SPEC = importlib.util.spec_from_file_location("inir_niri_kdl", KDL_PATH)
kdl = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = kdl
SPEC.loader.exec_module(kdl)
NIRI_INCLUDE_DEPTH_LIMIT = 10


def include_graph(config_dir):
    config_dir = Path(config_dir)
    root = config_dir / "config.kdl"
    base_real = config_dir.resolve()
    visited_logical = {}
    external = {}

    def visit(path, depth=0):
        if depth >= NIRI_INCLUDE_DEPTH_LIMIT:
            return
        # Preserve `..` until the filesystem resolves symlink components.
        path = Path(path).expanduser().absolute()
        logical_key = str(path)
        if visited_logical.get(logical_key, NIRI_INCLUDE_DEPTH_LIMIT) <= depth:
            return
        try:
            resolved = path.resolve(strict=True)
        except (OSError, RuntimeError) as error:
            raise ValueError(f"Cannot resolve active Niri include {path}: {error}") from error
        if not resolved.is_file():
            raise ValueError(f"Active Niri include is not a regular file: {resolved}")
        visited_logical[logical_key] = depth
        try:
            content = resolved.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as error:
            raise ValueError(f"Cannot read active Niri include {resolved}: {error}") from error

        try:
            nodes = kdl.nodes(content)
            for node in nodes:
                if node.name != "include" or node.disabled:
                    continue
                include_value = kdl.include_path(node)
                if include_value is None:
                    raise ValueError(f"Unsupported Niri include syntax in {resolved}")
                include = Path(include_value).expanduser()
                if not include.is_absolute():
                    include = path.parent / include
                matches = sorted(glob.glob(str(include)))
                # A missing optional/glob include contributes no active file.
                for matched in matches:
                    visit(Path(matched), depth + 1)
        except (OSError, UnicodeError, ValueError) as error:
            raise ValueError(f"Cannot inspect active Niri include graph at {resolved}: {error}") from error

        try:
            resolved.relative_to(base_real)
        except ValueError:
            external[resolved] = resolved

    if root.exists():
        visit(root)
    return sorted(external)


def collect(config_dir, archive_dir, manifest_path):
    archive_dir = Path(archive_dir)
    archive_dir.mkdir(parents=True, exist_ok=True)
    paths = include_graph(config_dir)
    entries = []
    for index, path in enumerate(paths):
        info = path.stat()
        if not stat.S_ISREG(info.st_mode):
            raise ValueError(f"External Niri include is not a regular file: {path}")
        relative = Path("files") / f"{index:06d}.kdl"
        destination = archive_dir / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, destination)
        parent_chain = []
        current = path.parent
        while True:
            info_parent = current.stat()
            parent_chain.append({"path": str(current), "device": info_parent.st_dev,
                                 "inode": info_parent.st_ino})
            if current.parent == current:
                break
            current = current.parent
        entries.append({"path": str(path), "archive": relative.as_posix(),
                        "parent_chain": parent_chain,
                        "mode": stat.S_IMODE(info.st_mode)})
    manifest = {"version": 1, "external_files": entries}
    Path(manifest_path).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def load_manifest(manifest_path, archive_dir):
    manifest = json.loads(Path(manifest_path).read_text(encoding="utf-8"))
    if manifest.get("version") != 1 or not isinstance(manifest.get("external_files"), list):
        raise ValueError("Unsupported Niri include manifest")
    archive_root = Path(archive_dir).resolve()
    targets = set()
    entries = []
    for item in manifest["external_files"]:
        target = Path(item.get("path", ""))
        archive = (Path(archive_dir) / item.get("archive", "")).resolve()
        try:
            archive.relative_to(archive_root)
        except ValueError as error:
            raise ValueError("Niri include manifest contains an invalid archive path") from error
        if not target.is_absolute() or target in targets:
            raise ValueError("Niri include manifest contains an invalid or duplicate target")
        if not archive.is_file():
            raise ValueError(f"Snapshot is missing external Niri include data: {archive}")
        targets.add(target)
        chain = item.get("parent_chain")
        if not isinstance(chain, list) or not chain:
            # Legacy manifests remain readable, but the currently resolved
            # parent identity is captured during preflight and checked again.
            chain = None
        entries.append((target, archive, int(item.get("mode", 0o600)) & 0o777, chain))
    return entries


def preflight_restore(entries):
    identities = {}
    for target, _archive, _mode, chain in entries:
        if not target.parent.is_dir():
            raise ValueError(f"External Niri include directory is missing: {target.parent}")
        resolved_parent = target.parent.resolve(strict=True)
        if resolved_parent != target.parent.parent.resolve(strict=True) / target.parent.name:
            raise ValueError(f"External Niri include directory changed identity: {target.parent}")
        if target.is_symlink() or (target.exists() and not target.is_file()):
            raise ValueError(f"External Niri include target changed type: {target}")
        if not os.access(resolved_parent, os.W_OK):
            raise ValueError(f"External Niri include directory is not writable: {resolved_parent}")
        if chain:
            for record in chain:
                directory = Path(record["path"])
                try:
                    info = directory.stat()
                except OSError as error:
                    raise ValueError(f"External Niri include directory changed: {directory}") from error
                if (info.st_dev, info.st_ino) != (int(record["device"]), int(record["inode"])):
                    raise ValueError(f"External Niri include directory changed identity: {directory}")
        identities[target] = (resolved_parent, resolved_parent.stat().st_dev, resolved_parent.stat().st_ino)
    return identities


def restore(manifest_path, archive_dir):
    entries = load_manifest(manifest_path, archive_dir)
    identities = preflight_restore(entries)
    staged = []
    backups = []
    replaced = []
    rollback_failures = []
    try:
        for target, archive, mode, _chain in entries:
            parent, device, inode = identities[target]
            # Resolve and validate immediately before creating anything in the
            # directory; a replaced parent symlink must never redirect restore.
            current_parent = target.parent.resolve(strict=True)
            current_info = current_parent.stat()
            if (current_parent, current_info.st_dev, current_info.st_ino) != (parent, device, inode):
                raise ValueError(f"External Niri include directory changed identity: {target.parent}")
            with tempfile.NamedTemporaryFile(dir=parent, prefix=".inir-niri-restore-",
                                             delete=False) as stream:
                temporary = Path(stream.name)
                with archive.open("rb") as source:
                    shutil.copyfileobj(source, stream)
                stream.flush()
                os.fsync(stream.fileno())
            temporary.chmod(mode)
            staged.append((target, temporary))
            actual_target = parent / target.name
            if actual_target.is_symlink() or (actual_target.exists() and not actual_target.is_file()):
                raise ValueError(f"External Niri include target changed type: {actual_target}")
            if actual_target.exists():
                fd, backup_name = tempfile.mkstemp(dir=parent, prefix=".inir-niri-backup-")
                os.close(fd)
                backup = Path(backup_name)
                shutil.copy2(actual_target, backup)
                backups.append((actual_target, backup))
            else:
                backups.append((actual_target, None))
        # Recheck every parent immediately before publication and use its
        # resolved directory path as the os.replace destination.
        for target, temporary in staged:
            parent, device, inode = identities[target]
            info = parent.stat()
            if (info.st_dev, info.st_ino) != (device, inode):
                raise ValueError(f"External Niri include directory changed identity: {parent}")
            actual_target = parent / target.name
            if actual_target.is_symlink() or (actual_target.exists() and not actual_target.is_file()):
                raise ValueError(f"External Niri include target changed type: {actual_target}")
            os.replace(temporary, actual_target)
            replaced.append(actual_target)
    except Exception as error:
        backup_map = dict(backups)
        for target in reversed(replaced):
            backup = backup_map[target]
            try:
                if backup is None:
                    target.unlink(missing_ok=True)
                else:
                    os.replace(backup, target)
            except Exception as rollback_error:
                rollback_failures.append((target, backup, rollback_error))
        if rollback_failures:
            details = "; ".join(
                f"{target}: restore original from retained backup {backup} ({failure})"
                for target, backup, failure in rollback_failures)
            raise OSError(f"Niri snapshot restore failed ({error}); rollback also failed: {details}") from error
        raise
    finally:
        for _target, temporary in staged:
            temporary.unlink(missing_ok=True)
        # Keep any backup that could not be moved back into place. Do not erase
        # the last copy of live user data while handling a failed rollback.
        retain = {backup for _target, backup, _failure in rollback_failures if backup is not None}
        for _target, backup in backups:
            if backup is not None and backup not in retain:
                backup.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="action", required=True)
    capture = subparsers.add_parser("capture")
    capture.add_argument("config_dir")
    capture.add_argument("archive_dir")
    capture.add_argument("manifest")
    check = subparsers.add_parser("has-external")
    check.add_argument("config_dir")
    restore_parser = subparsers.add_parser("restore")
    restore_parser.add_argument("manifest")
    restore_parser.add_argument("archive_dir")
    check_restore = subparsers.add_parser("check-restore")
    check_restore.add_argument("manifest")
    check_restore.add_argument("archive_dir")
    args = parser.parse_args()
    try:
        if args.action == "capture":
            collect(args.config_dir, args.archive_dir, args.manifest)
        elif args.action == "has-external":
            return 0 if include_graph(args.config_dir) else 1
        elif args.action == "restore":
            restore(args.manifest, args.archive_dir)
        else:
            preflight_restore(load_manifest(args.manifest, args.archive_dir))
        return 0
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as error:
        print(error, file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
