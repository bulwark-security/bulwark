# WASI Preview 1 Adapter

Because the Rust compiler output is WASI Preview 1 and Bulwark currently
uses WASI Preview 2, we need an adapter to convert the compiler's output.
This adapter should be updated with each new `wasmtime` release.

Notably, `wasmtime` maintains a `dev` release containing these adapters
and also publishes these adapters associated with its GitHub Actions.
However, these adapters must be locked to the runtime and should only
be acquired from the artifacts of the release that corresponds with the
version of the runtime in use. Using them from any other source will
typically result in errors.
