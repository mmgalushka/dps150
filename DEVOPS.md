# dps150 — Development, CI and Distribution

Status: baseline, 2026-10-30. Owns the toolchain, the development and test workflow, CI, the release process, where artifacts live and how users install them. Product goals, architecture and the decision log live in [spec/MAIN.md](spec/MAIN.md); this file does not repeat them.

## 1. Principles
- **GitHub only, for now.** Source, CI, release artifacts and the Homebrew tap all live on GitHub. No external registry accounts.
- **Simulator first, hardware when it matters.** The simulator covers day-to-day development and all of CI. The real device is reachable (directly on the Mac, or from the sandbox via the bridge, §4.1) for protocol verification and manual acceptance, but never from CI.
- **One tag, one release.** Pushing a `v*` tag produces every artifact.

## 2. Project facts
| Item | Value |
|---|---|
| Name | `dps150` (package and binary) |
| Repository | `mmgalushka/dps150`, public |
| Homebrew tap | `mmgalushka/homebrew-tap` |
| Language | Rust (toolchain pinned in `rust-toolchain.toml`, MSRV equals the pinned toolchain) |

### 2.1 Repository layout
Items marked *(planned)* do not exist yet; the rest is in the repository.

```
README.md  CONTRIBUTING.md  DEVOPS.md  LICENSE  .gitignore
Cargo.toml                 workspace root; also the package of the `dps150` binary
Cargo.lock                 committed: the project ships a binary
rust-toolchain.toml
deny.toml                  dependency audit policy (§5)
helper.sh                  development entry point (§4.0)
src/main.rs                `dps150`: argument parsing, wiring of the layers
crates/                    one crate per layer of spec/MAIN.md §5
  protocol/                frame codec, checksum, typed registers; no I/O
  device/                  pure link state machine, serial transport, device thread
  app/                     model, commands, safety rules, config files
  ui/                      ratatui rendering and input, colour, fonts
  simulator/               library `SimDevice` and binary `dps150-sim`
spec/                      specification (spec/MAIN.md is the start)
scripts/                   hardware bridge and attach scripts (§4.1)
tests/
  fixtures/                captured device bytes, read by `protocol` and `simulator` tests
  golden/                  reference renderings, read by `ui` tests (spec/DESIGN.md Appendix A)
.github/workflows/         ci.yml, and release.yml (planned) (§5)
```

- **Dependencies point one way:** `protocol` ← `device` ← `app` ← `ui` ← `dps150`. `simulator` depends only on `protocol`; no production crate depends on it, and `dps150-sim` is excluded from release artifacts.
- **Shared test data stays at the root.** `tests/fixtures/` and `tests/golden/` are used by several crates, so tests locate them relative to the workspace root (for example `env!("CARGO_MANIFEST_DIR")/../../tests/golden`); they are never copied into a crate.
- **Cross-crate and pty tests** (levels 1 and 2 of §4) live in the root package's `tests/*.rs`; unit tests stay in their crate.
- **`.gitignore` allowlist.** A new file type or directory needs its `!` rule in the same commit that adds the file (CONTRIBUTING.md).

## 3. Build prerequisites
- Rust toolchain from `rust-toolchain.toml`.
- Linux: `libudev-dev` and `pkg-config` (required by `serialport` for port enumeration).
- `helper.sh` wraps the common tasks (§4.0).

## 4. Development and testing
The `device` core is a pure state machine (spec/APP.md §7), so the simulator can stand in for the hardware at two levels.

| Level | What runs | Covers |
|---|---|---|
| 1. In-process | `protocol` unit tests; `device` and `app` tests with the pure device core piped to the library `SimDevice` (spec/SIMULATOR.md) | Encoding, parsing, state logic. Fast, no I/O. |
| 2. Pseudo-terminal | `dps150-sim` (spec/SIMULATOR.md) opens a pty and prints its path; `dps150 --port <path>`; faults are injected through the simulator's stdin | The real `serialport` code path, including partial frames and pushed frames interleaved with replies (spec/MAIN.md §3). Port auto-detection by VID/PID is not covered. |
| 3. UI golden and invariant tests | ratatui `TestBackend` compared cell by cell with the plain-text files in `tests/golden/` at their reference sizes (spec/DESIGN.md Appendix A); invariant checks (no overlap or clipping, key values visible) over a size sweep 44×16 … 250×80; reference sizes 44×16, 80×24, 100×21, 120×40 | Rendering against spec/DESIGN.md Appendix A and the Fluid stretch rules (spec/DESIGN.md §2.2). |

- **Level 2 runs on Linux only** (M0 spike, 2026-10-31). On macOS `serialport` sets the baud rate with the `IOSSIOSPEED` ioctl, which a pty refuses with `ENOTTY`, so `dps150 --port <pty>` at 115200 cannot open the simulator there; on a Mac, run level 2 and the app against the simulator in the sandbox. Baud 0 skips the ioctl and works, but the app has no such path. `tests/pty_spike.rs` holds both results and fails if a `serialport` update lifts the limit. Levels 1 and 3 run on both systems.
- **Hardware checks are manual.** The test unit is on the author's Mac. USB does not pass through into the Linux sandbox or Docker on macOS, so the real port is used either directly on the Mac host or through the bridge below. spec/DESIGN.md §12 is the manual acceptance checklist.
- **Hardware from the sandbox.** `sbx` cannot pass USB devices through, so the device is shared over a loopback TCP bridge; see §4.1. Read-only by default.
- **No hardware-in-the-loop CI.** Output-enable behaviour is unverified (spec/PROTOCOL.md §8), and an unattended runner able to switch a bench supply is a safety risk.

### 4.0 Developer entry point: `helper.sh`

A single `helper.sh` at the repository root is the one entry point for everyday development tasks, so a contributor does not have to remember the cargo, simulator and bridge invocations. Build, test and lint exist today; the other commands below arrive with the milestones that need them. Requirements:

- **Scope.** Thin dispatcher only: build, test (per level, §4), lint, start the simulator, run the app against the simulator or a given port, hardware bridge and attach (§4.1), and packaging. Logic stays in cargo and `scripts/`; `helper.sh` calls them.
- **Behaviour.** Fails loudly (`set -euo pipefail`, real exit codes), checks that required tools are installed and says which one is missing, prints usage when called without or with an unknown command, and works with macOS bash 3.2.
- **Safety.** Hardware commands are read-only by default (§4.1). Nothing that writes to the device or enables output is a plain command.
- **CI.** CI stays as defined in §5 and does not depend on `helper.sh`; the script is a development aid.
- **Documentation.** The command list and usage live in `CONTRIBUTING.md`, not in `README.md`.

### 4.1 Using the physical device from an `sbx` sandbox
For contributors who develop inside a Docker Sandbox (`sbx`) on a Mac. The sandbox has no USB access (`sbx create --help` has no device option, checked 2026-10-29), so the host shares the serial port over TCP on loopback and the sandbox turns it back into a pseudo-terminal. The app only sees a normal serial port path.

```
 DPS-150 --USB-- Mac: hw-bridge.sh  127.0.0.1:5150   <--sbx proxy-->   sandbox: hw-attach.sh --> /tmp/ttyDPS0 --> dps150
```

**Safety first.** The bridge filters nothing: whoever connects can send any frame, and the device accepts out-of-range values verbatim (spec/PROTOCOL.md §7). Connect only a resistive load (or nothing), start read-only (GET frames and the connect frame only), do not enable output until spec/PROTOCOL.md §8 is resolved, and stop the bridge when you are done. It binds to `127.0.0.1` only.

**One-time setup (Mac)**
```
brew install socat
sbx policy allow network localhost:5150
```

**Each session**
1. Mac, terminal 1: `scripts/hw-bridge.sh` and leave it running. It prints `Bridging /dev/cu.usbmodem… <-> 127.0.0.1:5150` and is silent after that. Pass the port as an argument if more than one `usbmodem` device is present.
2. Sandbox: `scripts/hw-attach.sh &` creates `/tmp/ttyDPS0`.
3. Check, read-only: `timeout 3 head -c 64 /tmp/ttyDPS0 | od -An -tx1`. You should see frames starting `f0 a1`. If it is silent, the device has no session: send the connect frame first, `printf '\xf1\xc1\x00\x01\x01\x02' > /tmp/ttyDPS0` (the app does this on open; spec/PROTOCOL.md §3).
4. Run the app: `dps150 --port /tmp/ttyDPS0`.
5. When finished: stop `hw-attach.sh`, then Ctrl-C the bridge.

**Options** (environment variables): `HW_PORT_TCP` (default 5150, both scripts). Attach only: `HW_LINK` (sandbox pty path, default `/tmp/ttyDPS0`), `HW_HOST` (default `host.docker.internal`). Bridge only: `HW_RECORD=<prefix>` writes `<prefix>.dev-to-host` and `<prefix>.host-to-dev` raw dumps for use as test fixtures; `HW_DEBUG=1` turns on `socat -d -d`.

**Troubleshooting**
| Symptom | Cause and fix |
|---|---|
| `hw-attach.sh` exits at once, `/tmp/ttyDPS0` missing, `socat -d -d` shows `socket … is at EOF` | The host side is not accepting: the bridge is not running or has stopped. Check on the Mac: `lsof -nP -iTCP:5150` should show `socat` listening on `127.0.0.1:5150`. Restart the bridge. |
| Link exists, but no bytes | Port 5000 clash: macOS AirPlay Receiver listens on 5000 and swallows connections, which is why 5150 is the default. Otherwise check `sbx policy log \| grep 5150` for a `dial failed` or a blocked entry. |
| A TCP connect to the host "succeeds" | Not proof of anything: the sandbox proxy accepts a connection to any port before dialling the host. Judge by data, not by "connected". |
| Link is up, bridge is fine, but the device sends nothing | No session: the device stays silent until it receives the connect frame `F1 C1 00 01 01 02` (spec/PROTOCOL.md §3). Send it once as above. |
| Second client gets nothing | The bridge serves one client at a time (no `fork`), so stop the previous `hw-attach.sh` first. |
| Bridge prints `unknown option "b115200"` | Old script version: macOS `socat` has no such option; the script sets the rate with `stty`. Update `scripts/hw-bridge.sh`. |
| `unbound variable` in the bridge | Old script version; macOS bash 3.2 needs `${arr[@]+"${arr[@]}"}`. Update the script. |

**Limits.** Only bytes are forwarded, not DTR/RTS; the device was silent with or without them until it got the connect frame (spec/PROTOCOL.md §3), and streamed correctly through the bridge afterwards. A Linux host can use the same scripts with an explicit port argument (`/dev/ttyACM0`), but `sbx` device access there has not been checked.

## 5. Continuous integration
Two GitHub Actions workflows.

**`ci.yml`** — pull requests and pushes to `main`:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked` on Linux and macOS
- dependency audit (`cargo deny --locked check`, policy in `deny.toml`)

The root `Cargo.toml` is also the `dps150` package, so without `--workspace` (`--all` for `fmt`) cargo covers that package alone and skips the crates.

**`release.yml`** — tag `v*`:
- cargo-dist builds the target matrix (§6), creates the GitHub Release with archives and checksums, and updates the Homebrew formula.
- A separate job builds the `.deb` packages with cargo-deb and attaches them to the same Release. cargo-dist does not produce `.deb` files.
- Build-provenance attestations are generated for the artifacts.

## 6. Targets
| Target | Notes |
|---|---|
| `aarch64-apple-darwin` | macOS, Apple silicon |
| `x86_64-apple-darwin` | macOS, Intel |
| `x86_64-unknown-linux-gnu` | Linux amd64 |
| `aarch64-unknown-linux-gnu` | Linux arm64 |

Linux builds target an older glibc so the binary runs on older distributions (older-glibc runner or `cargo-zigbuild`). Fully static musl builds are avoided because of the `libudev` dependency.

## 7. Where artifacts live
| Artifact | Location |
|---|---|
| Source | GitHub repository |
| Archives, checksums, installer scripts, `.deb` files, attestations | GitHub Releases |
| Homebrew formula | `mmgalushka/homebrew-tap` (a git repository), updated by cargo-dist |

## 8. How users install
```
# macOS and Linux with Homebrew
brew install mmgalushka/tap/dps150

# Debian and Ubuntu
sudo apt install ./dps150_<version>_<arch>.deb
```
- Brew users update with `brew upgrade`. The `.deb` is installed from a file, so there is no apt repository and no `apt upgrade` updates.
- Binaries installed through Homebrew or `curl` carry no quarantine flag, so macOS notarization is not required. A binary downloaded in a browser would trigger Gatekeeper.
- **Serial port naming.** macOS: `/dev/cu.usbmodem<serial>` (use `cu.`, not `tty.`). Linux: `/dev/ttyACM<n>`, the standard name for a CDC-ACM device; the unit has not been tried on Linux yet. Auto-detection finds the port by USB ID on both (spec/MAIN.md §6); `--port` overrides it.
- **Linux permissions.** Serial access needs membership of the `dialout` group (`sudo usermod -aG dialout $USER`, then log in again), or a udev rule for USB ID `2E3C:5740` (spec/PROTOCOL.md §1): `SUBSYSTEM=="tty", ATTRS{idVendor}=="2e3c", ATTRS{idProduct}=="5740", MODE="0660", GROUP="dialout"`. The `.deb` prints this as a post-install note. macOS needs no setup.

## 9. Deferred
To be reviewed later:
- **crates.io** (`cargo install dps150`, `cargo binstall dps150`). The name `dps150` was free on 2026-10-29; claim it when publishing.
- **Signed apt repository** (GitHub Pages with `aptly` or `reprepro`, or a hosted service such as Cloudsmith or packagecloud), to provide `apt update` and `apt upgrade`.
- **Windows** builds and packaging (winget, scoop).
- **homebrew-core** and the Debian archive, which would allow the bare `brew install dps150` and `apt install dps150`.
