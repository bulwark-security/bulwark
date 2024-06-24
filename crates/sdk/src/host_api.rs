use forwarded_header_value::ForwardedHeaderValue;
use serde::de::DeserializeOwned;
use std::{collections::HashMap, net::IpAddr, str};

// For some reason, doc-tests in this module trigger a linker error, so they're set to no_run

pub use crate::{Decision, Outcome};

/// Construct a `Value` from a literal.
///
/// Useful for comparisons with functions that return a [`Value`].
///
/// # Example
///
/// ```no_run
/// use bulwark_sdk::*;
///
/// if let Some(strict) = config_var("strict") {
///     if strict == value!(true) {
///         // strict enforcement here
///     }
/// }
/// ```
///
/// See also [`from_value`].
#[macro_export(local_inner_macros)]
macro_rules! value {
    // Allow us to customize documentation without the normally auto-applied concatenation.
    ($($value:tt)+) => {
        value_internal!($($value)+)
    };
}
#[doc(hidden)]
pub use serde_json::json as value_internal;

/// Interpret a [`Value`] as an instance of type T.
///
/// # Example
///
#[cfg_attr(doctest, doc = " ````no_test")] // highlight, but don't run the test (rust/issues/63193)
/// ```rust
/// use bulwark_sdk::*;
/// use std::collections::HashMap;
/// use serde::Deserialize;
///
/// // Expects a plugin config like:
/// // `config = { host = { strict = true, suffix = "example.com" } }`
/// #[derive(Deserialize, Debug)]
/// struct HostConfig {
///     strict: bool,
///     suffix: String,
/// }
///
/// pub struct ExamplePlugin;
///
/// #[bulwark_plugin]
/// impl HttpHandlers for ExamplePlugin {
///     fn handle_request_decision(
///         _request: http::Request,
///         _labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         let config: HostConfig =
///             from_value(config_var("host").ok_or(error!("host config missing"))?)?;
///         // Use config here.
///         Ok(HandlerOutput::default())
///     }
/// }
/// ```
///
/// See also [`value!`].
#[inline]
pub fn from_value<T>(value: Value) -> Result<T, crate::Error>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value).map_err(crate::Error::from)
}

pub use serde_json::{Map, Value};

/// A type alias. See [`bytes::Bytes`](https://docs.rs/bytes/latest/bytes/struct.Bytes.html) for details.
pub type Bytes = bytes::Bytes;

/// A `HandlerOutput` represents a decision and associated output for a handler function within a detection.
///
/// This struct contains a [`Decision`] value that determines whether the request should be accepted or
/// restricted. A [full explanation of decisions](https://bulwark.security/docs/explanation/decisions/) can be
/// found in the main Bulwark documentation.
#[derive(Clone, Default)]
pub struct HandlerOutput {
    /// The `decision` value represents the numerical decision from a detection.
    ///
    /// It will typically be combined with similar decision values from other detections into a
    /// new combined decision value.
    pub decision: Decision,
    /// The `labels` field contains key/value pairs used to enrich the request with additional information.
    ///
    /// Labels are key-value pairs that are also associated with the request, but other plugins are the primary
    /// audience for labels. Bulwark uses a [label schema](https://bulwark.security/docs/reference/label-schema/)
    /// to reduce the need for plugin authors to do out-of-band coordination of terminology in order to interoperate.
    pub labels: HashMap<String, String>,
    /// The `tags` value represents the new tags to annotate the request with.
    ///
    /// Tags are arbitrary string values and are typically used to categorize requests and provide contextual
    /// information about why a plugin made the decision that it did. Humans are the primary audience for tags.
    pub tags: Vec<String>,
}

/// A `Verdict` represents a combined decision across multiple detections.
#[derive(Clone)]
pub struct Verdict {
    /// The `decision` value represents the combined numerical decision from multiple detections.
    pub decision: Decision,
    /// The `outcome` value represents a comparison of the numerical decision against a set of thresholds.
    pub outcome: Outcome,
    /// The `count` value represents the number of plugins that contributed to the decision.
    pub count: u32,
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
///
/// # Example
///
#[cfg_attr(doctest, doc = " ````no_test")] // highlight, but don't run the test (rust/issues/63193)
/// ```rust
/// use bulwark_sdk::*;
/// use ipnet::Ipv4Net;
/// use iprange::IpRange;
/// use std::net::IpAddr;
/// use std::collections::HashMap
///
/// struct BannedRanges;
///
/// #[bulwark_plugin]
/// impl HttpHandlers for BannedRanges {
///     fn handle_request_decision(
///         req: http::Request,
///         _labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         let mut output = HandlerOutput::default();
///         if let Some(IpAddr::V4(ip)) = client_ip(&req) {
///             let ip_range: IpRange<Ipv4Net> = from_value::<Vec<String>>(
///                 config_var("banned_ranges").ok_or(error!("banned_ranges not set"))?,
///             )?
///             .iter()
///             .map(|s| s.parse().unwrap())
///             .collect();
///
///             if ip_range.contains(&ip) {
///                 output.decision = RESTRICT;
///             }
///         }
///         Ok(output)
///     }
/// }
/// ```
pub fn config_var(key: &str) -> Option<Value> {
    crate::wit::bulwark::plugin::config::config_var(key).map(|v| v.into())
}

/// Returns the true remote client IP address.
///
/// This is derived from the `proxy_hops` configuration value and the
/// `Forwarded` or `X-Forwarded-For` headers.
///
/// # Example
///
#[cfg_attr(doctest, doc = " ````no_test")] // highlight, but don't run the test (rust/issues/63193)
/// ```rust
/// use bulwark_sdk::*;
/// use std::collections::HashMap;
///
/// struct RateLimiter;
///
/// #[bulwark_plugin]
/// impl HttpHandlers for RateLimiter {
///     fn handle_request_decision(
///         req: http::Request,
///         _labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         let mut output = HandlerOutput::default();
///         if let Some(ip) = client_ip(&req) {
///             let key = format!("ip:{}", ip);
///             let rate = redis::incr_rate_limit(key, 1, 60 * 15)?; // 15 minute window
///
///             if rate.attempts > 1000 {
///                 output.decision = RESTRICT;
///                 output.tags = vec!["rate-limited".to_string()];
///             }
///         }
///         Ok(output)
///     }
/// }
/// ```
pub fn client_ip(req: &crate::http::Request) -> Option<IpAddr> {
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
