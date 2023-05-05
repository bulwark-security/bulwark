// TODO: the host/guest wit files seem to be why the latest version switched to one generate macro?
// TODO: switch to wasmtime::component::bindgen!
wit_bindgen_rust::import!("../../bulwark-host.wit");

use {
    std::{
        net::{IpAddr, Ipv4Addr, Ipv6Addr},
        str::FromStr,
    },
    validator::{Validate, ValidationErrors},
};

use self::bulwark_host::DecisionInterface;

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
pub type Breaker = bulwark_host::BreakerInterface;
/// A `Rate` contains the values needed to implement a rate-limiter pattern within a plugin.
///
/// # Fields
///
/// * `attempts` - The number of attempts made within the expiration window.
/// * `expiration` - The expiration timestamp in seconds since the epoch.
pub type Rate = bulwark_host::RateInterface;

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

impl From<bulwark_host::ResponseInterface> for Response {
    fn from(response: bulwark_host::ResponseInterface) -> Self {
        let mut builder = http::response::Builder::new();
        builder = builder.status::<u16>(response.status.try_into().unwrap());
        for bulwark_host::HeaderInterface { name, value } in response.headers {
            builder = builder.header(name, value);
        }
        builder
            .body(BodyChunk {
                end_of_stream: true,
                size: response.chunk.len().try_into().unwrap(),
                start: 0,
                content: response.chunk,
            })
            .unwrap()
    }
}

impl From<bulwark_host::IpInterface> for IpAddr {
    fn from(ip: bulwark_host::IpInterface) -> Self {
        match ip {
            bulwark_host::IpInterface::V4(v4) => Self::V4(Ipv4Addr::new(v4.0, v4.1, v4.2, v4.3)),
            bulwark_host::IpInterface::V6(v6) => Self::V6(Ipv6Addr::new(
                v6.0, v6.1, v6.2, v6.3, v6.4, v6.5, v6.6, v6.7,
            )),
        }
    }
}

// TODO: can we avoid conversions, perhaps by moving bindgen into lib.rs?

impl From<bulwark_host::DecisionInterface> for Decision {
    fn from(decision: bulwark_host::DecisionInterface) -> Self {
        Decision {
            accept: decision.accept,
            restrict: decision.restrict,
            unknown: decision.unknown,
        }
    }
}

impl From<bulwark_host::OutcomeInterface> for Outcome {
    fn from(outcome: bulwark_host::OutcomeInterface) -> Self {
        match outcome {
            bulwark_host::OutcomeInterface::Trusted => Outcome::Trusted,
            bulwark_host::OutcomeInterface::Accepted => Outcome::Accepted,
            bulwark_host::OutcomeInterface::Suspected => Outcome::Suspected,
            bulwark_host::OutcomeInterface::Restricted => Outcome::Restricted,
        }
    }
}

// NOTE: `pub use` pattern would be better than inlined functions, but apparently that can't be rustdoc'd?

// TODO: might need either get_remote_addr or an extension on the request for non-forwarded IP address

/// Returns the guest environment's configuration value as a JSON [`Value`].
///
/// By convention this will return a [`Value::Object`].
pub fn get_config() -> Value {
    let raw_config = bulwark_host::get_config();
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
    let raw_config = bulwark_host::get_config();
    let object: serde_json::Value = serde_json::from_slice(&raw_config).unwrap();
    match object {
        Value::Object(v) => v.get(&key.to_string()).cloned(),
        _ => panic!("unexpected config value"),
    }
}

/// Returns a named value from the request context's params.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the param value.
pub fn get_param_value(key: &str) -> Value {
    // TODO: this should return a result
    let raw_value = bulwark_host::get_param_value(key);
    let value: serde_json::Value = serde_json::from_slice(&raw_value).unwrap();
    value
}

/// Set a named value in the request context's params.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the param value.
/// * `value` - The value to record. Values are serialized JSON.
pub fn set_param_value(key: &str, value: Value) {
    // TODO: this should return a result
    let json = serde_json::to_vec(&value).unwrap();
    bulwark_host::set_param_value(key, &json);
}

/// Returns a named environment variable value as a [`String`].
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the environment variable being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The environment variable name. Case-sensitive.
pub fn get_env(key: &str) -> String {
    // TODO: this should return a result
    String::from_utf8(bulwark_host::get_env_bytes(key)).unwrap()
}

/// Returns a named environment variable value as bytes.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the environment variable being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The environment variable name. Case-sensitive.
pub fn get_env_bytes(key: &str) -> Vec<u8> {
    // TODO: this should return a result
    bulwark_host::get_env_bytes(key)
}

/// Returns the incoming request.
pub fn get_request() -> Request {
    let raw_request: bulwark_host::RequestInterface = bulwark_host::get_request();
    let chunk: Vec<u8> = raw_request.chunk;
    // TODO: error handling
    let method = Method::from_str(raw_request.method.as_str()).unwrap();
    let mut request = http::Request::builder()
        .method(method)
        .uri(raw_request.uri)
        .version(http::Version::HTTP_11); // TODO: don't hard-code version
    for header in raw_request.headers {
        request = request.header(header.name, header.value);
    }
    request
        .body(BodyChunk {
            content: chunk,
            size: raw_request.chunk_length,
            start: raw_request.chunk_start,
            end_of_stream: raw_request.end_of_stream,
        })
        .unwrap()
}

/// Returns the response received from the interior service.
pub fn get_response() -> Response {
    let raw_response: bulwark_host::ResponseInterface = bulwark_host::get_response();
    let chunk: Vec<u8> = raw_response.chunk;
    // TODO: error handling
    let status: u16 = raw_response.status.try_into().unwrap();
    let mut response = http::Response::builder().status(status);
    for header in raw_response.headers {
        response = response.header(header.name, header.value);
    }
    response
        .body(BodyChunk {
            content: chunk,
            size: raw_response.chunk_length,
            start: raw_response.chunk_start,
            end_of_stream: raw_response.end_of_stream,
        })
        .unwrap()
}

/// Returns the originating client's IP address, if available.
pub fn get_client_ip() -> Option<IpAddr> {
    bulwark_host::get_client_ip().map(|ip| ip.into())
}

/// Sends an outbound HTTP request.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the host being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `request` - The HTTP request to send.
pub fn send_request(request: Request) -> Response {
    let request_id = bulwark_host::prepare_request(
        request.method().as_str(),
        request.uri().to_string().as_str(),
    );
    for (name, value) in request.headers() {
        bulwark_host::add_request_header(request_id, name.as_str(), value.as_bytes());
    }
    let chunk = request.body();
    if !chunk.end_of_stream {
        panic!("the entire request body must be available");
    } else if chunk.start != 0 {
        panic!("chunk start must be 0");
    } else if chunk.size > 16384 {
        panic!("the entire request body must be 16384 bytes or less");
    }
    let response = bulwark_host::set_request_body(request_id, &chunk.content);
    Response::from(response)
}

/// Records the decision value the plugin wants to return.
///
/// # Arguments
///
/// * `decision` - The [`Decision`] output of the plugin.
pub fn set_decision(decision: Decision) -> Result<(), ValidationErrors> {
    decision.validate()?;
    bulwark_host::set_decision(DecisionInterface {
        accept: decision.accept,
        restrict: decision.restrict,
        unknown: decision.unknown,
    });
    Ok(())
}

/// Records a decision indicating that the plugin wants to accept a request.
///
/// This function is sugar for `set_decision(1.0, 0.0, 0.0)` and should be used in conjunction with a weight value.
pub fn set_accepted() {
    bulwark_host::set_decision(DecisionInterface {
        accept: 1.0,
        restrict: 0.0,
        unknown: 0.0,
    });
}

/// Records a decision indicating that the plugin wants to restrict a request.
///
/// This function is sugar for `set_decision(0.0, 1.0, 0.0)` and should be used in conjunction with a weight value.
pub fn set_restricted() {
    bulwark_host::set_decision(DecisionInterface {
        accept: 1.0,
        restrict: 0.0,
        unknown: 0.0,
    });
}

/// Records the tags the plugin wants to associate with its decision.
///
/// # Arguments
///
/// * `tags` - The list of tags to associate with a [`Decision`]
#[inline]
pub fn set_tags(tags: &[&str]) {
    // TODO: use BTreeSet for merging sorted tag lists?
    bulwark_host::set_tags(tags)
}

/// Returns the combined decision, if available.
///
/// Typically used in the feedback phase.
pub fn get_combined_decision() -> Decision {
    // TODO: Option<Decision> ?
    bulwark_host::get_combined_decision().into()
}

/// Returns the combined set of tags associated with a decision, if available.
///
/// Typically used in the feedback phase.
#[inline]
pub fn get_combined_tags() -> Vec<String> {
    bulwark_host::get_combined_tags()
}

/// Returns the outcome of the combined decision, if available.
///
/// Typically used in the feedback phase.
pub fn get_outcome() -> Outcome {
    // TODO: Option<Outcome> ?
    bulwark_host::get_outcome().into()
}

/// Returns the named state value retrieved from Redis.
///
/// Also used to retrieve a counter value.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
#[inline]
pub fn get_remote_state(key: &str) -> Vec<u8> {
    bulwark_host::get_remote_state(key)
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
pub fn set_remote_state(key: &str, value: &[u8]) {
    bulwark_host::set_remote_state(key, value)
}

/// Increments a named counter in Redis.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
#[inline]
pub fn increment_remote_state(key: &str) -> i64 {
    bulwark_host::increment_remote_state(key)
}

/// Increments a named counter in Redis by a specified delta value.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will panic if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
/// * `delta` - The amount to increase the counter by.
#[inline]
pub fn increment_remote_state_by(key: &str, delta: i64) -> i64 {
    bulwark_host::increment_remote_state_by(key, delta)
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
pub fn set_remote_ttl(key: &str, ttl: i64) {
    bulwark_host::set_remote_ttl(key, ttl)
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
pub fn increment_rate_limit(key: &str, delta: i64, window: i64) -> Rate {
    bulwark_host::increment_rate_limit(key, delta, window)
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
pub fn check_rate_limit(key: &str) -> Rate {
    bulwark_host::check_rate_limit(key)
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
/// * `success_delta` - The amount to increase the success counter by. Generally zero on failure.
/// * `failure_delta` - The amount to increase the failure counter by. Generally zero on success.
/// * `window` - How long each period should be in seconds.
#[inline]
pub fn increment_breaker(
    key: &str,
    success_delta: i64,
    failure_delta: i64,
    window: i64,
) -> Breaker {
    bulwark_host::increment_breaker(key, success_delta, failure_delta, window)
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
pub fn check_breaker(key: &str) -> Breaker {
    bulwark_host::check_breaker(key)
}
