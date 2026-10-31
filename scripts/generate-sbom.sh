#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

source scripts/lib/packaging.sh

output=${1:-"$ofi_output_dir/openforensic-imager-${ofi_version}.cdx.json"}
shift || true
artifacts=("$@")
cmake -E make_directory "$(dirname "$output")"

metadata_file=$(mktemp)
rust_file=$(mktemp)
system_file=$(mktemp)
files_file=$(mktemp)
cleanup() {
    rm -f "$metadata_file" "$rust_file" "$system_file" "$files_file"
}
trap cleanup EXIT

cargo metadata --format-version 1 --locked >"$metadata_file"

jq -c '
    [
        .packages[]
        | {
            type: "library",
            "bom-ref": ("pkg:cargo/" + .name + "@" + .version),
            group: "cargo",
            name: .name,
            version: .version,
            purl: ("pkg:cargo/" + .name + "@" + .version),
            licenses: (
                if .license == null then
                    [{license: {name: "NOASSERTION"}}]
                else
                    [{expression: .license}]
                end
            ),
            properties: [
                {name: "cargo:source", value: (.source // "workspace")}
            ]
        }
    ]
    | sort_by(."bom-ref")
' "$metadata_file" >"$rust_file"

system_packages=(
    libtsk19t64
    libewf2
    libafflib0t64
    libvmdk1
    libvhdi1
    libsqlite3-0
    libqt6core6t64
    libqt6gui6
    libqt6widgets6
    libqt6qml6
    libqt6quick6
    libqt6quickcontrols2-6
    libqt6quickwidgets6
    qml6-module-qtquick
    qml6-module-qtquick-controls
    qml6-module-qtquick-layouts
    qml6-module-qtquick-templates
    qml6-module-qtquick-window
)
: >"$system_file"
for package in "${system_packages[@]}"; do
    version=$(dpkg-query -W -f='${Version}' "$package:amd64" 2>/dev/null || true)
    if [[ -z "$version" ]]; then
        continue
    fi
    jq -nc \
        --arg name "$package" \
        --arg version "$version" \
        '{
            type: "library",
            "bom-ref": ("pkg:deb/ubuntu/" + $name + "@" + $version),
            group: "ubuntu-26.04",
            name: $name,
            version: $version,
            purl: ("pkg:deb/ubuntu/" + $name + "@" + $version + "?arch=amd64")
        }' >>"$system_file"
done
jq -s 'sort_by(."bom-ref")' "$system_file" >"$system_file.sorted"
mv "$system_file.sorted" "$system_file"

: >"$files_file"
for artifact in "${artifacts[@]}"; do
    if [[ ! -f "$artifact" ]]; then
        continue
    fi
    digest=$(sha256sum "$artifact" | awk '{ print $1 }')
    size=$(stat -c '%s' "$artifact")
    jq -nc \
        --arg name "$(basename "$artifact")" \
        --arg digest "$digest" \
        --argjson size "$size" \
        '{
            type: "file",
            "bom-ref": ("file:" + $name + "@sha256:" + $digest),
            name: $name,
            hashes: [{alg: "SHA-256", content: $digest}],
            properties: [{name: "file:size", value: ($size | tostring)}]
        }' >>"$files_file"
done
jq -s 'sort_by(."bom-ref")' "$files_file" >"$files_file.sorted"
mv "$files_file.sorted" "$files_file"

timestamp=$(date -u -d "@$ofi_source_date_epoch" +%Y-%m-%dT%H:%M:%SZ)
jq -n \
    --arg timestamp "$timestamp" \
    --arg version "$ofi_version" \
    --slurpfile rust "$rust_file" \
    --slurpfile system "$system_file" \
    --slurpfile files "$files_file" \
    '{
        bomFormat: "CycloneDX",
        specVersion: "1.6",
        version: 1,
        metadata: {
            timestamp: $timestamp,
            tools: {
                components: [{
                    type: "application",
                    name: "openforensic-imager-packaging",
                    version: $version
                }]
            },
            component: {
                type: "application",
                "bom-ref": ("pkg:cargo/openforensic-imager@" + $version),
                name: "openforensic-imager",
                version: $version,
                licenses: [{expression: "GPL-3.0-or-later"}]
            }
        },
        components: (
            ($rust[0] + $system[0] + $files[0])
            | sort_by(."bom-ref")
        )
    }' >"$output"

(
    cd "$(dirname "$output")"
    sha256sum "$(basename "$output")" >"$(basename "$output").sha256"
)
printf '%s\n' "$output"
