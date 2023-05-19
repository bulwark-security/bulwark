# Bulwark Decisions

Automated security decision making under uncertainty.

## What is Bulwark?

Bulwark is a fast, modern, open-source web application security engine that makes it easier than ever to implement
resilient and observable security operations for your web services. It is designed around a user-friendly
detection-as-code pattern. Security teams can quickly compose powerful detections from reusable building-blocks
while unburdening product application logic from the increased complexity of domain-specific controls.

A complete overview may be found in Bulwark's [documentation](https://docs.bulwark.security/).

## Decision

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
algorithms may be found in the
[decision internals](https://docs.bulwark.security/introduction/core-concepts/decision-internals) documentation.
