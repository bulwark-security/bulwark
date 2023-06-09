use {
    std::{net::IpAddr, str, str::FromStr},
    validator::Validate,
};

// For some reason, doc-tests in this module trigger a linker error, so they're set to no_run

use crate::bulwark_host::DecisionInterface;

pub use crate::{Decision, Outcome};
pub use http::{Extensions, Method, StatusCode, Uri, Version};
pub use serde_json::{Map, Value};

/// An HTTP request combines a head consisting of a [`Method`], [`Uri`], and headers with a [`BodyChunk`], which provides
/// access to the first chunk of a request body.
pub type Request = http::Request<BodyChunk>;
/// An HTTP response combines a head consisting of a [`StatusCode`] and headers with a [`BodyChunk`], which provides
/// access to the first chunk of a response body.
pub type Response = http::Response<BodyChunk>;

// NOTE: fields are documented via Markdown instead of normal rustdoc because the underlying type is from the macro.
/// A `Breaker` contains the values needed to implement a circuit-breaker pattern within a plugin.
///
/// # Fields
///
/// * `generation` - The number of times a breaker has been incremented within the expiration window.
/// * `successes` - The number of total success outcomes tracked within the expiration window.
/// * `failures` - The number of total failure outcomes tracked within the expiration window.
/// * `consecutive_successes` - The number of consecutive success outcomes.
/// * `consecutive_failures` - The number of consecutive failure outcomes.
/// * `expiration` - The expiration timestamp in seconds since the epoch.
pub type Breaker = crate::bulwark_host::BreakerInterface;
/// A `Rate` contains the values needed to implement a rate-limiter pattern within a plugin.
///
/// # Fields
///
/// * `attempts` - The number of attempts made within the expiration window.
/// * `expiration` - The expiration timestamp in seconds since the epoch.
pub type Rate = crate::bulwark_host::RateInterface;

/// The number of successes or failures to increment the breaker by.
pub enum BreakerDelta {
    Success(i64),
    Failure(i64),
}

/// The first chunk of an HTTP body.
///
/// Bulwark does not send the entire body to the guest plugin environment. This limitation limits the impact of
/// copying a large number of bytes from the host into guest VMs. A full body copy would be required for each
/// plugin for every request or response otherwise.
///
/// This has consequences for any plugin that wants to parse the body it receives. Some data formats like JSON
/// may be significantly more difficult to work with if only partially received, and streaming parsers which may be
/// more tolerant to trunctation are recommended in such cases. There will be some situations where this limitation
/// prevents useful parsing entirely and plugins may need to make use of the `unknown` result value to express this.
pub struct BodyChunk {
    pub end_of_stream: bool,
    pub size: u64,
    pub start: u64,
    // TODO: use bytes crate to avoid copies
    pub content: Vec<u8>,
}

/// An empty HTTP body
pub const NO_BODY: BodyChunk = BodyChunk {
    end_of_stream: true,
    size: 0,
    start: 0,
    content: vec![],
};

// TODO: might need either get_remote_addr or an extension on the request for non-forwarded IP address

/// Returns the incoming request.
pub fn get_request() -> Request {
    let raw_request: crate::bulwark_host::RequestInterface = crate::bulwark_host::get_request();
    let chunk: Vec<u8> = raw_request.chunk;
    // This code shouldn't be reachable if the method is invalid
    let method = Method::from_str(raw_request.method.as_str()).expect("should be a valid method");
    let mut request = http::Request::builder()
        .method(method)
        .uri(raw_request.uri)
        .version(match raw_request.version.as_str() {
            "HTTP/0.9" => http::Version::HTTP_09,
            "HTTP/1.0" => http::Version::HTTP_10,
            "HTTP/1.1" => http::Version::HTTP_11,
            "HTTP/2.0" => http::Version::HTTP_2,
            "HTTP/3.0" => http::Version::HTTP_3,
            _ => http::Version::HTTP_11,
        });
    for (name, value) in raw_request.headers {
        request = request.header(name, value);
    }
    request
        .body(BodyChunk {
            content: chunk,
            size: raw_request.chunk_length,
            start: raw_request.chunk_start,
            end_of_stream: raw_request.end_of_stream,
        })
        // Everything going into the builder should have already been validated somewhere else
        // Proxy layer shouldn't send it through if it's invalid
        .expect("should be a valid request")
}

/// Returns the response received from the interior service.
pub fn get_response() -> Option<Response> {
    let raw_response: crate::bulwark_host::ResponseInterface = crate::bulwark_host::get_response()?;
    let chunk: Vec<u8> = raw_response.chunk;
    let status = raw_response.status as u16;
    let mut response = http::Response::builder().status(status);
    for (name, value) in raw_response.headers {
        response = response.header(name, value);
    }
    Some(
        response
            .body(BodyChunk {
                content: chunk,
                size: raw_response.chunk_length,
                start: raw_response.chunk_start,
                end_of_stream: raw_response.end_of_stream,
            })
            // Everything going into the builder should have already been validated somewhere else
            // Proxy layer shouldn't send it through if it's invalid
            .expect("should be a valid response"),
    )
}

/// Returns the originating client's IP address, if available.
pub fn get_client_ip() -> Option<IpAddr> {
    crate::bulwark_host::get_client_ip().map(|ip| ip.into())
}

/// Returns a named value from the request context's params.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the param value.
pub fn get_param_value(key: &str) -> Result<Value, crate::Error> {
    let raw_value = crate::bulwark_host::get_param_value(key)?;
    let value: serde_json::Value = serde_json::from_slice(&raw_value).unwrap();
    Ok(value)
}

/// Set a named value in the request context's params.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the param value.
/// * `value` - The value to record. Values are serialized JSON.
pub fn set_param_value(key: &str, value: Value) -> Result<(), crate::Error> {
    let json = serde_json::to_vec(&value)?;
    crate::bulwark_host::set_param_value(key, &json)?;
    Ok(())
}

/// Returns the guest environment's configuration value as a JSON [`Value`].
///
/// By convention this will return a [`Value::Object`].
pub fn get_config() -> Value {
    let raw_config = crate::bulwark_host::get_config();
    serde_json::from_slice(&raw_config).unwrap()
}

/// Returns a named guest environment configuration value as a JSON [`Value`].
///
/// A shortcut for calling [`get_config`], reading it as an `Object`, and then retrieving a named [`Value`] from it.
///
/// # Arguments
///
/// * `key` - A key indexing into a configuration [`Map`]
pub fn get_config_value(key: &str) -> Option<Value> {
    // TODO: this should return a result
    let raw_config = crate::bulwark_host::get_config();
    let object: serde_json::Value = serde_json::from_slice(&raw_config).unwrap();
    match object {
        Value::Object(v) => v.get(&key.to_string()).cloned(),
        _ => panic!("unexpected config value"),
    }
}

/// Returns a named environment variable value as a [`String`].
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the environment variable being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The environment variable name. Case-sensitive.
pub fn get_env(key: &str) -> Result<String, crate::EnvVarError> {
    Ok(String::from_utf8(crate::bulwark_host::get_env_bytes(key)?)?)
}

/// Returns a named environment variable value as bytes.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the environment variable being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The environment variable name. Case-sensitive.
pub fn get_env_bytes(key: &str) -> Result<Vec<u8>, crate::EnvVarError> {
    Ok(crate::bulwark_host::get_env_bytes(key)?)
}

/// Records the decision value the plugin wants to return.
///
/// # Arguments
///
/// * `decision` - The [`Decision`] output of the plugin.
pub fn set_decision(decision: Decision) -> Result<(), crate::Error> {
    // Validate here because it should provide a better error than the one that the host will give.
    decision.validate()?;
    crate::bulwark_host::set_decision(DecisionInterface {
        accept: decision.accept,
        restrict: decision.restrict,
        unknown: decision.unknown,
    })
    .expect("should not be able to produce an invalid result");
    Ok(())
}

/// Records a decision indicating that the plugin wants to accept a request.
///
/// This function is sugar for `set_decision(Decision { value, 0.0, 0.0 }.scale())`
/// If used with a 1.0 value it should be given a weight in its config.
///
/// # Arguments
///
/// * `value` - The `accept` value to set.
pub fn set_accepted(value: f64) {
    crate::bulwark_host::set_decision(
        Decision {
            accept: value,
            restrict: 0.0,
            unknown: 0.0,
        }
        .scale()
        .into(),
    )
    .expect("should not be able to produce an invalid result");
}

/// Records a decision indicating that the plugin wants to restrict a request.
///
/// This function is sugar for `set_decision(Decision { 0.0, value, 0.0 }.scale())`.
/// If used with a 1.0 value it should be given a weight in its config.
///
/// # Arguments
///
/// * `value` - The `restrict` value to set.
pub fn set_restricted(value: f64) {
    crate::bulwark_host::set_decision(
        Decision {
            accept: 0.0,
            restrict: value,
            unknown: 0.0,
        }
        .scale()
        .into(),
    )
    .expect("should not be able to produce an invalid result");
}

/// Records the tags the plugin wants to associate with its decision.
///
/// # Arguments
///
/// * `tags` - The list of tags to associate with a [`Decision`]
///
/// # Examples
///
/// All of these are valid:
///
/// ```no_run
/// use bulwark_wasm_sdk::set_tags;
///
/// set_tags(["tag"]);
/// set_tags(vec!["tag"]);
/// set_tags([String::from("tag")]);
/// set_tags(vec![String::from("tag")]);
/// // Clear tags, rarely needed
/// set_tags::<[_; 0], String>([]);
/// set_tags::<Vec<_>, String>(vec![]);
/// ```
#[inline]
pub fn set_tags<I: IntoIterator<Item = V>, V: Into<String>>(tags: I) {
    let tags: Vec<String> = tags.into_iter().map(|s| s.into()).collect();
    let tags: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();
    crate::bulwark_host::set_tags(tags.as_slice())
}

/// Records additional tags the plugin wants to associate with its decision.
///
/// # Arguments
///
/// * `tags` - The list of additional tags to associate with a [`Decision`]
///
/// # Examples
///
/// All of these are valid:
///
/// ```no_run
/// use bulwark_wasm_sdk::append_tags;
///
/// append_tags(["tag"]);
/// append_tags(vec!["tag"]);
/// append_tags([String::from("tag")]);
/// append_tags(vec![String::from("tag")]);
/// ```
#[inline]
pub fn append_tags<I: IntoIterator<Item = V>, V: Into<String>>(tags: I) -> Vec<String> {
    let tags: Vec<String> = tags.into_iter().map(|s| s.into()).collect();
    let tags: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();
    crate::bulwark_host::append_tags(tags.as_slice())
}

/// Returns the combined decision, if available.
///
/// Typically used in the feedback phase.
pub fn get_combined_decision() -> Option<Decision> {
    crate::bulwark_host::get_combined_decision().map(|decision| decision.into())
}

/// Returns the combined set of tags associated with a decision, if available.
///
/// Typically used in the feedback phase.
#[inline]
pub fn get_combined_tags() -> Option<Vec<String>> {
    crate::bulwark_host::get_combined_tags()
}

/// Returns the outcome of the combined decision, if available.
///
/// Typically used in the feedback phase.
pub fn get_outcome() -> Option<Outcome> {
    crate::bulwark_host::get_outcome().map(|outcome| outcome.into())
}

/// Sends an outbound HTTP request.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the host being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `request` - The HTTP request to send.
pub fn send_request(request: Request) -> Result<Response, crate::HttpError> {
    let request = crate::bulwark_host::RequestInterface::from(request);
    Ok(Response::from(crate::bulwark_host::send_request(&request)?))
}

/// Returns the named state value retrieved from Redis.
///
/// Also used to retrieve a counter value.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
#[inline]
pub fn get_remote_state(key: &str) -> Result<Vec<u8>, crate::RemoteStateError> {
    Ok(crate::bulwark_host::get_remote_state(key)?)
}

/// Parses a counter value from state stored as a string.
///
/// # Arguments
///
/// * `value` - The string representation of a counter.
#[inline]
pub fn parse_counter(value: Vec<u8>) -> Result<i64, crate::ParseCounterError> {
    Ok(str::from_utf8(value.as_slice())?.parse::<i64>()?)
}

/// Set a named value in Redis.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
/// * `value` - The value to record. Values are byte strings, but may be interpreted differently by Redis depending on context.
#[inline]
pub fn set_remote_state(key: &str, value: &[u8]) -> Result<(), crate::RemoteStateError> {
    Ok(crate::bulwark_host::set_remote_state(key, value)?)
}

/// Increments a named counter in Redis.
///
/// Returns the value of the counter after it's incremented.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
#[inline]
pub fn increment_remote_state(key: &str) -> Result<i64, crate::RemoteStateError> {
    Ok(crate::bulwark_host::increment_remote_state(key)?)
}

/// Increments a named counter in Redis by a specified delta value.
///
/// Returns the value of the counter after it's incremented.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
/// * `delta` - The amount to increase the counter by.
#[inline]
pub fn increment_remote_state_by(key: &str, delta: i64) -> Result<i64, crate::RemoteStateError> {
    Ok(crate::bulwark_host::increment_remote_state_by(key, delta)?)
}

/// Sets an expiration on a named value in Redis.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
/// * `ttl` - The time-to-live for the value in seconds.
#[inline]
pub fn set_remote_ttl(key: &str, ttl: i64) -> Result<(), crate::RemoteStateError> {
    Ok(crate::bulwark_host::set_remote_ttl(key, ttl)?)
}

// TODO: needs an example
/// Increments a rate limit, returning the number of attempts so far and the expiration time.
///
/// The rate limiter is a counter over a period of time. At the end of the period, it will expire,
/// beginning a new period. Window periods should be set to the longest amount of time that a client should
/// be locked out for. The plugin is responsible for performing all rate-limiting logic with the counter
/// value it receives.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
/// * `delta` - The amount to increase the counter by.
/// * `window` - How long each period should be in seconds.
#[inline]
pub fn increment_rate_limit(
    key: &str,
    delta: i64,
    window: i64,
) -> Result<Rate, crate::RemoteStateError> {
    Ok(crate::bulwark_host::increment_rate_limit(
        key, delta, window,
    )?)
}

/// Checks a rate limit, returning the number of attempts so far and the expiration time.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will panic if permission has not been granted.
///
/// See [`increment_rate_limit`].
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
#[inline]
pub fn check_rate_limit(key: &str) -> Result<Rate, crate::RemoteStateError> {
    Ok(crate::bulwark_host::check_rate_limit(key)?)
}

/// Increments a circuit breaker, returning the generation count, success count, failure count,
/// consecutive success count, consecutive failure count, and expiration time.
///
/// The plugin is responsible for performing all circuit-breaking logic with the counter
/// values it receives. The host environment does as little as possible to maximize how much
/// control the plugin has over the behavior of the breaker.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
/// * `delta` - The amount to increase the success or failure counter by.
/// * `window` - How long each period should be in seconds.
///
/// # Examples
///
/// ```no_run
/// use bulwark_wasm_sdk::*;
///
/// pub struct CircuitBreaker;
///
/// #[bulwark_plugin]
/// impl Handlers for CircuitBreaker {
///     fn on_response_decision() -> Result {
///         if let Some(ip) = get_client_ip() {
///             let key = format!("client.ip:{ip}");
///             // "failure" could be determined by other methods besides status code
///             let failure = get_response().map(|r| r.status().as_u16() >= 500).unwrap_or(true);
///             let breaker = increment_breaker(
///                 &key,
///                 if !failure {
///                     BreakerDelta::Success(1)
///                 } else {
///                     BreakerDelta::Failure(1)
///                 },
///                 60 * 60, // 1 hour
///             )?;
///             // use breaker here
///         }
///         Ok(())
///     }
/// }
/// ```
pub fn increment_breaker(
    key: &str,
    delta: BreakerDelta,
    window: i64,
) -> Result<Breaker, crate::RemoteStateError> {
    let (success_delta, failure_delta) = match delta {
        BreakerDelta::Success(d) => (d, 0),
        BreakerDelta::Failure(d) => (0, d),
    };
    Ok(crate::bulwark_host::increment_breaker(
        key,
        success_delta,
        failure_delta,
        window,
    )?)
}

/// Checks a circuit breaker, returning the generation count, success count, failure count,
/// consecutive success count, consecutive failure count, and expiration time.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will panic if permission has not been granted.
///
/// See [`increment_breaker`].
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
#[inline]
pub fn check_breaker(key: &str) -> Result<Breaker, crate::RemoteStateError> {
    Ok(crate::bulwark_host::check_breaker(key)?)
}
