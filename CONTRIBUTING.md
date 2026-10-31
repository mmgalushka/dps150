# Contributing

## Development commands

`helper.sh` is the entry point for everyday tasks (DEVOPS.md §4.0).

| Command | Does |
| --- | --- |
| `./helper.sh build` | Build every crate in the workspace |
| `./helper.sh test` | Run every test in the workspace |
| `./helper.sh lint` | Check formatting, run clippy and audit dependencies (needs `cargo-deny`) |

## Git ignore policy: ignore everything except...

`.gitignore` uses an **allowlist**. The first rule is `*`, which ignores every
file; anything that should be tracked is re-included explicitly with a `!` rule.

This keeps randomly created artifacts (build output, editor files, logs, scratch
notes, captures) out of commits by default.

### Adding a new file or directory

Only specific file types are allowed inside each directory, so a new file
needs a matching rule. Each directory needs its own `!/dir/` line, plus a
pattern for the files in it:

```gitignore
# new top-level file
!/NEW_FILE.md

# new file type in an existing directory
!/scripts/*.py

# new directory (the directory line and a file pattern are both needed)
!/src/
!/src/*.rs
```

Rules to remember:

- Git cannot re-include a file if its parent directory is ignored, so
  every directory level (e.g. `tests/` and `tests/fixtures/`) needs its own
  `!/dir/` line.
- Anchor entries with a leading `/` so they match only at the repository root.
- Check the result before committing:
  `git status --short -uall` lists what will be tracked, and
  `git check-ignore -v <path>` shows which rule applies to a path.
- Keep the allowlist grouped and commented, and commit `.gitignore` changes
  together with the files they allow.

### Currently allowed

| Path | Purpose |
| --- | --- |
| `LICENSE`, `README.md`, `CONTRIBUTING.md`, `DEVOPS.md`, `.gitignore` | Top-level documents |
| `spec/*.md` | Specification and design documents |
| `scripts/*.sh` | Helper shell scripts |
| `tests/fixtures/*.bin` | Captured binary protocol fixtures |
| `tests/golden/*.txt` | Golden UI renderings (plain text, one per screen and state) |
| `tests/*.rs` | Cross-crate and pseudo-terminal tests |
| `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml` | Workspace and toolchain configuration |
| `src/*.rs` | The `dps150` binary |
| `crates/*/Cargo.toml`, `crates/*/src/*.rs` | Workspace crates |
| `helper.sh` | Development entry point |
| `.github/workflows/*.yml` | CI workflows |
