[![Bulwark Logo](/docs/assets/bulwark-hero.png)](https://bulwark.security/)

---

[![Crates.io Version](https://img.shields.io/crates/v/bulwark-build)][build-crate]
[![msrv 1.76.0](https://img.shields.io/badge/msrv-1.76.0-dea584.svg?logo=rust)][rust-ver]
[![Crates.io Total Downloads](https://img.shields.io/crates/d/bulwark-build)][build-crate]
[![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/bulwark-security/bulwark/rust.yml)][gha]
[![docs.rs](https://img.shields.io/docsrs/bulwark-build)][rustdoc]

[build-crate]: https://crates.io/crates/bulwark-build
[rust-ver]: https://github.com/rust-lang/rust/releases/tag/1.76.0
[gha]: https://github.com/bulwark-security/bulwark/actions/workflows/rust.yml
[rustdoc]: https://docs.rs/bulwark-build

Automated security decision-making under uncertainty.

## 🛠️ Build

The build crate is responsible for building Bulwark's plugins.

## 🏰 What is Bulwark?

Bulwark is a fast, modern, open-source web application firewall (WAF) and API security gateway. It simplifies the
implementation of detective security controls while offering comprehensive visibility into your web services. Bulwark's
detection-as-code approach to rule definition offers security teams higher confidence in their response to persistent
and adaptive threats. Bulwark plugins offer a wide range of capabilities, enabling security teams to define and evolve
detections rapidly, without making changes to the underlying application.

## 🚀 Quickstart

Bulwark plugins are compiled to WebAssembly before use. While it's recommended to do this using a workflow like
[GitHub Actions](https://docs.github.com/en/actions), you can also do this manually, particularly for development.
Most users will want to use the Bulwark CLI to build their plugins. The CLI uses this library as a dependency.

To compile a Bulwark plugin with the CLI (after installing the CLI with `cargo install bulwark-cli`):

```bash
bulwark-cli build -p rules/example-plugin -o dist/plugins/
```

In some cases, you may want to compile a plugin without the CLI, such as within a test case.

```rust
bulwark_build::build_plugin(
    base.join("path/to/plugin"),            // input directory
    base.join("dist/plugins/plugin.wasm"),  // output file
    &[],                                    // no additional compiler args
    true,                                   // install missing targets if needed
)?;
```

## 💪 Contributing

Check out the list of [open issues](https://github.com/bulwark-security/bulwark/issues). We actively maintain a
list of [issues suitable for new contributors][good-first-issue] to the project. Alternatively, detection plugins
may be contributed to the [community ruleset](https://github.com/bulwark-security/bulwark-community-ruleset).

We do not require contributors to sign a license agreement (CLA) because we want users of Bulwark to be confident
that the software will remain available under its current license.

[good-first-issue]: https://github.com/bulwark-security/bulwark/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22

### 🤝 License

This project is licensed under the Apache 2.0 license with the LLVM exception. See LICENSE for more details.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project
by you, as defined in the Apache 2.0 license, shall be licensed as above, without any additional terms or conditions.

## 🛟 Getting Help

To start, check if the answer to your question can be found in any of the
[guides](https://bulwark.security/docs/guides/getting-started/) or
[API documentation](https://docs.rs/bulwark-sdk/latest/bulwark_sdk/).
If you aren't able to find an answer there, check the Bulwark project's
[discussion forum](https://github.com/bulwark-security/bulwark/discussions).
We are happy to help answer your questions and provide guidance through our
community forum.
