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
pub fn get<K: AsRef<str>>(key: K) -> Result<Option<Vec<u8>>, crate::RemoteStateError> {
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
pub fn get_string<K: AsRef<str>>(key: K) -> Result<Option<String>, crate::RemoteStateError> {
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
pub fn get_i64<K: AsRef<str>>(key: K) -> Result<Option<i64>, crate::RemoteStateError> {
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
pub fn set<K: AsRef<str>, V: AsRef<[u8]>>(key: K, value: V) -> Result<(), crate::RemoteStateError> {
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
pub fn set_string<K: AsRef<str>, V: AsRef<str>>(
    key: K,
    value: V,
) -> Result<(), crate::RemoteStateError> {
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
pub fn set_i64<K: AsRef<str>>(key: K, value: i64) -> Result<(), crate::RemoteStateError> {
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
pub fn del<I: IntoIterator<Item = T>, T: Into<String>>(
    keys: I,
) -> Result<u32, crate::RemoteStateError> {
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
pub fn incr<K: AsRef<str>>(key: K) -> Result<i64, crate::RemoteStateError> {
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
pub fn incr_by<K: AsRef<str>>(key: K, delta: i64) -> Result<i64, crate::RemoteStateError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::incr_by(key, delta)?)
}

pub fn sadd<K: AsRef<str>, I: IntoIterator<Item = T>, T: Into<String>>(
    key: K,
    members: I,
) -> Result<u32, crate::RemoteStateError> {
    let key: &str = key.as_ref();
    let members: Vec<String> = members.into_iter().map(|s| s.into()).collect();
    Ok(crate::wit::bulwark::plugin::redis::sadd(
        key,
        members.as_slice(),
    )?)
}

pub fn smembers<K: AsRef<str>>(key: K) -> Result<Vec<String>, crate::RemoteStateError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::smembers(key)?)
}

pub fn srem<K: AsRef<str>, I: IntoIterator<Item = T>, T: Into<String>>(
    key: K,
    members: I,
) -> Result<u32, crate::RemoteStateError> {
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
pub fn expire<K: AsRef<str>>(key: K, ttl: u64) -> Result<(), crate::RemoteStateError> {
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
pub fn expire_at<K: AsRef<str>>(key: K, unix_time: u64) -> Result<(), crate::RemoteStateError> {
    let key: &str = key.as_ref();
    Ok(crate::wit::bulwark::plugin::redis::expire_at(
        key, unix_time,
    )?)
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
/// the prefix of the key being requested. This function will return an error if permission has not been granted.
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
/// * `delta` - The amount to increase the counter by.
/// * `window` - How long each period should be in seconds.
#[inline]
pub fn incr_rate_limit<K: AsRef<str>>(
    key: K,
    delta: i64,
    window: i64,
) -> Result<Rate, crate::RemoteStateError> {
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
/// See [`increment_rate_limit`].
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
pub fn check_rate_limit<K: AsRef<str>>(key: K) -> Result<Option<Rate>, crate::RemoteStateError> {
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
/// # Examples
///
/// ```no_compile
/// use bulwark_wasm_sdk::*;
///
/// struct CircuitBreaker;
///
/// #[bulwark_plugin]
/// impl HttpHandlers for CircuitBreaker {
///     fn handle_response_decision(
///         req: Request,
///         resp: Response,
///         _labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         let mut output = HandlerOutput::default();
///         if let Some(ip) = client_ip(req) {
///             let key = format!("client.ip:{ip}");
///             // "success" or "failure" could be determined by other methods besides status code
///             let success = resp.status().as_u16() < 500;
///             let breaker = incr_breaker(
///                 &key,
///                 1,
///                 success,
///                 60 * 60, // 1 hour
///             )?;
///             // use breaker here
///         }
///         Ok(output)
///     }
/// }
/// ```
pub fn incr_breaker<K: AsRef<str>>(
    key: K,
    delta: i64,
    success: bool,
    window: i64,
) -> Result<Breaker, crate::RemoteStateError> {
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
/// See [`incr_breaker`].
///
/// # Arguments
///
/// * `key` - The key name corresponding to the state counter.
#[inline]
pub fn check_breaker<K: AsRef<str>>(key: K) -> Result<Option<Breaker>, crate::RemoteStateError> {
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
