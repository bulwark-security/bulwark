[![Bulwark Logo](/docs/assets/bulwark-hero.png)](https://bulwark.security/)

---

[![Crates.io Version](https://img.shields.io/crates/v/bulwark-ext-processor)][ext-processor-crate]
[![msrv 1.76.0](https://img.shields.io/badge/msrv-1.76.0-dea584.svg?logo=rust)][rust-ver]
[![Crates.io Total Downloads](https://img.shields.io/crates/d/bulwark-ext-processor)][ext-processor-crate]
[![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/bulwark-security/bulwark/rust.yml)][gha]
[![docs.rs](https://img.shields.io/docsrs/bulwark-sdk)][rustdoc]

[ext-processor-crate]: https://crates.io/crates/bulwark-ext-processor
[rust-ver]: https://github.com/rust-lang/rust/releases/tag/1.76.0
[gha]: https://github.com/bulwark-security/bulwark/actions/workflows/rust.yml
[rustdoc]: https://docs.rs/bulwark-sdk

Automated security decision-making under uncertainty.

## 🧩 Envoy External Processor

The `bulwark-ext-processor` crate is responsible for exposing a service that implements the
[Envoy external processing API](https://www.envoyproxy.io/docs/envoy/latest/api-v3/extensions/filters/http/ext_proc/v3/ext_proc.proto).
It connects Envoy to Bulwark's WebAssembly host environment.

This crate does not have a useful function outside of its use as a dependency of `bulwark-cli`.

## 🏰 What is Bulwark?

Bulwark is a fast, modern, open-source web application firewall (WAF) and API security gateway. It simplifies the
implementation of detective security controls while offering comprehensive visibility into your web services. Bulwark's
detection-as-code approach to rule definition offers security teams higher confidence in their response to persistent
and adaptive threats. Bulwark plugins offer a wide range of capabilities, enabling security teams to define and evolve
detections rapidly, without making changes to the underlying application.

## 🚀 Quickstart

In a Bulwark deployment, there are several pieces working together. In the current version of Bulwark,
[Envoy](https://www.envoyproxy.io/) handles the initial HTTP request processing. Bulwark uses Envoy's
[external processing API][ext-proc] to hook that processing and perform security decision-making on the traffic.
In most configurations, there will be an interior service that handles the actual business logic of the
web application and Envoy will be configured to send the traffic onwards once Bulwark has made its decision.

[ext-proc]: https://www.envoyproxy.io/docs/envoy/latest/api-v3/extensions/filters/http/ext_proc/v3/ext_proc.proto

An example [Envoy configuration file](/crates/ext-processor/examples/envoy.yaml) is provided as a starting point
for the typical deployment setup just described. The Envoy server would be launched with the following command:

```bash
envoy -c envoy.yaml
```

Bulwark's own [configuration file](https://bulwark.security/docs/reference/configuration/) is a TOML file that defines
which detection plugins should be used to process a request, as well as details like the listening port and the address
for the Redis server. The listening port in Bulwark's configuration must match the port number given for the
corresponding external processing filter section in Envoy's configuration. Bulwark is launched with the following
command (after installing the CLI with `cargo install bulwark-cli`):

```bash
bulwark-cli ext-processor -c bulwark.toml
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
