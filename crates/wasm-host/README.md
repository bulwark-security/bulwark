# Bulwark WebAssembly Host

Automated security decision making under uncertainty.

## What is Bulwark?

Bulwark is a fast, modern, open-source web application security engine that makes it easier than ever to implement
resilient and observable security operations for your web services. It is designed around a user-friendly
detection-as-code pattern. Security teams can quickly compose powerful detections from reusable building-blocks
while unburdening product application logic from the increased complexity of domain-specific controls.

A complete overview may be found in Bulwark's [documentation](https://docs.bulwark.security/).

## WebAssembly Host

Bulwark's WebAssembly (WASM) host environment loads and compiles plugins, tracks a request context for each
incoming request and plugin, and provides implementations of the host functions needed by Bulwark plugin guests.
