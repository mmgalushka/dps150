#!/usr/bin/env bash
# Development entry point: a thin dispatcher over cargo (DEVOPS.md §4.0).
set -euo pipefail

cd "$(dirname "$0")"

usage() {
    cat <<USAGE
Usage: ./helper.sh <command>

Commands:
  build   Build every crate in the workspace
  test    Run every test in the workspace
  lint    Check formatting, run clippy and audit dependencies
USAGE
}

require() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "helper.sh: missing tool: $1 ($2)" >&2
        exit 1
    fi
}

command_name="${1:-}"
case "$command_name" in
    build)
        require cargo "install with rustup"
        cargo build --workspace --locked
        ;;
    test)
        require cargo "install with rustup"
        cargo test --workspace --locked
        ;;
    lint)
        require cargo "install with rustup"
        require cargo-deny "install with: cargo install cargo-deny --locked"
        cargo fmt --all --check
        cargo clippy --workspace --all-targets --locked -- -D warnings
        cargo deny --locked check
        ;;
    *)
        usage
        if [ -n "$command_name" ]; then
            echo "helper.sh: unknown command: $command_name" >&2
        fi
        exit 2
        ;;
esac
