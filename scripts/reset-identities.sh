#!/usr/bin/env bash
set -euo pipefail

APP_ID="com.robobeti.blabber-app"

case "$(uname -s)" in
    Linux)
        base_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
        ;;
    Darwin)
        base_dir="$HOME/Library/Application Support"
        ;;
    *)
        echo "error: unsupported OS '$(uname -s)' this script only handles Linux and macOS" >&2
        exit 1
        ;;
esac

app_data_dir="$base_dir/$APP_ID"
identities_dir="$app_data_dir/identities"
spaces_dir="$app_data_dir/spaces"
blobs_dir="$app_data_dir/blobs"

if [[ ! -d "$app_data_dir" ]]; then
    echo "no app data found at $app_data_dir. nothing to delete"
    exit 0
fi

echo "This will permanently delete:"
found_any=0
for dir in "$identities_dir" "$spaces_dir" "$blobs_dir"; do
    if [[ -d "$dir" ]]; then
        found_any=1
        echo "  - $dir"
    fi
done

if [[ "$found_any" -eq 0 ]]; then
    echo "nothing to delete no identities/spaces/blobs directories found under $app_data_dir"
    exit 0
fi

if [[ -d "$identities_dir" ]]; then
    echo
    echo "identities found:"
    find "$identities_dir" -maxdepth 1 -name '*.bin' -exec basename {} .bin \; | sed 's/^/  - /'
fi

if [[ "${1:-}" != "-y" && "${1:-}" != "--yes" ]]; then
    echo
    read -r -p "Type 'delete' to confirm: " confirmation
    if [[ "$confirmation" != "delete" ]]; then
        echo "aborted, nothing was deleted"
        exit 1
    fi
fi

for dir in "$identities_dir" "$spaces_dir" "$blobs_dir"; do
    if [[ -d "$dir" ]]; then
        rm -rf -- "$dir"
        echo "deleted $dir"
    fi
done

echo "done"
