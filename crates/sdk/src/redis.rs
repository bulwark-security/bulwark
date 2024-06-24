//! The `redis` module manages remote state, including rate-limiting and circuit-breaking.

/// Returned when there is an issue with the remote state requested by the plugin.
#[derive(thiserror::Error, Debug)]
pub enum RedisError {
    #[error("access to state key '{key}' denied")]
    Permission { key: String },
    #[error("error accessing remote state: {message}")]
    Remote { message: String },
    #[error("invalid argument: {message}")]
    InvalidArgument { message: String },
    #[error("unexpected type received")]
    TypeError,
    #[error("{message}")]
    InvalidUnicode { message: String },
    #[error("could not parse integer value: {message}")]
    InvalidInteger { message: String },
    #[error("unexpected redis error: {message}")]
    Other { message: String },
}

impl From<crate::wit::bulwark::plugin::redis::Error> for RedisError {
    fn from(error: crate::wit::bulwark::plugin::redis::Error) -> Self {
        match error {
            crate::wit::bulwark::plugin::redis::Error::Permission(key) => {
                RedisError::Permission { key }
            }
            crate::wit::bulwark::plugin::redis::Error::Remote(message) => {
                RedisError::Remote { message }
            }
            crate::wit::bulwark::plugin::redis::Error::InvalidArgument(message) => {
                RedisError::InvalidArgument { message }
            }
            crate::wit::bulwark::plugin::redis::Error::TypeError => RedisError::TypeError,
            crate::wit::bulwark::plugin::redis::Error::Other(message) => {
                RedisError::Other { message }
            }
        }
    }
}

impl From<std::string::FromUtf8Error> for RedisError {
    fn from(error: std::string::FromUtf8Error) -> Self {
        RedisError::InvalidUnicode {
            message: error.to_string(),
        }
    }
}

impl From<crate::ParseCounterError> for RedisError {
    fn from(error: crate::ParseCounterError) -> Self {
        RedisError::InvalidInteger {
            message: error.to_string(),
        }
    }
}

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
pub type Breaker = crate::wit::bulwark::plugin::redis::Breaker;
/// A `Rate` contains the values needed to implement a rate-limiter pattern within a plugin.
///
/// # Fields
///
/// * `attempts` - The number of attempts made within the expiration window.
/// * `expiration` - The expiration timestamp in seconds since the epoch.
pub type Rate = crate::wit::bulwark::plugin::redis::Rate;

/// Returns the named state value retrieved from Redis as bytes.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
pub fn get<K: AsRef<str>>(key: K) -> Result<Option<Vec<u8>>, RedisError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::get(key)?)
}

/// Returns the named state value retrieved from Redis as a string.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
pub fn get_string<K: AsRef<str>>(key: K) -> Result<Option<String>, RedisError> {
    let key: &str = key.as_ref();
    let bytes = crate::wit::bulwark::plugin::redis::get(key)?;
    if let Some(bytes) = bytes {
        Ok(Some(String::from_utf8(bytes)?))
    } else {
        Ok(None)
    }
}

/// Returns the named state value retrieved from Redis as an i64.
///
/// This is generally used with counters.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
pub fn get_i64<K: AsRef<str>>(key: K) -> Result<Option<i64>, RedisError> {
    let key: &str = key.as_ref();
    let bytes = crate::wit::bulwark::plugin::redis::get(key)?;
    if let Some(bytes) = bytes {
        Ok(Some(parse_i64(bytes)?))
    } else {
        Ok(None)
    }
}

/// Set a named byte value in Redis.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
/// * `value` - The value to record. Values are byte strings, but may be interpreted differently by Redis depending on context.
pub fn set<K: AsRef<str>, V: AsRef<[u8]>>(key: K, value: V) -> Result<(), RedisError> {
    let key: &str = key.as_ref();
    let value: &[u8] = value.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::set(key, value)?)
}

/// Set a named string value in Redis.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
/// * `value` - The value to record. Values are byte strings, but may be interpreted differently by Redis depending on context.
pub fn set_string<K: AsRef<str>, V: AsRef<str>>(key: K, value: V) -> Result<(), RedisError> {
    let key: &str = key.as_ref();
    let value: &str = value.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::set(
        key,
        value.as_bytes(),
    )?)
}

/// Set a named integer value in Redis.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
/// * `value` - The value to record. Values are byte strings, but may be interpreted differently by Redis depending on context.
pub fn set_i64<K: AsRef<str>>(key: K, value: i64) -> Result<(), RedisError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::set(
        key,
        value.to_string().as_bytes(),
    )?)
}

/// Deletes named values in Redis.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `keys` - The list of key names corresponding to state values.
pub fn del<I: IntoIterator<Item = T>, T: Into<String>>(keys: I) -> Result<u32, RedisError> {
    let keys: Vec<String> = keys.into_iter().map(|s| s.into()).collect();
    Ok(crate::wit::bulwark::plugin::redis::del(keys.as_slice())?)
}

/// Increments a named counter in Redis.
///
/// Returns the value of the counter after it's incremented.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
pub fn incr<K: AsRef<str>>(key: K) -> Result<i64, RedisError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::incr(key)?)
}

/// Increments a named counter in Redis by a specified delta value.
///
/// Returns the value of the counter after it's incremented.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
/// * `delta` - The amount to increase the counter by.
pub fn incr_by<K: AsRef<str>>(key: K, delta: i64) -> Result<i64, RedisError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::incr_by(key, delta)?)
}

/// Adds a set of members to a set in Redis.
///
/// Returns the number of members that were added to the set.
/// Members already present in the set are not included in this count.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the set.
/// * `members` - The new members to add to the set.
pub fn sadd<K: AsRef<str>, I: IntoIterator<Item = T>, T: Into<String>>(
    key: K,
    members: I,
) -> Result<u32, RedisError> {
    let key: &str = key.as_ref();
    let members: Vec<String> = members.into_iter().map(|s| s.into()).collect();
    Ok(crate::wit::bulwark::plugin::redis::sadd(
        key,
        members.as_slice(),
    )?)
}

/// Retrieves a set of members stored in Redis.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the set.
pub fn smembers<K: AsRef<str>>(key: K) -> Result<Vec<String>, RedisError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::smembers(key)?)
}

/// Removes members from a set in Redis.
///
/// Returns the number of members that were removed from the set.
/// Members not present in the set are not included in this count.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the set.
/// * `members` - The new members to add to the set.
pub fn srem<K: AsRef<str>, I: IntoIterator<Item = T>, T: Into<String>>(
    key: K,
    members: I,
) -> Result<u32, RedisError> {
    let key: &str = key.as_ref();
    let members: Vec<String> = members.into_iter().map(|s| s.into()).collect();
    Ok(crate::wit::bulwark::plugin::redis::srem(
        key,
        members.as_slice(),
    )?)
}

/// Sets an expiration on a named value in Redis with a TTL.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
/// * `ttl` - The time-to-live for the value in seconds.
pub fn expire<K: AsRef<str>>(key: K, ttl: u64) -> Result<(), RedisError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::expire(key, ttl)?)
}

/// Sets an expiration on a named value in Redis to a specific time.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state value.
/// * `unix_time` - The unix timestamp in seconds.
pub fn expire_at<K: AsRef<str>>(key: K, unix_time: u64) -> Result<(), RedisError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::expire_at(
        key, unix_time,
    )?)
}

/// Increments a rate limit, returning the number of attempts so far and the expiration time.
///
/// The rate limiter is a counter over a period of time. At the end of the period, it will expire,
/// beginning a new period. Window periods should be set to the longest amount of time that a client should
/// be locked out for. The plugin is responsible for performing all rate-limiting logic with the counter
/// value it receives.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
/// * `delta` - The amount to increase the counter by.
/// * `window` - How long each period should be in seconds.
///
/// # Example
///
#[cfg_attr(doctest, doc = " ````no_test")] // highlight, but don't run the test (rust/issues/63193)
/// ```rust
/// use bulwark_sdk::*;
/// use std::collections::HashMap;
///
/// struct PostRateLimiter;
///
/// #[bulwark_plugin]
/// impl HttpHandlers for PostRateLimiter {
///     fn handle_request_decision(
///         req: Request,
///         _labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         let mut output = HandlerOutput::default();
///         if let Some(ip) = client_ip(&req) {
///             let key = format!("ip:post:{ip}");
///             if req.method() == http::Method::POST {
///                 let rate = redis::incr_rate_limit(key, 1, 5 * 60)?; // 5 minutes
///                 if rate.attempts >= 1000 {
///                     output.decision = RESTRICT;
///                     output.tags = vec!["rate-limited".to_string()];
///                 }
///             }
///         }
///         Ok(output)
///     }
/// }
/// ```
///
/// See [`check_rate_limit`] for an example covering both request and response handlers.
#[inline]
pub fn incr_rate_limit<K: AsRef<str>>(key: K, delta: i64, window: i64) -> Result<Rate, RedisError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::incr_rate_limit(
        key, delta, window,
    )?)
}

/// Checks a rate limit, returning the number of attempts so far and the expiration time.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
///
/// # Example
///
#[cfg_attr(doctest, doc = " ````no_test")] // highlight, but don't run the test (rust/issues/63193)
/// ```rust
/// use bulwark_sdk::*;
/// use std::collections::HashMap;
///
/// struct NotFoundRateLimiter;
///
/// #[bulwark_plugin]
/// impl HttpHandlers for NotFoundRateLimiter {
///     fn handle_request_decision(
///         req: Request,
///         _labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         let mut output = HandlerOutput::default();
///         if let Some(ip) = client_ip(&req) {
///             let key = format!("ip:not-found:{ip}");
///             // Only check the rate limit, don't increment, because we don't know the status code yet.
///             if let Some(rate) = redis::check_rate_limit(key)? {
///                 if rate.attempts >= 200 {
///                     output.decision = RESTRICT;
///                     output.tags = vec!["rate-limited".to_string()];
///                 }
///             }
///         }
///         Ok(output)
///     }
///
///     fn handle_response_decision(
///         req: Request,
///         resp: Response,
///         _labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         if let Some(ip) = client_ip(&req) {
///             let key = format!("ip:not-found:{ip}");
///             if resp.status().as_u16() == 404 {
///                 redis::incr_rate_limit(key, 1, 60 * 60)?; // 1 hour window
///             }
///         }
///         Ok(HandlerOutput::default())
///     }
/// }
/// ```
///
/// See [`incr_rate_limit`] for a simpler example covering only the request handler.
pub fn check_rate_limit<K: AsRef<str>>(key: K) -> Result<Option<Rate>, RedisError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::check_rate_limit(key)?)
}

/// Increments a circuit breaker, returning the generation count, success count, failure count,
/// consecutive success count, consecutive failure count, and expiration time.
///
/// The plugin is responsible for performing all circuit-breaking logic with the counter
/// values it receives. The host environment does as little as possible to maximize how much
/// control the plugin has over the behavior of the breaker.
///
/// In order for this function to succeed, a plugin's configuration must explicitly declare a permission grant for
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
/// * `delta` - The amount to increase the success or failure counter by.
/// * `success` - Whether the operation was successful or not, determining which counter to increment.
/// * `window` - How long each period should be in seconds.
///
/// # Example
///
#[cfg_attr(doctest, doc = " ````no_test")] // highlight, but don't run the test (rust/issues/63193)
/// ```rust
/// use bulwark_sdk::*;
/// use std::collections::HashMap;
///
/// struct OptionsScan;
///
/// #[bulwark_plugin]
/// impl HttpHandlers for OptionsScan {
///     fn handle_request_decision(
///         req: Request,
///         _labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         let mut output = HandlerOutput::default();
///         if let Some(ip) = client_ip(&req) {
///             let key = format!("ip:options:{ip}");
///             let success = req.method() != http::Method::OPTIONS;
///             let breaker = redis::incr_breaker(key, 1, success, 15 * 60)?; // 15 minutes
///             if breaker.consecutive_failures >= 25 {
///                 // After 25 or more consecutive options requests from the same client,
///                 // then mark the request suspicious but don't block it outright.
///                 output.decision = Decision::restricted(0.25);
///                 output.tags = vec!["options-scan".to_string()];
///             }
///         }
///         Ok(output)
///     }
/// }
/// ```
///
/// See [`check_breaker`] for an example covering both request and response handlers.
pub fn incr_breaker<K: AsRef<str>>(
    key: K,
    delta: i64,
    success: bool,
    window: i64,
) -> Result<Breaker, RedisError> {
    let key: &str = key.as_ref();
    let (success_delta, failure_delta) = match success {
        true => (delta, 0),
        false => (0, delta),
    };
    Ok(crate::wit::bulwark::plugin::redis::incr_breaker(
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
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
///
/// # Example
///
#[cfg_attr(doctest, doc = " ````no_test")] // highlight, but don't run the test (rust/issues/63193)
/// ```rust
/// use bulwark_sdk::*;
/// use std::collections::HashMap;
///
/// struct SoftCircuitBreaker;
///
/// #[bulwark_plugin]
/// impl HttpHandlers for SoftCircuitBreaker {
///     fn handle_request_decision(
///         req: Request,
///         _labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         let mut output = HandlerOutput::default();
///         if let Some(ip) = client_ip(&req) {
///             let key = format!("ip:breaker:{ip}");
///             // Only check the breaker, don't increment, because we don't know the status code yet.
///             if let Some(breaker) = redis::check_breaker(key)? {
///                 if breaker.consecutive_failures >= 10 && breaker.consecutive_failures % 10 != 0 {
///                     // Breaker tripped, but allow every 10th request through.
///                     // Only soft-limit requests by setting a low score.
///                     output.decision = Decision::restricted(0.25);
///                     output.tags = vec!["circuit-broken".to_string()];
///                 }
///             }
///         }
///         Ok(output)
///     }
///
///     fn handle_response_decision(
///         req: Request,
///         resp: Response,
///         _labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         if let Some(ip) = client_ip(&req) {
///             let key = format!("ip:breaker:{ip}");
///             let success = resp.status().as_u16() < 500;
///             redis::incr_breaker(key, 1, success, 10 * 60)?; // 10 minutes
///         }
///         Ok(HandlerOutput::default())
///     }
/// }
/// ```
///
/// See [`incr_breaker`] for a simpler example covering only the request handler.
#[inline]
pub fn check_breaker<K: AsRef<str>>(key: K) -> Result<Option<Breaker>, RedisError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::check_breaker(key)?)
}

/// Parses a counter value from state stored as a string.
///
/// # Arguments
///
/// * `value` - The string representation of a counter.
#[inline]
fn parse_i64(value: Vec<u8>) -> Result<i64, crate::ParseCounterError> {
    Ok(std::str::from_utf8(value.as_slice())?.parse::<i64>()?)
}
