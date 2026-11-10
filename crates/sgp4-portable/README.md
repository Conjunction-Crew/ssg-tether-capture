# sgp4-portable

A copy of the [sgp4](https://crates.io/crates/sgp4) crate, version 2.3.0
(<https://github.com/neuromorphicsystems/sgp4>), with every math call on the
[libm](https://crates.io/crates/libm) crate. The library is named `sgp4p`.

## Origin and licence

sgp4 2.3.0 is published under the MIT licence, Copyright (c) 2020 International
Centre for Neuromorphic Systems. The licence text is kept unchanged in
[`LICENSE`](LICENSE) and is reproduced in the repository's
`THIRD-PARTY-NOTICES.md`.

## What was changed

- The 256 calls of `sin`, `cos`, `atan2`, `powf` and `powi` in `src/` go through
  the new file `src/portable.rs`, which calls the `libm` crate (`powi` is a fixed
  square-and-multiply loop). `libm` is plain Rust, so the results are the same
  bits on Linux, macOS and Windows. Its `Float` and `Euclid` traits replace the
  ones upstream imports from `num-traits`, which is no longer a dependency.
  Operations that are exact in IEEE 754 (`sqrt`, `abs`, `floor`, `round`, `%`) are
  left as they were.
- The crate builds without `std` (features `alloc` and `libm`), as upstream allows.
- The code was formatted with `cargo fmt`, and the methods of the `Float` trait in
  `portable.rs` that nothing calls were removed.
- Upstream's tests, examples and documentation examples are not carried
  (`doctest = false`: the examples name the crate `sgp4`).

Nothing else was changed. Measured against upstream on macOS, up to 219 days from
the epoch: on two low Earth orbits the results differ by at most 7 ULP per
component, 3.4e-9 m; on a Molniya and a geostationary record by at most 25 ULP,
1.2e-8 m, which is at most 2.8e-16 of their distance from the Earth.

## How to check it

Compare `src/` with the registry source of sgp4 2.3.0:

```sh
diff -ru ~/.cargo/registry/src/*/sgp4-2.3.0/src crates/sgp4-portable/src
```

Every changed line is a renamed math call (`x.sin()` → `x.p_sin()` and so on), a
`use num_traits::…` line replaced by `use crate::portable::…`, a formatting change,
or this crate's header in `src/lib.rs`.
