[![Bulwark Logo](/docs/assets/bulwark-hero.png)](https://bulwark.security/)

---

[![Crates.io Version](https://img.shields.io/crates/v/bulwark-decision)][decision-crate]
[![msrv 1.76.0](https://img.shields.io/badge/msrv-1.76.0-dea584.svg?logo=rust)][rust-ver]
[![Crates.io Total Downloads](https://img.shields.io/crates/d/bulwark-decision)][decision-crate]
[![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/bulwark-security/bulwark/rust.yml)][gha]
[![docs.rs](https://img.shields.io/docsrs/bulwark-decision)][rustdoc]

[decision-crate]: https://crates.io/crates/bulwark-decision
[rust-ver]: https://github.com/rust-lang/rust/releases/tag/1.76.0
[gha]: https://github.com/bulwark-security/bulwark/actions/workflows/rust.yml
[rustdoc]: https://docs.rs/bulwark-decision

Automated security decision-making under uncertainty.

## 🎱 Decision

The decision crate is responsible for representing and processing Bulwark's security decisions.

Bulwark makes all of its security decisions by reading the output from plugins. Plugins primarily output a decision
structure, accompanied by an optional set of tags that help to annotate the result. The decision structure is designed
to allow plugins to quantitatively express uncertainty in an intuitive way. Each decision is composed of three values,
an `accept` value, a `restrict` value, and an `unknown` value. All three are expected to be real numbers in the range
zero to one, and have a combined sum of one. The greater the value for either the `accept` or `restrict` value, the
stronger the evidence a plugin is claiming for the respective outcome. The greater the `unknown` value, the weaker a
plugin is claiming its evidence is. Plugins may indicate that they have no evidence one way or the other by simply
returning nothing or by setting their decision's `unknown` component to its maximum value.

It is based on Dempster-Shafer theory, and a more advanced discussion of the decision structure and combination
algorithms may be found in the [decision explanation](https://bulwark.security/docs/explanation/decisions/).

## 🏰 What is Bulwark?

Bulwark is a fast, modern, open-source web application firewall (WAF) and API security gateway. It simplifies the
implementation of detective security controls while offering comprehensive visibility into your web services. Bulwark's
detection-as-code approach to rule definition offers security teams higher confidence in their response to persistent
and adaptive threats. Bulwark plugins offer a wide range of capabilities, enabling security teams to define and evolve
detections rapidly, without making changes to the underlying application.

## 🚀 Quickstart

The [`Decision`](https://docs.rs/bulwark-decision/latest/bulwark_decision/struct.Decision.html) is the main struct used
in this crate. The struct has a number of functions for constructing `Decision`s. For simple use-cases,
[`Decision::accepted`](https://docs.rs/bulwark-sdk/latest/bulwark_sdk/struct.Decision.html#method.accepted) and
[`Decision::restricted`](https://docs.rs/bulwark-sdk/latest/bulwark_sdk/struct.Decision.html#method.restricted) may
be used to convert from intuitive scalar "score" values to corresponding `Decision`s. These are appropriate for the vast
majority of applications.

```rust
use bulwark_decision::Decision;

let x = Decision::accepted(1.0); // Decision { accept: 1.0, restrict: 0.0, unknown: 0.0 })
let y = Decision::accepted(0.5); // Decision { accept: 0.5, restrict: 0.0, unknown: 0.5 })
let z = Decision::accepted(0.0); // Decision { accept: 0.0, restrict: 0.0, unknown: 1.0 })

let a = Decision::restricted(1.0); // Decision { accept: 0.0, restrict: 1.0, unknown: 0.0 })
let b = Decision::restricted(0.5); // Decision { accept: 0.0, restrict: 0.5, unknown: 0.5 })
let c = Decision::restricted(0.0); // Decision { accept: 0.0, restrict: 0.0, unknown: 1.0 })
```

Counter-based decisions may be constructed by simply converting each counter to a ratio, and then weighting the result
based on how predictive the counter actually is.

```rust
use bulwark_decision::Decision;

const WEIGHT: f64 = 0.25; // discount the decision, capping it to 0.25

let good_count = 90.0;
let bad_count = 10.0;
let x = Decision {
    accept: good_count / (good_count + bad_count),
    restrict: bad_count / (good_count + bad_count),
    unknown: 0.0,
}.weight(WEIGHT); // Decision { accept: 0.225, restrict: 0.025, unknown: 0.75 }
```

Bulwark uses the [`combine_murphy`](https://docs.rs/bulwark-sdk/latest/bulwark_sdk/struct.Decision.html#method.combine_murphy)
function to merge `Decision` values together. The crate also includes a public
[`combine_conjunctive`](https://docs.rs/bulwark-sdk/latest/bulwark_sdk/struct.Decision.html#method.combine_conjunctive)
function which may be useful for some applications, but it's primarily used internally by `combine_murphy`. Its
primary drawback is that it can return `NaN` for "high conflict" `Decision`s whereas `combine_murphy` won't.

The [`pignistic`](https://docs.rs/bulwark-sdk/latest/bulwark_sdk/struct.Decision.html#method.pignistic) method
is used when the representation of a `Decision` must be acted upon. Typically this is done after combination. It
reassigns all uncertainty in the `unknown` value to the two other components. The resulting `Decision` will have an
`unknown` component of zero. The remaining `restrict` value will be a risk score, while the `accept` value will be it's
inverse.

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
