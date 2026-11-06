# Contributing

Thanks for wanting to help. dps150 is small, so a good first contribution can be a bug report, a test on another OS or terminal, or a fix to the docs. By taking part you agree to the [code of conduct](CODE_OF_CONDUCT.md).

## Ways to help

- **Report a bug** with the [bug template](https://github.com/mmgalushka/dps150/issues/new/choose). Include `dps150 --version`, your OS, terminal and terminal size.
- **Try it on your hardware.** The DPS-150 is the only device this runs against. Linux in particular has had little hardware testing. Facts verified on a real unit are the most valuable thing you can add (they are tagged **[V]** in `spec/PROTOCOL.md`; unverified ones are **[U]**).
- **Fix or add something.** Look for issues labelled `good first issue`. For anything bigger than a small fix, open an issue first so we can agree on the behaviour before you write code.
- **No hardware?** You can do almost everything with the simulator, see below.

## Spec-driven development

The specification in `spec/` is the source of truth. The code follows the spec, not the other way round.

- `spec/MAIN.md` holds goals, scope, architecture and the decision log. The other files own one area each: `PROTOCOL`, `DESIGN`, `SAFETY`, `APP`, `SIMULATOR`, `PLAN`.
- `spec/REQUIREMENTS.md` is the register of testable rules. Each has a stable ID (`PRE-03`, `SAF-08`, …), the section where the rule text lives (tagged `{#ID}`), a status and the test levels that check it. IDs are never reused or renumbered; a removed rule stays in the table as `withdrawn`.
- Facts about the device or the simulator carry a tag: **[V]** verified on hardware, **[U]** unverified, **[A]** assumed in the simulator, **[D]** decided design.

**The rule: if behaviour changes, the spec changes first.** The order is:

1. **Spec.** Edit the section that owns the rule, and add or change its row in `spec/REQUIREMENTS.md`. If it is a design decision, add a line to the decision log.
2. **Tests.** Add or update tests that cite the rule ID. UI changes update the golden renderings in `tests/golden/`.
3. **Code.** Make the tests pass.
4. **One pull request** carries all three, so a reviewer can read the spec diff first.

A pull request that changes behaviour without a spec change will be asked to add one. Pure refactors, docs and CI changes do not need it.

*Example.* You want preset names to allow 12 characters instead of 9. Rule `PRE-03` says 9; `spec/DESIGN.md` §8.1 and §8.5 explain why (the Compact layout column). So you edit `PRE-03` and those sections, then the tests and the Compact golden (`presets-compact.txt`), and only then the code.

## Getting started

1. Clone the repository and enter it: `git clone https://github.com/mmgalushka/dps150.git && cd dps150`
2. Install Rust with [rustup](https://rustup.rs). On macOS with Homebrew: `brew install rustup`, then add `export PATH="$(brew --prefix rustup)/bin:$PATH"` to `~/.zshrc` and open a new terminal. On Linux also install `libudev-dev` and `pkg-config`.
3. Run `./helper.sh init`. It checks the prerequisites, installs the pinned toolchain and builds the workspace.
4. Run against the simulator (no device needed): in one terminal `cargo run -p simulator --bin dps150-sim`, which prints `listening on <path>`, then in another `./helper.sh run --port <path>`. Type `load 10` or `trip otp` into the simulator to inject conditions ([spec/SIMULATOR.md](spec/SIMULATOR.md) §5).
5. With a device: connect it with its USB cable and run `./helper.sh run`. Use `--port /dev/cu.usbmodemXXXX` (macOS) or `--port /dev/ttyACM0` (Linux) if the port is not found on its own. Press `?` for the keys and `q` to quit.
6. Before a pull request: `./helper.sh test` and `./helper.sh lint`.

Keep the output off with nothing attached to the terminals on a first run. On Linux your user needs the `dialout` group (`sudo usermod -aG dialout $USER`, then log in again). Only one program can hold the serial port, so close other copies of the app first.

## Development commands

`helper.sh` is the entry point for everyday tasks (DEVOPS.md §4.0).

| Command | Does |
| --- | --- |
| `./helper.sh init` | Check the prerequisites and build once (after cloning) |
| `./helper.sh run [--port <path>]` | Run the app, passing the arguments to `dps150` |
| `./helper.sh build` | Build every crate in the workspace |
| `./helper.sh test` | Run every test in the workspace |
| `./helper.sh lint` | Check formatting, run clippy and audit dependencies (needs `cargo-deny`) |
| `./helper.sh deb` | Build the `.deb` package into `target/debian/` (needs `cargo-deb`, Linux) |
| `./helper.sh dist-plan` | Show what a release would build (needs `cargo-dist`) |

## Tests

Tests are in levels (`L1`, `L2`, `L3` in the register): unit tests per crate, integration tests against the simulator over a pty, and full-screen tests that compare the rendered screen with the golden files. Hardware is never used in CI. Details: [DEVOPS.md](DEVOPS.md) §4.

## Pull requests

- Branch from `main` and name it by kind: `feat/…`, `fix/…`, `docs/…`.
- Keep each pull request about one thing. Commit messages: a short imperative first line ("Show q quit in the main panel key bar").
- Fill in the pull request template. CI runs format, clippy, tests on Linux and macOS, a dependency audit and coverage.
- Add a line to `CHANGELOG.md` under *Unreleased* if users would notice the change.
- **Output safety is the one thing we are strict about.** Anything touching what turns the output on, what it is set to, or what happens on a lost link needs a rule in `spec/SAFETY.md` and a test.
- AI-assisted code is welcome (this project is largely written with Claude). The same rules apply: you are responsible for what you submit, and the spec and tests decide whether it is right.

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
| `LICENSE`, `README.md`, `CONTRIBUTING.md`, `DEVOPS.md`, `CHANGELOG.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `.gitignore` | Top-level documents |
| `docs/images/*.png` | README images (screenshots, collage, device photo) |
| `spec/*.md` | Specification and design documents |
| `scripts/*.sh` | Helper shell scripts |
| `tests/fixtures/*.bin` | Captured binary protocol fixtures |
| `tests/golden/*.txt` | Golden UI renderings (plain text, one per screen and state) |
| `tests/*.rs` | Cross-crate and pseudo-terminal tests |
| `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml` | Workspace and toolchain configuration |
| `src/*.rs` | The `dps150` binary |
| `crates/*/Cargo.toml`, `crates/*/src/*.rs`, `crates/*/tests/*.rs`, `crates/*/examples/*.rs` | Workspace crates, their integration tests and examples |
| `helper.sh` | Development entry point |
| `.github/workflows/*.yml`, `.github/dependabot.yml`, `.github/ISSUE_TEMPLATE/*.yml`, `.github/PULL_REQUEST_TEMPLATE.md` | CI workflows, dependency updates, issue and pull request templates |
| `codecov.yml` | Coverage reporting settings |
| `packaging/postinst` | Maintainer script of the `.deb` (prints the serial-port note) |
| `dist-workspace.toml` | cargo-dist release configuration |
