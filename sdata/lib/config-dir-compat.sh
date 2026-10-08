#!/usr/bin/env bash

# Preserve both config trees while establishing the legacy path as an alias.
inir_config_compat_apply() {
  local xdg_config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
  local config_new="${xdg_config_home}/inir"
  local config_legacy="${xdg_config_home}/illogical-impulse"
  local backup_dir target

  mkdir -p -- "$xdg_config_home" || return 1

  # Validate the canonical destination before touching a legacy path. In
  # particular, never unlink a user supplied legacy symlink until its data is
  # safely available through the canonical directory.
  if [[ ( -e "$config_new" || -L "$config_new" ) && ! -d "$config_new" ]]; then
    echo "Canonical config path is not a directory: $config_new" >&2
    return 1
  fi

  if [[ -L "$config_legacy" ]]; then
    target="$(readlink -- "$config_legacy" 2>/dev/null)" || return 1
    if [[ "$target" == "$config_new" ]]; then
      mkdir -p -- "$config_new" || return 1
      return 0
    fi
    # An external directory link may be the only location containing the
    # user's settings. Merge without replacing canonical files, then switch
    # the alias only after the copy succeeds. Keep the external source intact.
    if [[ -d "$config_legacy" ]]; then
      # A legacy symlink can be the only remaining pointer to the user's
      # config. If there is no canonical tree yet, make the canonical name an
      # alias to that same real directory instead of copying it into a partial
      # tree. Keep the original link and its data untouched.
      if [[ ! -e "$config_new" && ! -L "$config_new" ]]; then
        local real_target staged_link
        real_target="$(readlink -f -- "$config_legacy" 2>/dev/null)" || return 1
        [[ -d "$real_target" ]] || {
          echo "Legacy config link target is not a directory: $config_legacy -> $target" >&2
          return 1
        }
        staged_link="${config_new}.inir-new-$$"
        ln -s -- "$real_target" "$staged_link" || return 1
        if ! mv -T -- "$staged_link" "$config_new"; then
          rm -f -- "$staged_link"
          echo "Could not create canonical config alias; original config link was preserved" >&2
          return 1
        fi
        return 0
      fi
      mkdir -p -- "$config_new" || return 1
      if ! cp -an -- "$config_legacy/." "$config_new/"; then
        echo "Could not copy legacy config from $config_legacy; original link was preserved" >&2
        return 1
      fi
      if ! ln -s -- "$config_new" "${config_legacy}.inir-new-$$"; then
        echo "Could not prepare compatibility link; original config link was preserved" >&2
        return 1
      fi
      if ! mv -Tf -- "${config_legacy}.inir-new-$$" "$config_legacy"; then
        rm -f -- "${config_legacy}.inir-new-$$"
        echo "Could not activate compatibility link; original config link was preserved" >&2
        return 1
      fi
      return 0
    fi
    echo "Legacy config link target is unavailable: $config_legacy -> $target" >&2
    return 1
  fi

  if [[ -d "$config_legacy" && ! -d "$config_new" ]]; then
    mv -- "$config_legacy" "$config_new" || return 1
    if ln -s -- "$config_new" "$config_legacy"; then
      return 0
    fi
    if mv -- "$config_new" "$config_legacy"; then
      echo "Could not create config compatibility link; legacy config was restored" >&2
    else
      echo "Could not create config compatibility link or restore the legacy directory at $config_legacy" >&2
    fi
    return 1
  fi

  if [[ -d "$config_legacy" && -d "$config_new" ]]; then
    backup_dir="${config_legacy}.pre-inir-$(date +%Y%m%d%H%M%S)-$$-${RANDOM}"
    mv -- "$config_legacy" "$backup_dir" || return 1
    if ! cp -an -- "$backup_dir/." "$config_new/"; then
      if [[ ! -e "$config_legacy" ]]; then
        mv -- "$backup_dir" "$config_legacy" || true
      fi
      echo "Could not merge legacy config; original data remains at $backup_dir" >&2
      return 1
    fi
    if ! ln -s -- "$config_new" "$config_legacy"; then
      if [[ ! -e "$config_legacy" ]]; then
        mv -- "$backup_dir" "$config_legacy" || true
      fi
      echo "Could not create compatibility link; original data remains at $backup_dir" >&2
      return 1
    fi
    return 0
  fi

  mkdir -p -- "$config_new" || return 1
  if [[ ! -e "$config_legacy" ]]; then
    ln -s -- "$config_new" "$config_legacy" || return 1
  else
    echo "Legacy config path is neither a directory nor a compatibility link: $config_legacy" >&2
    return 1
  fi
}
