MIGRATION_ID="018-modularize-niri-config"
MIGRATION_TITLE="Modularize Niri config"
MIGRATION_DESCRIPTION="Splits a legacy monolithic Niri config.kdl into config.d fragments while preserving the current user configuration."
MIGRATION_TARGET_FILE="~/.config/niri/config.kdl"
MIGRATION_REQUIRED=true

migration_check() {
  local config="${XDG_CONFIG_HOME:-$HOME/.config}/niri/config.kdl"
  [[ -f "$config" ]] || return 1
  grep -q 'include "config\.d/' "$config" && return 1
  grep -qE '^[[:space:]]*include[[:space:]]+"[^"]+"' "$config" && return 1
  return 0
}

migration_preview() {
  echo -e "${STY_RED}- ~/.config/niri/config.kdl as one monolithic file${STY_RST}"
  echo -e "${STY_GREEN}+ ~/.config/niri/config.kdl with config.d/10..90 fragments${STY_RST}"
}

migration_apply() {
  local config="${XDG_CONFIG_HOME:-$HOME/.config}/niri/config.kdl"

  if ! migration_check; then
    return 0
  fi

  export INIR_MIGRATION_NIRI_CONFIG="$config"
  export INIR_MIGRATION_LAUNCHER_PATH="${XDG_BIN_HOME:-$HOME/.local/bin}/inir"
  python3 << 'MIGRATE'
import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

config_path = Path(os.environ["INIR_MIGRATION_NIRI_CONFIG"]).expanduser()
launcher_path = os.environ["INIR_MIGRATION_LAUNCHER_PATH"]
niri_dir = config_path.parent
modular_dir = niri_dir / "config.d"

content = config_path.read_text(encoding="utf-8")
if 'include "config.d/' in content:
    raise SystemExit(0)

lines = content.splitlines(keepends=True)


def kdl_string_end(text, start):
    """Return the end offset of a normal or raw KDL string at start."""
    if text[start] == '"':
        i = start + 1
        escaped = False
        while i < len(text):
            char = text[i]
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                return i + 1
            i += 1
        return len(text)

    raw_r_prefix = text[start] == "r"
    prefix_end = start + (1 if raw_r_prefix else 0)
    if prefix_end >= len(text) or (text[prefix_end] != "#" and not
                                   (raw_r_prefix and text[prefix_end] == '"')):
        return None
    while prefix_end < len(text) and text[prefix_end] == "#":
        prefix_end += 1
    hash_count = prefix_end - start - (1 if text[start] == "r" else 0)
    if (hash_count == 0 and not raw_r_prefix) or prefix_end >= len(text) or text[prefix_end] != '"':
        return None
    closing = '"' + ("#" * hash_count)
    closing_start = text.find(closing, prefix_end + 1)
    return len(text) if closing_start < 0 else closing_start + len(closing)


def mask_kdl_comments(text):
    """Blank comments while retaining strings, line endings and source offsets."""
    chars = list(text)
    block_depth = 0
    block_start_line = None
    multiline_comment_ends = {}
    line_comment = False
    i = 0
    while i < len(text):
        char = text[i]
        next_char = text[i + 1] if i + 1 < len(text) else ""
        if line_comment:
            if char == "\n":
                line_comment = False
            else:
                chars[i] = " "
            i += 1
            continue
        if block_depth:
            if char == "/" and next_char == "*":
                chars[i] = chars[i + 1] = " "
                if block_depth == 0:
                    block_start_line = text.count("\n", 0, i)
                block_depth += 1
                i += 2
                continue
            if char == "*" and next_char == "/":
                chars[i] = chars[i + 1] = " "
                block_depth -= 1
                if block_depth == 0 and block_start_line is not None:
                    end_line = text.count("\n", 0, i)
                    if end_line > block_start_line:
                        multiline_comment_ends[block_start_line] = max(
                            end_line, multiline_comment_ends.get(block_start_line, -1)
                        )
                    block_start_line = None
                i += 2
                continue
            if char != "\n":
                chars[i] = " "
            i += 1
            continue
        string_end = kdl_string_end(text, i)
        if string_end is not None:
            i = string_end
            continue
        if char == "/" and next_char == "/":
            chars[i] = chars[i + 1] = " "
            line_comment = True
            i += 2
            continue
        if char == "/" and next_char == "*":
            chars[i] = chars[i + 1] = " "
            block_start_line = text.count("\n", 0, i)
            block_depth = 1
            i += 2
            continue
        i += 1
    return "".join(chars), multiline_comment_ends


def mask_kdl_strings(text):
    """Blank string contents for structural brace counting only."""
    chars = list(text)
    i = 0
    while i < len(text):
        string_end = kdl_string_end(text, i)
        if string_end is None:
            i += 1
            continue
        for offset in range(i, string_end):
            if chars[offset] != "\n":
                chars[offset] = " "
        i = string_end
    return "".join(chars)


comment_masked, multiline_comment_ends = mask_kdl_comments(content)
active_lines = comment_masked.splitlines(keepends=True)
depth_lines = mask_kdl_strings(comment_masked).splitlines(keepends=True)
statement_continuation_ends = {}
scan_offset = 0
while scan_offset < len(comment_masked):
    string_end = kdl_string_end(comment_masked, scan_offset)
    if string_end is None:
        scan_offset += 1
        continue
    start_line = comment_masked.count("\n", 0, scan_offset)
    end_line = comment_masked.count("\n", 0, string_end - 1)
    if end_line > start_line:
        statement_continuation_ends[start_line] = max(
            end_line, statement_continuation_ends.get(start_line, -1)
        )
    scan_offset = string_end
for line_number, end_line in multiline_comment_ends.items():
    statement_continuation_ends[line_number] = max(
        end_line, statement_continuation_ends.get(line_number, -1)
    )
for line_number, line in enumerate(depth_lines):
    trailing = line.rstrip("\r\n").rstrip()
    slash_count = len(trailing) - len(trailing.rstrip("\\"))
    if slash_count % 2 == 1:
        statement_continuation_ends[line_number] = max(
            line_number + 1, statement_continuation_ends.get(line_number, -1)
        )

bucket_files = {
    "10": "10-input-and-cursor.kdl",
    "20": "20-layout-and-overview.kdl",
    "30": "30-window-rules.kdl",
    "40": "40-environment.kdl",
    "50": "50-startup.kdl",
    "60": "60-animations.kdl",
    "70": "70-binds.kdl",
    "80": "80-layer-rules.kdl",
    "90": "90-user-extra.kdl",
}

buckets = {key: [] for key in ["root", *bucket_files.keys()]}

root_tokens = {"prefer-no-csd", "hotkey-overlay", "screenshot-path"}
map_tokens = {
    "input": "10",
    "gestures": "10",
    "cursor": "10",
    "layout": "20",
    "overview": "20",
    "window-rule": "30",
    "environment": "40",
    "spawn-at-startup": "50",
    "animations": "60",
    "binds": "70",
    "layer-rule": "80",
}


def classify(stripped: str) -> str:
    token = stripped.split()[0] if stripped.split() else ""
    if token in root_tokens:
        return "root"
    return map_tokens.get(token, "90")


pending = []
i = 0
while i < len(lines):
    line = lines[i]
    active_line = active_lines[i]
    stripped = active_line.strip()

    if not stripped:
        pending.append(line)
        i += 1
        continue

    segment = pending + [line]
    pending = []
    depth = depth_lines[i].count("{") - depth_lines[i].count("}")
    continuation_end_line = statement_continuation_ends.get(i, -1)
    i += 1

    while (depth > 0 or i <= continuation_end_line) and i < len(lines):
        segment.append(lines[i])
        depth += depth_lines[i].count("{") - depth_lines[i].count("}")
        continuation_end_line = max(
            continuation_end_line, statement_continuation_ends.get(i, -1)
        )
        i += 1

    buckets[classify(stripped)].extend(segment)

if pending:
    buckets["root"].extend(pending)


def normalize_text(lines_or_text):
    if isinstance(lines_or_text, list):
        text = "".join(lines_or_text)
    else:
        text = lines_or_text
    text = text.strip("\n")
    return f"{text}\n" if text else ""


stage_root = Path(tempfile.mkdtemp(prefix=".inir-modularize-", dir=niri_dir))
stage_modular_dir = stage_root / "config.d"
stage_modular_dir.mkdir()

startup_text = normalize_text(buckets["50"]).replace(
    'spawn-at-startup "inir" "start"',
    f'spawn-at-startup "{launcher_path}" "start"',
)
binds_text = normalize_text(buckets["70"]).replace(
    'spawn "inir" "',
    f'spawn "{launcher_path}" "',
).replace(
    'spawn "bash" "-lc" "exec \\"$(inir path)/scripts/launch-terminal.sh\\""',
    f'spawn "{launcher_path}" "terminal"',
).replace(
    'spawn "bash" "-lc" "exec \\"$(inir path)/scripts/close-window.sh\\""',
    f'spawn "{launcher_path}" "close-window"',
)

USER_EXTRA_TEMPLATE = """\
// ─────────────────────────────────────────────────────────────────────────────
// 90 — Your personal overrides
// ─────────────────────────────────────────────────────────────────────────────
//
// This file is YOURS. iNtoo updates will never overwrite it.
// Put any custom configuration here: extra binds, per-app window rules,
// output configuration, named workspaces, etc.
//
// Since Niri evaluates config top-to-bottom, settings here take effect
// after all other config.d files. For window-rule properties, all matching
// rules accumulate. For scalar values (like gaps), the last one wins.
//
// Examples:
//
//   // Named workspaces (sticky, always present)
//   workspace "browser"
//   workspace "code"
//   workspace "music"
//
//   // Per-app workspace assignment
//   window-rule {
//       match app-id="firefox"
//       open-on-workspace "browser"
//   }
//
//   // Extra keybind
//   binds {
//       Mod+P { spawn "rofi" "-show" "drun"; }
//   }
//
//   // Output config for your specific monitor
//   output "DP-1" {
//       mode "3440x1440@100.000"
//       scale 1
//       position x=0 y=0
//   }
// ─────────────────────────────────────────────────────────────────────────────
"""

write_map = {
    "10": normalize_text(buckets["10"]),
    "20": normalize_text(buckets["20"]),
    "30": normalize_text(buckets["30"]),
    "40": normalize_text(buckets["40"]),
    "50": startup_text,
    "60": normalize_text(buckets["60"]),
    "70": binds_text,
    "80": normalize_text(buckets["80"]),
    "90": normalize_text(buckets["90"]) or USER_EXTRA_TEMPLATE,
}

for key, filename in bucket_files.items():
    (stage_modular_dir / filename).write_text(write_map[key], encoding="utf-8")

root_text = normalize_text(buckets["root"]).rstrip("\n")
include_lines = [
    'include "config.d/10-input-and-cursor.kdl"',
    'include "config.d/20-layout-and-overview.kdl"',
    'include "config.d/30-window-rules.kdl"',
    'include "config.d/40-environment.kdl"',
    'include "config.d/50-startup.kdl"',
    'include "config.d/60-animations.kdl"',
    'include "config.d/70-binds.kdl"',
    'include "config.d/80-layer-rules.kdl"',
    'include "config.d/90-user-extra.kdl"',
]
root_parts = []
if root_text:
    root_parts.append(root_text)
root_parts.extend(include_lines)
staged_config = stage_root / "config.kdl"
staged_config.write_text("\n\n".join(root_parts).rstrip() + "\n", encoding="utf-8")

validator = shutil.which("niri")
if validator:
    checked = subprocess.run([validator, "validate", "-c", str(staged_config)],
                             text=True, capture_output=True)
    if checked.returncode != 0:
        shutil.rmtree(stage_root, ignore_errors=True)
        raise SystemExit(checked.stdout + checked.stderr or "Generated Niri config is invalid")

backup_dir = None
if modular_dir.exists():
    backup_dir = niri_dir / f"config.d.pre-modular-{time.strftime('%Y%m%d-%H%M%S')}"
    if backup_dir.exists():
        backup_dir = niri_dir / f"{backup_dir.name}-{os.getpid()}"
    shutil.move(str(modular_dir), str(backup_dir))
try:
    shutil.move(str(stage_modular_dir), str(modular_dir))
    os.replace(staged_config, config_path)
except OSError:
    if backup_dir is not None and backup_dir.exists():
        shutil.rmtree(modular_dir, ignore_errors=True)
        shutil.move(str(backup_dir), str(modular_dir))
    raise
finally:
    shutil.rmtree(stage_root, ignore_errors=True)
MIGRATE
}
