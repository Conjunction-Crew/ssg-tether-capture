---
sidebar_position: 3
---

# Testing

How the project is tested, what each kind of test is for, and how to add one. The rules here bind every change: a pull request that lowers a coverage gate, slows the tests past the budget or breaks a pinned value needs an operator decision.

## Running tests

The workspace has three crates (`ssg-app`, `ssg-sim`, `ssg-sim-cli`) and `default-members` lists only `ssg-app`, so a bare `cargo test` runs the app's tests and nothing else. Run the whole workspace, as CI does:

```bash
cargo test --workspace --locked
```

Useful variants:

```bash
cargo test -p ssg-sim <name>                  # one crate, tests whose path contains <name>
cargo test --workspace -- --nocapture         # show stdout of passing tests
cargo test --workspace -- --list              # every test, one per line
cargo test -p ssg-sim-cli --locked cli_three_runs_to_capture_identical -- --ignored --nocapture
```

The first workspace build is long (Bevy and its dependencies); later runs are incremental.

## The layers

| Layer | Where | What it pins |
|---|---|---|
| Unit and system tests | `crates/ssg-sim/src/tests/` (`unit_tests`, `capture_*`, `orbit_*`, `project_files`, `tether_spawn`, `core_systems`, `session_telemetry`, `integration_tests`) and `crates/ssg-app/src/tests/` | One function or one system at a time, through a headless Bevy `App` where a system is involved. Every public function of `ssg-sim` has at least one test that asserts on its effect |
| Characterization tests | `crates/ssg-sim/src/tests/characterization/`, `crates/ssg-app/src/tests/characterization_app/` | The behaviour of the simulation as it is today (P2), including the Q13 scenario checkpoints (`baseline_hubble`). They change only together with a numbered finding (`F-nn`) and an operator-approved before/after |
| Scenario tests | `crates/ssg-sim/tests/` (`scenario_q13`, `scenario_example_plans`, `scenario_pause_warp`) | Out-of-crate tests that use only the public API and `ssg_sim::test_support`: the Q13 run, every example capture plan on the Q13 pair, pause and time warp |
| CLI tests | `crates/ssg-sim-cli/tests/` (`smoke`, `determinism`, `harness_parity`) | The built `ssg-sim` binary: output structure and error paths (`smoke`), bit-identical repeated runs and warp 1× vs 16× (`determinism`), and the CLI trajectory against the in-process harness (`harness_parity`) |
| Ignored long tests | `cli_three_runs_to_capture_identical` (CLI) and `baseline_q13_to_capture` (`ssg-sim`) | Determinism all the way to the `capture` state. They take minutes, so they carry `#[ignore]` and run on the CI schedule, on manual runs and on `full-ci` PRs (see below) |

Do not add `#[ignore]` to hide a slow or failing test. A test is ignored only when it is long, it has a named place where it runs, and that place is listed in the [Determinism](../concepts/determinism.md) tests table.

### The `test_support` harness

`ssg_sim::test_support` (feature `test-support`, enabled by the crates' dev-dependencies) is the shared headless harness. Tests in `crates/ssg-sim/src/tests/` use it as `crate::test_support`; out-of-crate tests and the CLI tests use `ssg_sim::test_support`. The app crate wraps it in `crates/ssg-app/src/tests/app_support.rs`.

- **`test_app()`** builds a minimal headless app wired like the real one (`SimPlugins`, no rendering), with a fixed sim epoch. **Advance time only with `tick` / `ticks`** (`app.update()` is exactly one fixed step). Never call `fixed_physics_step` from test code: outside `FixedUpdate` it would step the physics with a wall-clock delta, or not at all.
- **`fixed_epoch()`** is the sim epoch. Never read the wall clock in a test (F-06).
- **`enter_sim`, `start_running`** switch to the sim screen and start the clock.
- **`capture_rig`, `rig_set`, `rig_step`** drive the capture state machine with scripted ranges and speeds, without advancing physics. The controller-branch tests use them.
- **`q13`** builds the Q13 scenario (Hubble and the P0 chaser) through the real body spawner; `q13::run` samples checkpoints every 640 ticks.
- **`LogTap`** collects the `LogEvent` messages. Drain it after every tick, because the message buffers swap each frame.
- **`TempDir`** is a directory under `std::env::temp_dir()` named after the process id, removed on drop.
- **`Lcg`** is the fixed-seed generator for property-style tests (below). **`assert_close*`** compare floats with a stated tolerance and a label.

**Tests never touch the user's config directory, home directory or working directory.** Anything that needs files uses a `TempDir`. Functions that read or write the real config (`load_from_config`, `save_to_config`, `set_working_directory`) are tested through path-taking helpers (`read_config_file`, `write_config_file`) and are listed as approved exceptions in the public-function inventory.

### Property-style tests

There is no property-testing dependency. A test builds many inputs from an `Lcg` with a fixed seed (a few hundred cases per property), checks the property for each and, on failure, prints the seed and the case index so the failure can be reproduced exactly. Used for capture-plan validation and compilation (valid plans validate and compile; each single mutation gives the message the validator gives today) and for orbital element conversions (round trips, the LVLH frame round trip, the mean-motion and semi-major-axis relation).

### What a test must assert

Every test asserts on behaviour: a value, a state, a direction, an error message, a log event or an exit code. A test that only calls a function, `assert!(true)`, a `#[should_panic]` without `expected = "…"`, or a check too loose to fail (a computed value with a known expected value checked only for `is_finite()`) does not count and is rejected in review. When a new test shows a behaviour that looks wrong, pin today's behaviour with a `// Proposed finding: …` comment and raise it as a finding instead of fixing the code in the same change.

## Determinism in tests

Results are bit-identical **per platform**, not across platforms (see [Determinism](../concepts/determinism.md)).

- Each determinism test compares runs with each other on the same platform (`determinism`, `harness_parity`, `baseline_q13_fixed_epoch_short`). These run everywhere.
- The pinned numeric goldens (the capture checkpoints, the CLI `state_hash`) hold on macOS/aarch64 only. They are never asserted on Linux or Windows, and a test never carries a `cfg(target_os)` branch or a loosened tolerance to paper over a platform difference.
- **The CLI and the harness are offset by one step (F-51).** The CLI measures from step 2 (three updates, the capture command applied in the third); `q13::build` ticks once more, so the harness measures from step 3. Compare the two **by absolute step**: the CLI sample at step `s` and the harness checkpoint with the same `s` are the same trajectory point, bit for bit. `harness_parity` checks this; neither side's step semantics change, because that would move a pinned value.

## Coverage

CI measures line coverage in the `coverage` job ([CI/CD Workflows](./cicd-workflows.md)): one instrumented run of **all workspace tests** (`cargo llvm-cov --workspace`), so tests in any crate count towards the lines they execute. Only production code counts: `crates/*/src/tests/`, `crates/ssg-sim/src/test_support/` and `crates/*/tests/` are excluded by `scripts/coverage_gate.sh`, and test modules are kept in those paths, never inline.

| Area | Gate |
|---|---|
| `ssg-sim` | ≥ 80 % |
| `capture/`, `orbit/`, `physics_bubble`, `floating_origin`, `time` | ≥ 85 % each |
| `ssg-sim-cli` | ≥ 70 % |
| `ssg-app` | reported, no gate (the UI is rewritten in a later stage) |

The gates are enforced: a PR that drops an area below its number fails the `coverage` job and so `ci-success`. The thresholds live in `scripts/coverage_gates.conf`; lowering one, removing an area, or adding a coverage exclusion (`--ignore-filename-regex`, `#[coverage(off)]`, `cfg(coverage)`) needs an operator decision. Do not write tests that execute code without checking anything just to move the number.

To reproduce the numbers locally (the environment variables matter: without them the instrumented build needs tens of GB of disk), with `cargo-llvm-cov` 0.9.1 and `rustup component add llvm-tools-preview`:

```bash
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo llvm-cov --workspace --locked \
  --json --summary-only --output-path target/coverage-summary.json
sh scripts/coverage_gate.sh target/coverage-summary.json
```

The CI run also uploads a `coverage-report` artifact (HTML, `lcov.info`, `summary.json`; kept 14 days) that shows which lines are not covered. Local numbers can differ from CI by a few lines (platform `cfg`); the CI job is the gate.

## Test time

The "Lint and test (Linux)" job prints **Test execution time** (the sum of the `finished in` times of every test binary) to its job summary and as an annotation, so `gh run view <id>` shows it. It measures the uninstrumented test run, which is why coverage runs in a separate job. The number is noisy (identical test code has varied by about 10 s between runs and more between runner images), so it is a tripwire, not a benchmark: the reference for later stages is the median of the two v0.3.3 runs (the release PR and the push to `main` after it), and a change that raises the time by 25 s or more over that reference stops for a decision. New scenario tests should stay near 3 s each locally; run independent slow work concurrently (as `harness_parity` does) rather than one after the other.

## Adding a test

- **A function or a system** → a test in the matching file under `crates/ssg-sim/src/tests/` (or a new file declared in `src/tests/mod.rs`), built on `test_app()` and `tick` / `ticks`. Add the function to `plans/stage-5/results/pub-fn-inventory.md` if it is new and public.
- **Behaviour across the public API** → a file in `crates/ssg-sim/tests/`, using only `ssg_sim::…`.
- **The binary** → `crates/ssg-sim-cli/tests/`, running the built `ssg-sim` with the Q13 project.
- **Many inputs** → a property-style test with `Lcg`.
- **Anything with files** → a `TempDir`.
- Avoid `app.run()`; `app.update()` advances one fixed step.
- Run `cargo test --workspace --locked` and, for anything that touches paths or platform code, add the `full-ci` label to the PR: Windows and macOS run `cargo check` only on ordinary PRs, so a path-separator or platform bug in a test shows up only on a full run.
