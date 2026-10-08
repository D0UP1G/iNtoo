# User modification detection for iNtoo
# Detects and preserves user changes before updates overwrite them
# This script is meant to be sourced.

# shellcheck shell=bash

USER_MODS_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/quickshell/user-mods"
MAX_USER_MODS=5

# File patterns to track for modifications (code files only)
TRACKED_PATTERNS=("*.qml" "*.js" "*.py" "*.sh" "*.fish")

###############################################################################
# Manifest v2 with checksums
###############################################################################

# Check if manifest has checksums (v2 format)
manifest_has_checksums() {
    local manifest_file="$1"
    [[ -f "$manifest_file" ]] || return 1
    # v2 manifests may have either the legacy ii header or the current inir header
    head -1 "$manifest_file" | grep -qE "(ii|inir)-manifest v2" && return 0
    # Fallback: check if any line has path:checksum format (64 hex chars)
    grep -q "^[^#].*:[a-f0-9]\{64\}$" "$manifest_file" 2>/dev/null
}

find_runtime_manifest_file() {
    local target_dir="$1"
    local manifest
    for manifest in "${target_dir}/.inir-manifest" "${target_dir}/.ii-manifest"; do
        if [[ -f "$manifest" ]]; then
            printf '%s\n' "$manifest"
            return 0
        fi
    done
    return 1
}

###############################################################################
# Modification Detection
###############################################################################

# Detect files that user modified (checksum differs from manifest)
# Outputs list of modified file paths, one per line
detect_user_modifications() {
    local target_dir="$1"
    local manifest_file="$2"

    [[ -f "$manifest_file" ]] || return 0
    manifest_has_checksums "$manifest_file" || return 0

    # Read manifest and compare checksums directly
    while IFS=: read -r path checksum; do
        # Skip comments, empty lines, and entries without checksums
        [[ "$path" =~ ^# ]] && continue
        [[ -z "$path" ]] && continue
        [[ -z "$checksum" ]] && continue

        local full_path="${target_dir}/${path}"
        [[ -f "$full_path" ]] || continue

        local current_checksum
        current_checksum=$(sha256sum "$full_path" 2>/dev/null | cut -d' ' -f1)

        if [[ "$current_checksum" != "$checksum" ]]; then
            echo "$path"
        fi
    done < "$manifest_file"
}

# Detect files user added (exist in target but not in manifest)
# Only checks tracked patterns (QML, JS, etc.) inside the payload scope: the
# manifest never covers directories that are not installed (docs/, distro/, ...),
# so scanning the whole tree reports our own files as user additions forever on
# repo-link installs.
detect_user_additions() {
    local target_dir="$1"
    local manifest_file="$2"
    local repo_root="${3:-${REPO_ROOT:-}}"

    [[ -f "$manifest_file" ]] || return 0

    # Extract paths from manifest (works for both v1 and v2)
    local manifest_paths
    manifest_paths=$(mktemp)
    grep -v "^#" "$manifest_file" | cut -d: -f1 | sort -u > "$manifest_paths"

    local scan_dirs
    scan_dirs=$(mktemp)
    grep -F / "$manifest_paths" | cut -d/ -f1 | sort -u > "$scan_dirs"

    local find_roots=()
    local dir
    while IFS= read -r dir; do
        [[ -n "$dir" ]] || continue
        [[ -d "${target_dir}/${dir}" ]] && find_roots+=("${target_dir}/${dir}")
    done < "$scan_dirs"

    local pattern_args=()
    local pattern
    for pattern in "${TRACKED_PATTERNS[@]}"; do
        [[ ${#pattern_args[@]} -gt 0 ]] && pattern_args+=(-o)
        pattern_args+=(-name "$pattern")
    done

    {
        find "$target_dir" -maxdepth 1 -type f \( "${pattern_args[@]}" \) 2>/dev/null
        [[ ${#find_roots[@]} -gt 0 ]] && find "${find_roots[@]}" -type f \( "${pattern_args[@]}" \) 2>/dev/null
    } | while IFS= read -r file; do
        local rel_path="${file#$target_dir/}"
        [[ "$rel_path" == .* ]] && continue
        grep -qxF "$rel_path" "$manifest_paths" && continue
        # Shipped but deliberately not installed (runtime exclusions, repo-link
        # targets that are the checkout itself) — ours, not the user's.
        [[ -n "$repo_root" && -e "${repo_root}/${rel_path}" ]] && continue
        echo "$rel_path"
    done

    rm -f "$manifest_paths" "$scan_dirs"
}

# Detect modifications by comparing installed files directly against repo source
# Used when no checksum-based manifest exists (v1 manifests or fresh installs
# coming from main branch). Compares code files only.
detect_user_modifications_by_source_comparison() {
    local target_dir="$1"
    local repo_root="$2"

    local runtime_root_manifest="${repo_root}/sdata/runtime-root-files.txt"
    local runtime_dirs_manifest="${repo_root}/sdata/runtime-payload-dirs.txt"
    local checksum_extensions="qml|js|py|sh|fish"

    _compare_file() {
        local rel_path="$1"
        local ext="${rel_path##*.}"
        [[ "$ext" =~ ^($checksum_extensions)$ ]] || return 0

        local installed="${target_dir}/${rel_path}"
        local repo_file="${repo_root}/${rel_path}"
        [[ -f "$installed" ]] || return 0
        [[ -f "$repo_file" ]] || return 0

        local installed_hash repo_hash
        installed_hash=$(sha256sum "$installed" 2>/dev/null | cut -d' ' -f1)
        repo_hash=$(sha256sum "$repo_file" 2>/dev/null | cut -d' ' -f1)

        if [[ "$installed_hash" != "$repo_hash" ]]; then
            echo "$rel_path"
        fi
    }

    # Root QML files
    for qml in "$target_dir"/*.qml; do
        [[ -f "$qml" ]] || continue
        _compare_file "$(basename "$qml")"
    done

    # Root manifest files
    if [[ -f "$runtime_root_manifest" ]]; then
        while IFS= read -r runtime_file; do
            [[ -n "$runtime_file" ]] || continue
            _compare_file "$runtime_file"
        done < "$runtime_root_manifest"
    fi

    # Payload directory files
    if [[ -f "$runtime_dirs_manifest" ]]; then
        while IFS= read -r dir; do
            [[ -n "$dir" ]] || continue
            [[ -d "$target_dir/$dir" ]] || continue
            find "$target_dir/$dir" -type f ! -name 'AGENTS.md' 2>/dev/null | while read -r file; do
                local rel_path="${file#$target_dir/}"
                _compare_file "$rel_path"
            done
        done < "$runtime_dirs_manifest"
    fi

    unset -f _compare_file
}

###############################################################################
# Preservation
###############################################################################

# Preserve user modifications to a dedicated directory
# Args: target_dir, space-separated modified files, space-separated added files
# Outputs: path where modifications were saved
preserve_user_modifications() {
    local target_dir="$1"
    shift
    local mod_files_str="$1"
    shift
    local add_files_str="$1"

    local timestamp staging preserve_dir suffix
    timestamp=$(date +%Y%m%d-%H%M%S) || return 1
    mkdir -p -- "$USER_MODS_DIR" || return 1
    staging=$(mktemp -d "${USER_MODS_DIR}/.tmp-${timestamp}.XXXXXX") || return 1
    suffix="${staging##*.}"
    preserve_dir="${USER_MODS_DIR}/${timestamp}-${suffix}"

    local mod_count=0
    local add_count=0

    # Copy modified files
    if [[ -n "$mod_files_str" ]]; then
        while IFS= read -r path; do
            [[ -z "$path" ]] && continue
            if [[ "$path" == /* || "$path" == .. || "$path" == ../* || "$path" == */../* || "$path" == */.. ]]; then
                rm -rf -- "$staging"
                return 1
            fi
            local src="${target_dir}/${path}"
            local dst="${staging}/${path}"
            if [[ ! -f "$src" ]] || ! mkdir -p -- "$(dirname "$dst")" || ! cp -p -- "$src" "$dst"; then
                rm -rf -- "$staging"
                return 1
            fi
            ((mod_count++))
        done <<< "$mod_files_str"
    fi

    # Copy user additions to _additions subdirectory
    if [[ -n "$add_files_str" ]]; then
        while IFS= read -r path; do
            [[ -z "$path" ]] && continue
            if [[ "$path" == /* || "$path" == .. || "$path" == ../* || "$path" == */../* || "$path" == */.. ]]; then
                rm -rf -- "$staging"
                return 1
            fi
            local src="${target_dir}/${path}"
            local dst="${staging}/_additions/${path}"
            if [[ ! -f "$src" ]] || ! mkdir -p -- "$(dirname "$dst")" || ! cp -p -- "$src" "$dst"; then
                rm -rf -- "$staging"
                return 1
            fi
            ((add_count++))
        done <<< "$add_files_str"
    fi

    # Create metadata
    local from_commit
    from_commit=$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo "unknown")

    if ! python3 - "${staging}/metadata.json" "$(date -Iseconds)" "$from_commit" "$mod_count" "$add_count" <<'PY'
import json
import sys

path, preserved_at, from_commit, modified, additions = sys.argv[1:]
with open(path, "w", encoding="utf-8") as stream:
    json.dump({
        "preserved_at": preserved_at,
        "from_commit": from_commit,
        "modified_count": int(modified),
        "additions_count": int(additions),
    }, stream, ensure_ascii=False, indent=2)
    stream.write("\n")
PY
    then
        rm -rf -- "$staging"
        return 1
    fi

    if [[ -e "$preserve_dir" ]] || ! mv -- "$staging" "$preserve_dir"; then
        rm -rf -- "$staging"
        return 1
    fi

    # Cleanup old preservation directories
    cleanup_old_user_mods "$preserve_dir" || printf '%s\n' "Warning: could not prune old user modification backups" >&2

    [[ -d "$preserve_dir" ]] || return 1
    echo "$preserve_dir"
}

preserve_runtime_tree() {
    local source_dir="$1"
    local label="${2:-runtime}"

    local timestamp staging preserve_dir suffix dir_name file_count from_commit
    timestamp=$(date +%Y%m%d-%H%M%S) || return 1
    mkdir -p -- "$USER_MODS_DIR" || return 1
    staging=$(mktemp -d "${USER_MODS_DIR}/.tmp-${timestamp}.XXXXXX") || return 1
    suffix="${staging##*.}"
    preserve_dir="${USER_MODS_DIR}/${timestamp}-${suffix}"
    dir_name=$(printf '%s' "$label" | tr -cs 'A-Za-z0-9._-' '-')
    [[ -n "$dir_name" ]] || dir_name="runtime"

    if ! mkdir -p -- "${staging}/${dir_name}" || ! rsync -a \
        --exclude='.git/' \
        --exclude='.ii-manifest' \
        --exclude='.inir-manifest' \
        "${source_dir}/" "${staging}/${dir_name}/"; then
        rm -rf -- "$staging"
        return 1
    fi

    file_count=$(python3 - "${staging}/${dir_name}" <<'PY'
import os
import sys

count = 0
def fail(error):
    raise error

for _root, _directories, files in os.walk(sys.argv[1], onerror=fail):
    count += len(files)
print(count)
PY
) || { rm -rf -- "$staging"; return 1; }
    from_commit=$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo "unknown")

    if ! python3 - "${staging}/metadata.json" "$(date -Iseconds)" "$source_dir" "$label" "$dir_name" "$from_commit" "$file_count" <<'PY'
import json
import sys

path, preserved_at, source_dir, label, dir_name, from_commit, file_count = sys.argv[1:]
with open(path, "w", encoding="utf-8") as stream:
    json.dump({
        "preserved_at": preserved_at,
        "source_dir": source_dir,
        "label": label,
        "dir_name": dir_name,
        "from_commit": from_commit,
        "full_runtime_copy": True,
        "modified_count": int(file_count),
        "additions_count": 0,
    }, stream, ensure_ascii=False, indent=2)
    stream.write("\n")
PY
    then
        rm -rf -- "$staging"
        return 1
    fi

    if [[ -e "$preserve_dir" ]] || ! mv -- "$staging" "$preserve_dir"; then
        rm -rf -- "$staging"
        return 1
    fi

    cleanup_old_user_mods "$preserve_dir" || printf '%s\n' "Warning: could not prune old user modification backups" >&2

    [[ -d "$preserve_dir" ]] || return 1
    echo "$preserve_dir"
}

# Resolve the payload root for both partial modification backups and full
# runtime snapshots. Validate metadata before using its label as a path.
user_mods_restore_source_root() {
    local backup_path="$1" metadata="$1/metadata.json" full_copy label
    [[ -d "$backup_path" ]] || return 1
    [[ -f "$metadata" ]] || { printf '%s\n' "$backup_path"; return 0; }
    command -v jq >/dev/null 2>&1 || return 1
    jq empty "$metadata" >/dev/null 2>&1 || return 1
    full_copy=$(jq -r '.full_runtime_copy == true' "$metadata") || return 1
    if [[ "$full_copy" != "true" ]]; then
        printf '%s\n' "$backup_path"
        return 0
    fi

    label=$(jq -r '.dir_name // .label // empty' "$metadata") || return 1
    if [[ ! "$label" =~ ^[A-Za-z0-9._-]+$ || "$label" == "." || "$label" == ".." ]]; then
        label=$(jq -r '.label // empty' "$metadata" | tr -cs 'A-Za-z0-9._-' '-')
    fi
    [[ "$label" =~ ^[A-Za-z0-9._-]+$ && "$label" != "." && "$label" != ".." ]] || return 1
    [[ -d "$backup_path/$label" ]] || return 1
    printf '%s\n' "$backup_path/$label"
}

# Remove old user modification backups, keep only MAX_USER_MODS
cleanup_old_user_mods() {
    [[ -d "$USER_MODS_DIR" ]] || return 0

    local root entries_file record backup_path stale_count index=0 failed=0 protected="" protected_count=0
    local -a entries=()
    root=$(realpath -e -- "$USER_MODS_DIR") || return 1
    [[ -z "${1:-}" ]] || protected=$(realpath -e -- "$1") || return 1
    entries_file=$(mktemp) || return 1
    if ! (set -o pipefail; find "$root" -mindepth 1 -maxdepth 1 -type d -name '[0-9]*' -printf '%T@\t%p\0' 2>/dev/null | sort -z -n) > "$entries_file"; then
        rm -f -- "$entries_file"
        return 1
    fi
    while IFS= read -r -d '' record; do
        if [[ "${record#*$'\t'}" == "$protected" ]]; then
            protected_count=1
        else
            entries+=("$record")
        fi
    done < "$entries_file"
    rm -f -- "$entries_file" || return 1

    stale_count=$(( ${#entries[@]} + protected_count - MAX_USER_MODS ))
    (( stale_count > 0 )) || return 0
    for ((index = 0; index < stale_count; index++)); do
        record="${entries[$index]}"
        backup_path="${record#*$'\t'}"
        if [[ "$(dirname -- "$backup_path")" != "$root" || "$backup_path" == "$root" ]]; then
            log_error "Refusing to remove a path outside the user modification backup directory"
            failed=1
            continue
        fi
        rm -rf -- "$backup_path" || failed=1
    done
    return "$failed"
}

###############################################################################
# TUI Interaction
###############################################################################

# Display modification summary
show_modification_summary() {
    local mod_files_str="$1"
    local add_files_str="$2"

    local mod_count=0
    local add_count=0

    if [[ -n "$mod_files_str" ]]; then
        mod_count=$(echo "$mod_files_str" | grep -c .)
        tui_subtitle "Modified files (${mod_count}):"
        echo "$mod_files_str" | head -8 | while read -r f; do
            [[ -n "$f" ]] && echo "    $f"
        done
        [[ $mod_count -gt 8 ]] && tui_dim "    ... and $((mod_count - 8)) more"
    fi

    if [[ -n "$add_files_str" ]]; then
        add_count=$(echo "$add_files_str" | grep -c .)
        echo ""
        tui_subtitle "Files you added (${add_count}):"
        echo "$add_files_str" | head -5 | while read -r f; do
            [[ -n "$f" ]] && echo "    $f"
        done
        [[ $add_count -gt 5 ]] && tui_dim "    ... and $((add_count - 5)) more"
    fi
}

# Show diff for a specific file (if snapshot available)
show_file_diff() {
    local target_dir="$1"
    local file_path="$2"
    local snapshot_dir="$3"

    local current="${target_dir}/${file_path}"
    local original="${snapshot_dir}/inir/${file_path}"
    if [[ ! -f "$original" ]]; then
        original="${snapshot_dir}/ii/${file_path}"
    fi

    if [[ -f "$original" ]] && [[ -f "$current" ]]; then
        echo ""
        tui_subtitle "Changes in: $file_path"
        if command -v delta &>/dev/null; then
            delta "$original" "$current" 2>/dev/null || diff -u "$original" "$current" 2>/dev/null || true
        else
            diff -u --color=auto "$original" "$current" 2>/dev/null || diff -u "$original" "$current" 2>/dev/null || true
        fi
    else
        tui_warn "Cannot show diff: original file not found in snapshot"
    fi
}

# Interactive handler for user modifications
# Returns: 0 = continue with update, 1 = cancel update
# Sets PRESERVED_MODS_DIR global variable if modifications were preserved
handle_user_modifications() {
    local mod_files_str="$1"
    local add_files_str="$2"
    local source_dir="${3:-$II_TARGET}"

    local mod_count=0
    local add_count=0
    [[ -n "$mod_files_str" ]] && mod_count=$(echo "$mod_files_str" | grep -c . || echo 0)
    [[ -n "$add_files_str" ]] && add_count=$(echo "$add_files_str" | grep -c . || echo 0)

    local total=$((mod_count + add_count))
    [[ $total -eq 0 ]] && return 0

    PRESERVED_MODS_DIR=""

    echo ""
    tui_warn "Local modifications detected!"
    echo ""

    show_modification_summary "$mod_files_str" "$add_files_str"

    echo ""

    # Non-interactive mode: auto-preserve
    if ! $ask; then
        if ! PRESERVED_MODS_DIR=$(preserve_user_modifications "$source_dir" "$mod_files_str" "$add_files_str"); then
            tui_error "Could not preserve local modifications; update stopped"
            return 1
        fi
        tui_success "Auto-preserved $total file(s) to: $PRESERVED_MODS_DIR"
        return 0
    fi

    while true; do
        local choice
        choice=$(tui_choose "How would you like to proceed?" \
            "Preserve & Continue" \
            "View Changes" \
            "Continue (overwrite)" \
            "Cancel Update")

        case "$choice" in
            "Preserve & Continue")
                if ! PRESERVED_MODS_DIR=$(preserve_user_modifications "$source_dir" "$mod_files_str" "$add_files_str"); then
                    tui_error "Could not preserve local modifications; update stopped"
                    return 1
                fi
                echo ""
                tui_success "Modifications saved to:"
                tui_dim "    $PRESERVED_MODS_DIR"
                echo ""
                tui_info "To restore a file after update:"
                tui_dim "    cp $PRESERVED_MODS_DIR/<path> ~/.config/quickshell/inir/<path>"
                return 0
                ;;
            "View Changes")
                # Find latest snapshot for diff comparison
                local latest_snapshot
                latest_snapshot=$(ls -1t "$SNAPSHOTS_DIR" 2>/dev/null | head -1)

                if [[ -n "$latest_snapshot" ]] && [[ -d "${SNAPSHOTS_DIR}/${latest_snapshot}/inir" ]]; then
                    echo ""
                    local shown=0
                    while IFS= read -r f && [[ $shown -lt 3 ]]; do
                        [[ -z "$f" ]] && continue
                        show_file_diff "$source_dir" "$f" "${SNAPSHOTS_DIR}/${latest_snapshot}"
                        echo ""
                        ((shown++))
                    done <<< "$mod_files_str"
                    [[ $mod_count -gt 3 ]] && tui_dim "(Showing first 3 files only)"
                elif [[ -n "$latest_snapshot" ]] && [[ -d "${SNAPSHOTS_DIR}/${latest_snapshot}/ii" ]]; then
                    echo ""
                    local shown=0
                    while IFS= read -r f && [[ $shown -lt 3 ]]; do
                        [[ -z "$f" ]] && continue
                        show_file_diff "$source_dir" "$f" "${SNAPSHOTS_DIR}/${latest_snapshot}"
                        echo ""
                        ((shown++))
                    done <<< "$mod_files_str"
                    [[ $mod_count -gt 3 ]] && tui_dim "(Showing first 3 files only)"
                else
                    tui_warn "No snapshot available for comparison"
                fi
                echo ""
                read -rp "Press Enter to continue..."
                echo ""
                show_modification_summary "$mod_files_str" "$add_files_str"
                echo ""
                ;;
            "Continue (overwrite)")
                echo ""
                tui_warn "Your modifications will be overwritten"
                tui_info "A snapshot was created - you can rollback with: ./setup rollback"
                if tui_confirm "Are you sure?" "no"; then
                    return 0
                fi
                echo ""
                show_modification_summary "$mod_files_str" "$add_files_str"
                echo ""
                ;;
            "Cancel Update"|*)
                tui_info "Update cancelled"
                return 1
                ;;
        esac
    done
}

###############################################################################
# List preserved modifications (for user reference)
###############################################################################

list_user_mods() {
    [[ -d "$USER_MODS_DIR" ]] || { echo "No preserved modifications found."; return; }

    echo ""
    tui_title "Preserved User Modifications"
    echo ""

    for dir in $(ls -1t "$USER_MODS_DIR" 2>/dev/null); do
        local meta="${USER_MODS_DIR}/${dir}/metadata.json"
        if [[ -f "$meta" ]]; then
            local mod_count add_count
            mod_count=$(grep -o '"modified_count": [0-9]*' "$meta" 2>/dev/null | grep -o '[0-9]*' || echo 0)
            add_count=$(grep -o '"additions_count": [0-9]*' "$meta" 2>/dev/null | grep -o '[0-9]*' || echo 0)

            tui_key_value "$dir" "${mod_count:-0} modified, ${add_count:-0} added"
        fi
    done

    echo ""
    tui_info "Location: $USER_MODS_DIR"
}
