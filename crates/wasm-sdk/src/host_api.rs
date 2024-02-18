use {
    std::{collections::HashMap, net::IpAddr, str, str::FromStr},
    validator::Validate,
};

// For some reason, doc-tests in this module trigger a linker error, so they're set to no_run

pub use crate::{Decision, Error, Outcome};
pub use http::{Extensions, Method, StatusCode, Uri, Version};
pub use serde_json::json as value;
pub use serde_json::{Map, Value};

/// A type alias. See [`bytes::Bytes`] for details.
pub type Bytes = bytes::Bytes;
/// An HTTP request combines a head consisting of a [`Method`], [`Uri`], and headers with a [`BodyChunk`], which provides
/// access to the first chunk of a request body.
pub type Request = http::Request<Bytes>;
/// An HTTP request builder type alias. See [`http::request::Builder`] for details.
pub type RequestBuilder = http::request::Builder;
/// An HTTP response combines a head consisting of a [`StatusCode`] and headers with a [`BodyChunk`], which provides
/// access to the first chunk of a response body.
pub type Response = http::Response<Bytes>;
/// An HTTP response builder type alias. See [`http::response::Builder`] for details.
pub type ResponseBuilder = http::response::Builder;
/// An HTTP uri builder type alias. See [`http::uri::Builder`] for details.
pub type UriBuilder = http::uri::Builder;

// TODO: perhaps something more like http::Request<Box<dyn AsyncRead + Sync + Send + Unpin>>?
// TODO: or hyper::Request<HyperIncomingBody> to match WasiHttpView's new_incoming_request?

// /// The first chunk of an HTTP body.
// ///
// /// Bulwark does not send the entire body to the guest plugin environment. This limitation limits the impact of
// /// copying a large number of bytes from the host into guest VMs. Copying the full body for each plugin for every
// /// request or response would otherwise make detection times too unpredictable.
// ///
// /// This has consequences for any plugin that wants to parse the body it receives. Some data formats like JSON
// /// may be significantly more difficult to work with if only partially received, and streaming parsers which may be
// /// more tolerant to trunctation are recommended in such cases. There will be some situations where this limitation
// /// prevents useful parsing entirely and plugins may need to make use of the `unknown` result value to express this.
// #[derive(Clone)]
// pub struct BodyChunk {
//     pub received: bool,
//     pub end_of_stream: bool,
//     pub size: u64,
//     pub start: u64,
//     // TODO: use bytes crate to minimize copies
//     pub content: Vec<u8>,
// }

// /// An empty HTTP body
// pub const NO_BODY: BodyChunk = BodyChunk {
//     received: true,
//     end_of_stream: true,
//     size: 0,
//     start: 0,
//     content: vec![],
// };

// /// An unavailable HTTP body
// pub const UNAVAILABLE_BODY: BodyChunk = BodyChunk {
//     received: false,
//     end_of_stream: true,
//     size: 0,
//     start: 0,
//     content: vec![],
// };

/// A `HandlerOutput` represents a decision and associated output for a single handler within a single detection.
#[derive(Clone, Default)]
pub struct HandlerOutput {
    /// The `params` value represents the new params to enrich the request with.
    pub params: HashMap<String, String>,
    /// The `decision` value represents the combined numerical decision from multiple detections.
    pub decision: Decision,
    /// The `tags` value represents the new tags to annotate the request with.
    pub tags: Vec<String>,
}

/// A `Verdict` represents a combined decision across multiple detections.
#[derive(Clone)]
pub struct Verdict {
    /// The `decision` value represents the combined numerical decision from multiple detections.
    pub decision: Decision,
    /// The `outcome` value represents a comparison of the numerical decision against a set of thresholds.
    pub outcome: Outcome,
    /// The `tags` value represents the merged tags used to annotate the request.
    pub tags: Vec<String>,
}

// TODO: might need either get_remote_addr or an extension on the request for non-forwarded IP address

// /// Returns the incoming request.
// pub fn get_request() -> Request {
//     let raw_request: crate::bulwark_host::RequestInterface = crate::bulwark_host::get_request();
//     let chunk: Vec<u8> = raw_request.chunk;
//     // This code shouldn't be reachable if the method is invalid
//     let method = Method::from_str(raw_request.method.as_str()).expect("should be a valid method");
//     let mut request = http::Request::builder()
//         .method(method)
//         .uri(raw_request.uri)
//         .version(match raw_request.version.as_str() {
//             "HTTP/0.9" => http::Version::HTTP_09,
//             "HTTP/1.0" => http::Version::HTTP_10,
//             "HTTP/1.1" => http::Version::HTTP_11,
//             "HTTP/2.0" => http::Version::HTTP_2,
//             "HTTP/3.0" => http::Version::HTTP_3,
//             _ => http::Version::HTTP_11,
//         });
//     for (name, value) in raw_request.headers {
//         request = request.header(name, value);
//     }
//     request
//         .body(BodyChunk {
//             received: raw_request.body_received,
//             content: chunk,
//             size: raw_request.chunk_length,
//             start: raw_request.chunk_start,
//             end_of_stream: raw_request.end_of_stream,
//         })
//         // Everything going into the builder should have already been validated somewhere else
//         // Proxy layer shouldn't send it through if it's invalid
//         .expect("should be a valid request")
// }

// /// Returns the response received from the interior service.
// pub fn get_response() -> Option<Response> {
//     let raw_response: crate::bulwark_host::ResponseInterface = crate::bulwark_host::get_response()?;
//     let chunk: Vec<u8> = raw_response.chunk;
//     let status = raw_response.status as u16;
//     let mut response = http::Response::builder().status(status);
//     for (name, value) in raw_response.headers {
//         response = response.header(name, value);
//     }
//     Some(
//         response
//             .body(BodyChunk {
//                 received: raw_response.body_received,
//                 content: chunk,
//                 size: raw_response.chunk_length,
//                 start: raw_response.chunk_start,
//                 end_of_stream: raw_response.end_of_stream,
//             })
//             // Everything going into the builder should have already been validated somewhere else
//             // Proxy layer shouldn't send it through if it's invalid
//             .expect("should be a valid response"),
//     )
// }

// /// Determines whether the `on_request_body_decision` handler will be called with a request body or not.
// ///
// /// The [`bulwark_plugin`](bulwark_wasm_sdk_macros::bulwark_plugin) macro will automatically call this function
// /// within an auto-generated `on_init` handler. Normally, plugin authors do not need to call it directly.
// /// However, the default may be overriden if a plugin intends to cancel processing of the request body despite
// /// having a handler available for processing it.
// ///
// /// However, if the `on_init` handler is replaced, this function will need to be called manually. Most plugins
// /// will not need to do this.
// pub fn receive_request_body(body: bool) {
//     crate::bulwark_host::receive_request_body(body)
// }

// /// Determines whether the `on_response_body_decision` handler will be called with a response body or not.
// ///
// /// The [`bulwark_plugin`](bulwark_wasm_sdk_macros::bulwark_plugin) macro will automatically call this function
// /// within an auto-generated `on_init` handler. Normally, plugin authors do not need to call it directly.
// /// However, the default may be overriden if a plugin intends to cancel processing of the response body despite
// /// having a handler available for processing it.
// ///
// /// However, if the `on_init` handler is replaced, this function will need to be called manually. Most plugins
// /// will not need to do this.
// pub fn receive_response_body(body: bool) {
//     crate::bulwark_host::receive_response_body(body)
// }

// /// Returns a named value from the request context's params.
// ///
// /// # Arguments
// ///
// /// * `key` - The key name corresponding to the param value.
// pub fn get_param_value(key: &str) -> Result<Value, crate::Error> {
//     let raw_value = crate::bulwark_host::get_param_value(key)?;
//     let value: serde_json::Value = serde_json::from_slice(&raw_value).unwrap();
//     Ok(value)
// }

// /// Set a named value in the request context's params.
// ///
// /// # Arguments
// ///
// /// * `key` - The key name corresponding to the param value.
// /// * `value` - The value to record. Values are serialized JSON.
// pub fn set_param_value(key: &str, value: Value) -> Result<(), crate::Error> {
//     let json = serde_json::to_vec(&value)?;
//     crate::bulwark_host::set_param_value(key, &json)?;
//     Ok(())
// }

/// Returns the guest environment's configuration value as a [`Value`].
///
/// By convention this will return a [`Value::Object`].
pub fn get_config() -> Result<Value, Error> {
    Ok(crate::wit::bulwark::plugin::config::config()?.into())
}

/// Returns a named guest environment configuration value as a [`Value`].
///
/// Equivalent to calling [`get_config`], reading it as an `Object`, and then retrieving a named [`Value`] from it.
///
/// # Arguments
///
/// * `key` - A key indexing into a configuration [`Map`]
pub fn get_config_value(key: &str) -> Result<Option<Value>, Error> {
    Ok(crate::wit::bulwark::plugin::config::config_var(key)?.map(|v| v.into()))
}

// /// Returns a named environment variable value as a [`String`].
// ///
// /// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
// /// the environment variable being requested. This function will panic if permission has not been granted.
// ///
// /// # Arguments
// ///
// /// * `key` - The environment variable name. Case-sensitive.
// pub fn get_env(key: &str) -> Result<String, crate::EnvVarError> {
//     Ok(String::from_utf8(
//         crate::wit::bulwark::plugin::config::env_var(key)?,
//     )?)
// }

// /// Returns a named environment variable value as bytes.
// ///
// /// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
// /// the environment variable being requested. This function will panic if permission has not been granted.
// ///
// /// # Arguments
// ///
// /// * `key` - The environment variable name. Case-sensitive.
// pub fn get_env_bytes(key: &str) -> Result<Vec<u8>, crate::EnvVarError> {
//     Ok(crate::wit::bulwark::plugin::config::env_var(key)?)
// }

// /// Records the decision value the plugin wants to return.
// ///
// /// # Arguments
// ///
// /// * `decision` - The [`Decision`] output of the plugin.
// pub fn set_decision(decision: Decision) -> Result<(), crate::Error> {
//     // Validate here because it should provide a better error than the one that the host will give.
//     decision.validate()?;
//     crate::bulwark_host::set_decision(crate::bindings::Decision {
//         accepted: decision.accept,
//         restricted: decision.restrict,
//         unknown: decision.unknown,
//     })
//     .expect("should not be able to produce an invalid result");
//     Ok(())
// }

// /// Records a decision indicating that the plugin wants to accept a request.
// ///
// /// This function is sugar for `set_decision(Decision { value, 0.0, 0.0 }.scale())`
// /// If used with a 1.0 value it should be given a weight in its config.
// ///
// /// # Arguments
// ///
// /// * `value` - The `accept` value to set.
// pub fn set_accepted(value: f64) {
//     crate::bulwark_host::set_decision(
//         Decision {
//             accept: value,
//             restrict: 0.0,
//             unknown: 0.0,
//         }
//         .scale()
//         .into(),
//     )
//     .expect("should not be able to produce an invalid result");
// }

// /// Records a decision indicating that the plugin wants to restrict a request.
// ///
// /// This function is sugar for `set_decision(Decision { 0.0, value, 0.0 }.scale())`.
// /// If used with a 1.0 value it should be given a weight in its config.
// ///
// /// # Arguments
// ///
// /// * `value` - The `restrict` value to set.
// pub fn set_restricted(value: f64) {
//     crate::bulwark_host::set_decision(
//         Decision {
//             accept: 0.0,
//             restrict: value,
//             unknown: 0.0,
//         }
//         .scale()
//         .into(),
//     )
//     .expect("should not be able to produce an invalid result");
// }

// /// Records the tags the plugin wants to associate with its decision.
// ///
// /// # Arguments
// ///
// /// * `tags` - The list of tags to associate with a [`Decision`]
// ///
// /// # Examples
// ///
// /// All of these are valid:
// ///
// /// ```no_run
// /// use bulwark_wasm_sdk::set_tags;
// ///
// /// set_tags(["tag"]);
// /// set_tags(vec!["tag"]);
// /// set_tags([String::from("tag")]);
// /// set_tags(vec![String::from("tag")]);
// /// // Clear tags, rarely needed
// /// set_tags::<[_; 0], String>([]);
// /// set_tags::<Vec<_>, String>(vec![]);
// /// ```
// #[inline]
// pub fn set_tags<I: IntoIterator<Item = V>, V: Into<String>>(tags: I) {
//     let tags: Vec<String> = tags.into_iter().map(|s| s.into()).collect();
//     crate::bulwark_host::set_tags(tags.as_slice())
// }

// /// Records additional tags the plugin wants to associate with its decision.
// ///
// /// # Arguments
// ///
// /// * `tags` - The list of additional tags to associate with a [`Decision`]
// ///
// /// # Examples
// ///
// /// All of these are valid:
// ///
// /// ```no_run
// /// use bulwark_wasm_sdk::append_tags;
// ///
// /// append_tags(["tag"]);
// /// append_tags(vec!["tag"]);
// /// append_tags([String::from("tag")]);
// /// append_tags(vec![String::from("tag")]);
// /// ```
// #[inline]
// pub fn append_tags<I: IntoIterator<Item = V>, V: Into<String>>(tags: I) -> Vec<String> {
//     let tags: Vec<String> = tags.into_iter().map(|s| s.into()).collect();
//     crate::bulwark_host::append_tags(tags.as_slice())
// }

// /// Returns the combined decision, if available.
// ///
// /// Typically used in the feedback phase.
// pub fn get_combined_decision() -> Option<Decision> {
//     crate::bulwark_host::get_combined_decision().map(|decision| decision.into())
// }

// /// Returns the combined set of tags associated with a decision, if available.
// ///
// /// Typically used in the feedback phase.
// #[inline]
// pub fn get_combined_tags() -> Option<Vec<String>> {
//     crate::bulwark_host::get_combined_tags()
// }

// /// Returns the outcome of the combined decision, if available.
// ///
// /// Typically used in the feedback phase.
// pub fn get_outcome() -> Option<Outcome> {
//     crate::bulwark_host::get_outcome().map(|outcome| outcome.into())
// }

// /// Sends an outbound HTTP request.
// ///
// /// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
// /// the host being requested. This function will panic if permission has not been granted.
// ///
// /// # Arguments
// ///
// /// * `request` - The HTTP request to send.
// pub fn send_request(request: Request) -> Result<Response, crate::HttpError> {
//     let request = crate::bulwark_host::RequestInterface::from(request);
//     Ok(Response::from(crate::bulwark_host::send_request(&request)?))
// }
