#!/usr/bin/env python3
"""Exit 0 while a package manager operation is active (including Python emerge)."""
from pathlib import Path
import sys

MANAGERS = {
    "pacman", "yay", "paru", "dnf", "zypper", "apt", "apx", "xbps",
    "flatpak", "snap", "apk", "yum", "epsi", "pikman",
}
PORTAGE_QUERIES = {
    "--search", "--searchdesc", "--info", "--version", "--help", "--list-sets",
}


def package_operation(argv):
    if not argv:
        return False
    executable = Path(argv[0]).name
    index = 0
    # Python scripts have the interpreter as argv[0] and the script as argv[1].
    if executable.startswith("python") and len(argv) > 1:
        index = 1
        executable = Path(argv[index]).name
    if executable in MANAGERS:
        return True
    if executable != "emerge":
        return False
    args = argv[index + 1:]
    for arg in args:
        if arg in PORTAGE_QUERIES or arg in ("--pretend", "--pretend=y", "--pretend=yes"):
            return False
        if arg.startswith("-") and not arg.startswith("--"):
            if any(flag in arg[1:] for flag in "psShV"):
                return False
    return True


def package_manager_running(proc_root=Path("/proc")):
    for entry in proc_root.iterdir():
        if not entry.name.isdecimal():
            continue
        try:
            argv = entry.joinpath("cmdline").read_bytes().split(b"\0")
            args = [arg.decode(errors="surrogateescape") for arg in argv if arg]
        except (OSError, ProcessLookupError):
            continue
        if package_operation(args):
            return True
    return False


if __name__ == "__main__":
    sys.exit(0 if package_manager_running() else 1)
