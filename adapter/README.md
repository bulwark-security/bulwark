# WASI Preview 1 Adapter

Because the Rust compiler output is WASI Preview 1 and Bulwark currently
uses WASI Preview 2, we need an adapter to convert the compiler's output.
This adapter should be updated with each new `wasmtime` release. The
`git-sha` file is used to record the git commit SHA the adapter came from.

This file is acquired from the `wasmtime` project's "CI" workflow:
<https://github.com/bytecodealliance/wasmtime/actions/workflows/main.yml>

The correct CI run is found by filtering for "branch:release-x.y.z",
locating the run that corresponds to the appropriate release tag, and then by
downloading the zip file for the "bins-wasi-preview1-component-adapter"
artifact.
