#!/usr/bin/env bash

# Load selected keys from a Compose env file without executing it as shell code.
pb_load_env_file() {
  local file="$1" line key value requested
  shift
  [ -f "$file" ] || return 0
  local -A wanted=()
  for requested in "$@"; do wanted["$requested"]=1; done

  while IFS= read -r line || [ -n "$line" ]; do
    line="${line%$'\r'}"
    [[ "$line" =~ ^[[:space:]]*(#|$) ]] && continue
    [[ "$line" =~ ^[[:space:]]*([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*=[[:space:]]*(.*)$ ]] || continue
    key="${BASH_REMATCH[1]}"
    value="${BASH_REMATCH[2]}"
    [ -n "${wanted[$key]:-}" ] || continue
    [ -z "${!key+x}" ] || continue
    if [[ "$value" =~ ^\'(.*)\'[[:space:]]*$ ]] || [[ "$value" =~ ^\"(.*)\"[[:space:]]*$ ]]; then
      value="${BASH_REMATCH[1]}"
    elif [[ "$value" =~ ^(.*[^[:space:]])[[:space:]]+#.*$ ]]; then
      value="${BASH_REMATCH[1]}"
    fi
    printf -v "$key" '%s' "$value"
    export "${key?}"
  done < "$file"
}
