# Bulwark Envoy External Processor

Automated security decision making under uncertainty.

## What is Bulwark?

Bulwark is a fast, modern, open-source web application security engine that makes it easier than ever to implement
resilient and observable security operations for your web services. It is designed around a user-friendly
detection-as-code pattern. Security teams can quickly compose powerful detections from reusable building-blocks
while unburdening product application logic from the increased complexity of domain-specific controls.

A complete overview may be found in Bulwark's [documentation](https://docs.bulwark.security/).

## External Processor

The `bulwark-ext-processor` crate is responsible for exposing a service that implements the
[Envoy external processing API](https://www.envoyproxy.io/docs/envoy/latest/api-v3/extensions/filters/http/ext_proc/v3/ext_proc.proto).
It connects Envoy to Bulwark's WebAssembly host environment.
