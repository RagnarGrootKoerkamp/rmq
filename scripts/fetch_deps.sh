#!/usr/bin/env bash
# Clone the C++ RMQ libraries that build.rs compiles into BbST/ and FL-RMQ/, at the
# commits the bindings were written and tested against, plus FL-RMQ's sdsl-lite
# submodule (its only dependency the bindings need). Run from anywhere; a library
# already checked out at its pinned commit is left alone.
#
#     scripts/fetch_deps.sh
set -euo pipefail

cd "$(dirname "$0")/.."

# fetch <dir> <url> <commit>
fetch() {
    local dir=$1 url=$2 commit=$3
    if [[ -e $dir ]]; then
        local head
        head=$(git -C "$dir" rev-parse HEAD)
        if [[ $head != "$commit" ]]; then
            echo "$dir is at $head, expected $commit; check it out or remove it" >&2
            exit 1
        fi
        echo "$dir already at $commit" >&2
        return
    fi
    git clone --filter=blob:none "$url" "$dir"
    git -C "$dir" checkout --quiet "$commit"
}

fetch BbST https://github.com/kowallus/BbST 2161935921b00062ebe7a9c8118be5e6ce0ed7fe
fetch FL-RMQ https://github.com/FilippoLari/FL-RMQ ab55003b7b8808b44d0c304d127eff9da911dbc2

# The pinned sdsl-lite commit is recorded in FL-RMQ itself.
git -C FL-RMQ submodule update --init --depth 1 libs/sdsl-lite
