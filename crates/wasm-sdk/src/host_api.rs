use {
    forwarded_header_value::ForwardedHeaderValue,
    std::{collections::HashMap, net::IpAddr, str},
};

// For some reason, doc-tests in this module trigger a linker error, so they're set to no_run

pub use crate::{Decision, Outcome};
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
/// An HTTP header value type alias. See [`http::HeaderValue`] for details.
pub type HeaderValue = http::HeaderValue;

// TODO: perhaps something more like http::Request<Box<dyn AsyncRead + Sync + Send + Unpin>>?
// TODO: or hyper::Request<HyperIncomingBody> to match WasiHttpView's new_incoming_request?

/// A `HandlerOutput` represents a decision and associated output for a single handler within a single detection.
#[derive(Clone, Default)]
pub struct HandlerOutput {
    /// The `labels` field contains key/value pairs used to enrich the request with additional information.
    pub labels: HashMap<String, String>,
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

/// Returns all of the plugin's configuration key names.
pub fn config_keys() -> Vec<String> {
    crate::wit::bulwark::plugin::config::config_keys()
}

/// Returns a named plugin configuration value as a [`Value`].
///
/// # Arguments
///
/// * `key` - A key indexing into a configuration [`Map`]
pub fn config_var(key: &str) -> Option<Value> {
    crate::wit::bulwark::plugin::config::config_var(key).map(|v| v.into())
}

/// Returns the true remote client IP address.
///
/// This is derived from the `proxy_hops` configuration value and the
/// `Forwarded` or `X-Forwarded-For` headers.
pub fn client_ip(req: &Request) -> Option<IpAddr> {
    let proxy_hops = crate::wit::bulwark::plugin::config::proxy_hops();

    if let Some(forwarded) = req.headers().get("forwarded") {
        return parse_forwarded_ip(forwarded.as_bytes(), proxy_hops as usize);
    }
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        return parse_x_forwarded_for_ip(forwarded.as_bytes(), proxy_hops as usize);
    }
    None
}

/// Checks the request for a valid `Forwarded` header and returns the client IP address
/// indicated by the number of exterior proxy hops.
fn parse_forwarded_ip(forwarded: &[u8], proxy_hops: usize) -> Option<IpAddr> {
    let forwarded = str::from_utf8(forwarded).ok()?;
    let value = ForwardedHeaderValue::from_forwarded(forwarded).ok();
    value.and_then(|fhv| {
        if proxy_hops > fhv.len() {
            None
        } else {
            let item = fhv.iter().nth(fhv.len() - proxy_hops);
            item.and_then(|fs| fs.forwarded_for_ip())
        }
    })
}

/// Checks the request for a valid `X-Forwarded-For` header and returns the client IP address
/// indicated by the number of exterior proxy hops.
fn parse_x_forwarded_for_ip(forwarded: &[u8], proxy_hops: usize) -> Option<IpAddr> {
    let forwarded = str::from_utf8(forwarded).ok()?;
    let value = ForwardedHeaderValue::from_x_forwarded_for(forwarded).ok();
    value.and_then(|fhv| {
        if proxy_hops > fhv.len() {
            None
        } else {
            let item = fhv.iter().nth(fhv.len() - proxy_hops);
            item.and_then(|fs| fs.forwarded_for_ip())
        }
    })
}
