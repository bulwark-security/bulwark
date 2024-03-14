[![Bulwark Logo](/docs/assets/bulwark-hero.png)](https://bulwark.security/)

---

Automated security decision-making under uncertainty.

## 🏰 What is Bulwark?

Bulwark is a fast, modern, open-source web application firewall and API security gateway that simplifies the
implementation of detective security controls while offering comprehensive visibility into your web services. Bulwark's
detection-as-code approach to rule definition offers security teams higher confidence in their response to persistent
and adaptive threats. Bulwark plugins offer a wide range of capabilities, enabling security teams to define and evolve
detections rapidly, without making changes to the underlying application.

- **Open Source.** Freely available under a permissive Apache 2.0 license. We're committed to keeping it that way.
- **Extensible.** Define custom detection logic using the SDK, or customize reusable parameterized detections to
    your needs, without writing code.
- **Observable.** Gain deep visibility into both your traffic and the operation of your detections with built-in
    observability features. Enrich your traffic data for improved context.
- **Flexible.** Bulwark's plugin API enables detections to interact with Redis state and other services, including
    machine learning models. Plugins can perform their detections collaboratively using Bulwark's ensemble decisions.
    Securely decrypt session cookies to operate on application-level information.
- **Testable.** Detections can have automated tests. Prevent misclassifications from making it to production.
- **Accurate.** Bulwark has built-in mechanisms to help tune detections for high accuracy, minimize false
    positives, and quickly remediate accuracy issues if they occur. Meet compliance requirements for detective controls
    while avoiding false positives that would disrupt operations.
- **Sandboxed.** Every detection runs inside a secure WebAssembly sandbox, isolating detection logic, and ensuring that
    access never exceeds its permissions grants.
- **Safe.** Deploy in observe-only mode and build confidence in the system before enabling request blocking.
- **Commercially Supported.** Bulwark is actively developed and supported by
    [Bulwark Security PBC](https://bulwark.security/). We intend to offer a Bulwark cloud admin application, an
    advanced commercial ruleset, paid support, and professional services, in addition to the freely-available
    [community ruleset](https://github.com/bulwark-security/bulwark-community-ruleset). Contact us
    ([sales@bulwark.security](mailto:sales@bulwark.security)) if you are interested in early access as a design partner
    for any of these commercial offerings.

## 🕵️ Use Cases

- **Account Takeover:** Detect patterns of abuse like credential stuffing, password spraying, session hijacking,
    and phishing that target account login pages, authentication APIs, or make use of stolen cookies.
- **Site Scraping:** Identify and block bots that are ignoring `robots.txt` or scraping site data at abnormal
    frequencies, without negatively affecting well-behaved bots and crawlers. This is especially relevant for
    users that may be concerned about their sites being incorporated into training data without authorization.
- **Free-Tier Abuse:** Prevent abusive free-tier usage from making such offerings unsustainable. Send the results
    of runtime detective controls to a Bulwark plugin, where it can be combined with CAPTCHAs and other signals
    to help identify these behaviors earlier, before they consume limited resources.
- **Combine Fraud Signals:** Take advantage of Bulwark's caching and its ability to access external services to
    seamlessly combine fraud scoring from independent vendors. Protect interior services from high-volume
    automated fraud like card testing that may otherwise affect availability.

## 💻 Installation

1. [Install the Rust toolchain via rustup.](https://www.rust-lang.org/tools/install)
2. Install the `wasm32-wasi` target needed to build plugins: `rustup target add wasm32-wasi`
3. Install Bulwark: `cargo install bulwark-cli`

## 💪 Contributing

Check out the list of [open issues](https://github.com/bulwark-security/bulwark/issues). We actively maintain a
list of [issues suitable for new contributors][good-first-issue] to the project. Alternatively, detection plugins
may be contributed to the [community ruleset](https://github.com/bulwark-security/bulwark-community-ruleset).

We do not require contributors to sign a CLA because we want users of Bulwark to be confident that the software will
remain available under its current license.

[good-first-issue]: https://github.com/bulwark-security/bulwark/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22

## 🛟 Getting Help

To start, check if the answer to your question can be found in any of the [guides](https://docs.bulwark.security/guides)
or [API documentation](https://docs.bulwark.security/api). If you aren't able to find an answer there, check
the Bulwark project's [discussion forum](https://github.com/bulwark-security/bulwark/discussions). We would love to try
to answer your question.
