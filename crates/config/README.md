[![Bulwark Logo](/docs/assets/bulwark-hero.png)](https://bulwark.security/)

---

[![Crates.io Version](https://img.shields.io/crates/v/bulwark-config)][config-crate]
[![msrv 1.76.0](https://img.shields.io/badge/msrv-1.76.0-dea584.svg?logo=rust)][rust-ver]
[![Crates.io Total Downloads](https://img.shields.io/crates/d/bulwark-config)][config-crate]
[![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/bulwark-security/bulwark/rust.yml)][gha]
[![docs.rs](https://img.shields.io/docsrs/bulwark-config)][rustdoc]

[config-crate]: https://crates.io/crates/bulwark-config
[rust-ver]: https://github.com/rust-lang/rust/releases/tag/1.76.0
[gha]: https://github.com/bulwark-security/bulwark/actions/workflows/rust.yml
[rustdoc]: https://docs.rs/bulwark-config

Automated security decision-making under uncertainty.

## ⚙️ Config

The configuration crate is responsible for parsing Bulwark's configuration files and representing setting values
throughout the application.

This crate does not yet have a useful function outside of its use as a dependency of `bulwark-cli`.

## 🏰 What is Bulwark?

Bulwark is a fast, modern, open-source web application firewall (WAF) and API security gateway. It simplifies the
implementation of detective security controls while offering comprehensive visibility into your web services. Bulwark's
detection-as-code approach to rule definition offers security teams higher confidence in their response to persistent
and adaptive threats. Bulwark plugins offer a wide range of capabilities, enabling security teams to define and evolve
detections rapidly, without making changes to the underlying application.

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
