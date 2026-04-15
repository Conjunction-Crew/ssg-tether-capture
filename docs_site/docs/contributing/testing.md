---
sidebar_position: 3
---

# Testing

Tests live under `src/tests/` and are compiled as part of the main crate.

## Running tests

```bash
cargo test
```

To run a specific test:

```bash
cargo test <test_name>
```

To see stdout output from passing tests (useful for debugging):

```bash
cargo test -- --nocapture
```

## Test structure

```
src/tests/
  mod.rs              — declares test submodules
  unit_tests.rs       — pure unit tests (no Bevy app)
  integration_tests.rs — tests using create_app()
```

## Unit tests (`unit_tests.rs`)

Unit tests cover individual functions and types in isolation — orbital mechanics calculations, COE↔RV conversions, component default values, etc. These do not require a running Bevy `App`.

## Integration tests (`integration_tests.rs`)

Integration tests use `lib::create_app()` to build a real Bevy `App` without the rendering stack (`DefaultPlugins` is not added). This lets tests verify:

- System ordering and scheduling (e.g. `init_orbitals` runs before propagation)
- Component initialisation from `Orbit` variants
- Plugin registration correctness

```rust
#[test]
fn test_orbital_initialisation() {
    let mut app = ssg_tether_capture::create_app();
    // spawn entities, run app.update(), assert on components
}
```

## Adding new tests

- **Pure logic** → add to `unit_tests.rs`.
- **Bevy system behaviour** → add to `integration_tests.rs` using `create_app()`.
- Avoid `app.run()` in tests — use `app.update()` to advance one frame at a time.
