# glib 0.18 security backport

This directory contains the unmodified crates.io `glib` 0.18.5 release except
for this document and the changes described below.
Its original MIT license and copyright notices are preserved.

- Archive: https://static.crates.io/crates/glib/glib-0.18.5.crate
- Archive SHA-256: `233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5`
- Release source revision: `42b9caf98e03ded086362d9653ca58fe94dc8658`
- Advisory: https://rustsec.org/advisories/RUSTSEC-2024-0429.html
- Upstream fix: https://github.com/gtk-rs/gtk-rs-core/pull/1343
- Fix revision: `b5a4071e439bef2b5eea76c3aa25e5ae84839e34`

`VariantStrIter::impl_get` now declares its output pointer as `mut` and passes
`&mut p` to `g_variant_get_child`. Passing `&p` allowed the C function to mutate
an immutable Rust reference, causing undefined behavior and crashes under
optimization. No public API or package version is changed.

`src/collections/ptr_slice.rs`: `PtrSlice::truncate` now removes elements through
the existing `pop()` operation, which updates the length and NULL terminator
before dropping each removed element. This avoids reusing a pointer after
running the element's destructor, reported by CodeQL as "Access of invalid pointer".
Its documentation now correctly says truncation retains the first elements.
The regression suite covers truncation, reuse, zero length and no-op cases.

Tauri 2's GTK3 dependencies require glib 0.18; glib 0.20, which contains the
released fix, is not compatible with that dependency family. The root
`[patch.crates-io]` selects this reviewed local backport for every consumer.
There is no advisory suppression. Windows builds do not compile this Linux
dependency; the portable Rust core remains independent of GLib.

On Linux with `libglib2.0-dev` and `pkg-config` installed, run:

```sh
cargo test --locked --release -p glib-security-regression
```

The tests exercise all five affected iterator methods against the actual
library, with optimization enabled, and run in the Linux CI job. Reverting
the backport must make this test fail on optimizing compilers.

Remove this patch and the regression-test package when the Tauri dependency
chain can resolve a patched upstream glib release. Do not raise the local
version to pretend this is an upstream release.
