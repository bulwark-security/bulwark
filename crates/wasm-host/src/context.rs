use {
    crate::{ContextInstantiationError, Plugin, PluginStdio},
    ::redis::Commands,
    chrono::Utc,
    core::{future::Future, marker::Send, pin::Pin},
    std::{collections::HashMap, ops::DerefMut, sync::Arc},
    url::Url,
    wasmtime::component::Resource,
    wasmtime_wasi::preview2::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiView},
    wasmtime_wasi_http::{
        types::{HostFutureIncomingResponse, HostIncomingResponse, OutgoingRequest},
        WasiHttpCtx, WasiHttpView,
    },
};

/// The PluginContext manages access to information that needs to cross the plugin sandbox boundary.
pub struct PluginContext {
    /// The WASI context that determines how things like stdio map to our buffers.
    wasi_ctx: WasiCtx,
    /// The WASI HTTP context that allows us to manage HTTP resources.
    wasi_http: WasiHttpCtx,
    /// The WASI table that maps handles to resources.
    wasi_table: ResourceTable,
    /// The standard I/O buffers used by WASI and captured for logging.
    pub(crate) stdio: PluginStdio,
    /// All host configuration.
    host_config: Arc<bulwark_config::Config>,
    /// Plugin-specific configuration. Stored as bytes and deserialized as JSON values by the SDK.
    ///
    /// There may be multiple instances of the same plugin with different values for this configuration
    /// causing the plugin behavior to be different. For instance, a plugin might define a pattern-matching
    /// algorithm in its code while reading the specific patterns to match from this configuration.
    guest_config: Arc<serde_json::Map<String, serde_json::Value>>,
    /// The set of permissions granted to a plugin.
    permissions: bulwark_config::Permissions,
    /// The Redis connection pool and its associated Lua scripts.
    redis_info: Option<Arc<RedisInfo>>,
}

/// Wraps a Redis connection pool and a registry of predefined Lua scripts.
pub struct RedisInfo {
    /// The connection pool
    pub pool: r2d2::Pool<redis::Client>,
    /// A Lua script registry
    pub registry: ScriptRegistry,
}

/// A registry of predefined Lua scripts for execution within Redis.
pub struct ScriptRegistry {
    /// Increments a Redis key's counter value if it has not yet expired.
    ///
    /// Uses the service's clock rather than Redis'. Uses Redis' TTL on a best-effort basis.
    increment_rate_limit: redis::Script,
    /// Checks a Redis key's counter value if it has not yet expired.
    ///
    /// Uses the service's clock rather than Redis'. Uses Redis' TTL on a best-effort basis.
    check_rate_limit: redis::Script,
    /// Increments a Redis key's counter value, corresponding to either success or failure, if it has not yet expired.
    ///
    /// Uses the service's clock rather than Redis'. Uses Redis' TTL on a best-effort basis.
    increment_breaker: redis::Script,
    /// Checks a Redis key's counter value, corresponding to either success or failure, if it has not yet expired.
    ///
    /// Uses the service's clock rather than Redis'. Uses Redis' TTL on a best-effort basis.
    check_breaker: redis::Script,
}

impl Default for ScriptRegistry {
    fn default() -> ScriptRegistry {
        ScriptRegistry {
            // TODO: handle overflow errors by expiring everything on overflow and returning nil?
            increment_rate_limit: redis::Script::new(
                r#"
                local counter_key = "bulwark:rl:" .. KEYS[1]
                local increment_delta = tonumber(ARGV[1])
                local expiration_window = tonumber(ARGV[2])
                local timestamp = tonumber(ARGV[3])
                local expiration_key = counter_key .. ":exp"
                local expiration = tonumber(redis.call("get", expiration_key))
                local next_expiration = timestamp + expiration_window
                if not expiration or timestamp > expiration then
                    redis.call("set", expiration_key, next_expiration)
                    redis.call("set", counter_key, 0)
                    redis.call("expireat", expiration_key, next_expiration + 1)
                    redis.call("expireat", counter_key, next_expiration + 1)
                    expiration = next_expiration
                end
                local attempts = redis.call("incrby", counter_key, increment_delta)
                return { attempts, expiration }
                "#,
            ),
            check_rate_limit: redis::Script::new(
                r#"
                local counter_key = "bulwark:rl:" .. KEYS[1]
                local expiration_key = counter_key .. ":exp"
                local timestamp = tonumber(ARGV[1])
                local attempts = tonumber(redis.call("get", counter_key))
                local expiration = nil
                if attempts then
                    expiration = tonumber(redis.call("get", expiration_key))
                    if not expiration or timestamp > expiration then
                        attempts = nil
                        expiration = nil
                    end
                end
                return { attempts, expiration }
                "#,
            ),
            increment_breaker: redis::Script::new(
                r#"
                local generation_key = "bulwark:bk:g:" .. KEYS[1]
                local success_key = "bulwark:bk:s:" .. KEYS[1]
                local failure_key = "bulwark:bk:f:" .. KEYS[1]
                local consec_success_key = "bulwark:bk:cs:" .. KEYS[1]
                local consec_failure_key = "bulwark:bk:cf:" .. KEYS[1]
                local success_delta = tonumber(ARGV[1])
                local failure_delta = tonumber(ARGV[2])
                local expiration_window = tonumber(ARGV[3])
                local timestamp = tonumber(ARGV[4])
                local expiration = timestamp + expiration_window
                local generation = redis.call("incrby", generation_key, 1)
                local successes = 0
                local failures = 0
                local consec_successes = 0
                local consec_failures = 0
                if success_delta > 0 then
                    successes = redis.call("incrby", success_key, success_delta)
                    failures = tonumber(redis.call("get", failure_key)) or 0
                    consec_successes = redis.call("incrby", consec_success_key, success_delta)
                    redis.call("set", consec_failure_key, 0)
                    consec_failures = 0
                else
                    successes = tonumber(redis.call("get", success_key))
                    failures = redis.call("incrby", failure_key, failure_delta) or 0
                    redis.call("set", consec_success_key, 0)
                    consec_successes = 0
                    consec_failures = redis.call("incrby", consec_failure_key, failure_delta)
                end
                redis.call("expireat", generation_key, expiration + 1)
                redis.call("expireat", success_key, expiration + 1)
                redis.call("expireat", failure_key, expiration + 1)
                redis.call("expireat", consec_success_key, expiration + 1)
                redis.call("expireat", consec_failure_key, expiration + 1)
                return { generation, successes, failures, consec_successes, consec_failures, expiration }
                "#,
            ),
            check_breaker: redis::Script::new(
                r#"
                local generation_key = "bulwark:bk:g:" .. KEYS[1]
                local success_key = "bulwark:bk:s:" .. KEYS[1]
                local failure_key = "bulwark:bk:f:" .. KEYS[1]
                local consec_success_key = "bulwark:bk:cs:" .. KEYS[1]
                local consec_failure_key = "bulwark:bk:cf:" .. KEYS[1]
                local generation = tonumber(redis.call("get", generation_key))
                if not generation then
                    return { nil, nil, nil, nil, nil, nil }
                end
                local successes = tonumber(redis.call("get", success_key)) or 0
                local failures = tonumber(redis.call("get", failure_key)) or 0
                local consec_successes = tonumber(redis.call("get", consec_success_key)) or 0
                local consec_failures = tonumber(redis.call("get", consec_failure_key)) or 0
                local expiration = tonumber(redis.call("expiretime", success_key)) - 1
                return { generation, successes, failures, consec_successes, consec_failures, expiration }
                "#,
            ),
        }
    }
}

impl PluginContext {
    /// Creates a new `PluginContext`.
    ///
    /// # Arguments
    ///
    /// * `plugin` - The [`Plugin`] and its associated configuration.
    /// * `redis_info` - The Redis connection pool.
    /// * `http_client` - The HTTP client used for outbound requests.
    pub fn new(
        plugin: Arc<Plugin>,
        environment: HashMap<String, String>,
        redis_info: Option<Arc<RedisInfo>>,
    ) -> Result<PluginContext, ContextInstantiationError> {
        let stdio = PluginStdio::default();
        let wasi_ctx = WasiCtxBuilder::new()
            .stdout(stdio.stdout.clone())
            .stderr(stdio.stderr.clone())
            .envs(
                environment
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect::<Vec<(&str, &str)>>()
                    .as_slice(),
            )
            .build();

        Ok(PluginContext {
            wasi_ctx,
            wasi_http: WasiHttpCtx,
            wasi_table: ResourceTable::new(),
            stdio,
            host_config: Arc::new(plugin.host_config().clone()),
            guest_config: Arc::new(plugin.guest_config().clone()),
            permissions: plugin.permissions().clone(),
            redis_info,
        })
    }

    pub fn new_incoming_response(
        &mut self,
        response: HostIncomingResponse,
    ) -> wasmtime::Result<Resource<HostIncomingResponse>> {
        let id = self.wasi_table.push(response)?;
        Ok(id)
    }
}

impl WasiView for PluginContext {
    fn table(&self) -> &ResourceTable {
        &self.wasi_table
    }

    fn table_mut(&mut self) -> &mut ResourceTable {
        &mut self.wasi_table
    }

    fn ctx(&self) -> &WasiCtx {
        &self.wasi_ctx
    }

    fn ctx_mut(&mut self) -> &mut WasiCtx {
        &mut self.wasi_ctx
    }
}

impl WasiHttpView for PluginContext {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.wasi_http
    }

    fn table(&mut self) -> &mut ResourceTable {
        &mut self.wasi_table
    }

    fn send_request(
        &mut self,
        request: OutgoingRequest,
    ) -> wasmtime::Result<Resource<HostFutureIncomingResponse>>
    where
        Self: Sized,
    {
        verify_http_domains(&self.permissions.http, &request.authority)?;
        wasmtime_wasi_http::types::default_send_request(self, request)
    }
}

impl crate::bindings::bulwark::plugin::types::Host for PluginContext {}

impl crate::bindings::bulwark::plugin::config::Host for PluginContext {
    /// Returns the named config value.
    fn config_keys<'ctx, 'async_trait>(
        &'ctx mut self,
    ) -> Pin<Box<dyn Future<Output = wasmtime::Result<Vec<String>>> + Send + 'async_trait>>
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move { Ok(self.guest_config.keys().cloned().collect()) })
    }

    /// Returns the named config value.
    fn config_var<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<
                            Option<crate::bindings::bulwark::plugin::config::Value>,
                            crate::bindings::bulwark::plugin::config::Error,
                        >,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(self
                .guest_config
                .get(key.as_str())
                .map_or(Ok(None), |value| {
                    // Invert, we need Result<Option<V>, E> rather than Option<Result<V, E>>.
                    // This is also why the map_or default above is Ok(None).
                    value
                        .clone()
                        .try_into()
                        .map_err(|e: &'static str| {
                            crate::bindings::bulwark::plugin::config::Error::InvalidConversion(
                                e.to_string(),
                            )
                        })
                        .map(Some)
                }))
        })
    }

    /// Returns the number of proxy hops expected exterior to Bulwark.
    fn proxy_hops<'ctx, 'async_trait>(
        &'ctx mut self,
    ) -> Pin<Box<dyn Future<Output = wasmtime::Result<u8>> + Send + 'async_trait>>
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move { Ok(self.host_config.service.proxy_hops) })
    }
}

impl crate::bindings::bulwark::plugin::redis::Host for PluginContext {
    /// Retrieves the value associated with the given key.
    fn get<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<Option<Vec<u8>>, crate::bindings::bulwark::plugin::redis::Error>,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<Option<Vec<u8>>, crate::bindings::bulwark::plugin::redis::Error> {
                    verify_remote_state_prefixes(&self.permissions.state, &key)?;

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info.pool.get().map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?;

                        Ok(conn.get(key).map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?)
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }

    /// Sets the given key to the given value.
    ///
    /// Overwrites any previously existing value.
    fn set<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
        value: Vec<u8>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<(), crate::bindings::bulwark::plugin::redis::Error>,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<(), crate::bindings::bulwark::plugin::redis::Error> {
                    verify_remote_state_prefixes(&self.permissions.state, &key)?;

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info.pool.get().map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?;

                        conn.set::<String, Vec<u8>, redis::Value>(key, value)
                            .map_err(|err| {
                                crate::bindings::bulwark::plugin::redis::Error::Remote(
                                    err.to_string(),
                                )
                            })?;
                        Ok(())
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }

    /// Removes the given keys.
    ///
    /// Non-existant keys are ignored. Returns the number of keys that were removed.
    fn del<'ctx, 'async_trait>(
        &'ctx mut self,
        keys: Vec<String>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<u32, crate::bindings::bulwark::plugin::redis::Error>,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<u32, crate::bindings::bulwark::plugin::redis::Error> {
                    for key in keys.iter() {
                        verify_remote_state_prefixes(&self.permissions.state, key)?;
                    }

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info.pool.get().map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?;

                        Ok(conn.del(keys).map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?)
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }

    /// Increments the value associated with the given key by one.
    ///
    /// If the key does not exist, it is set to zero before being incremented.
    /// If the key already has a value that cannot be incremented, a `error::type-error` is returned.
    fn incr<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<i64, crate::bindings::bulwark::plugin::redis::Error>,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        self.incr_by(key, 1)
    }

    /// Increments the value associated with the given key by the given delta.
    ///
    /// If the key does not exist, it is set to zero before being incremented.
    /// If the key already has a value that cannot be incremented, a `error::type-error` is returned.
    fn incr_by<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
        delta: i64,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<i64, crate::bindings::bulwark::plugin::redis::Error>,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<i64, crate::bindings::bulwark::plugin::redis::Error> {
                    verify_remote_state_prefixes(&self.permissions.state, &key)?;

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info.pool.get().map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?;

                        Ok(conn.incr(key, delta).map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?)
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }

    /// Adds the given values to the named set.
    ///
    /// Returns the number of elements that were added to the set,
    /// not including all the elements already present in the set.
    fn sadd<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
        values: Vec<String>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<u32, crate::bindings::bulwark::plugin::redis::Error>,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<u32, crate::bindings::bulwark::plugin::redis::Error> {
                    verify_remote_state_prefixes(&self.permissions.state, &key)?;

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info.pool.get().map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?;

                        Ok(conn.sadd(key, values).map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?)
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }

    /// Returns the contents of the given set.
    fn smembers<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<Vec<String>, crate::bindings::bulwark::plugin::redis::Error>,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<Vec<String>, crate::bindings::bulwark::plugin::redis::Error> {
                    verify_remote_state_prefixes(&self.permissions.state, &key)?;

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info.pool.get().map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?;

                        Ok(conn.smembers(key).map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?)
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }

    /// Removes the given values from the named set.
    ///
    /// Returns the number of members that were removed from the set,
    /// not including non existing members.
    fn srem<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
        values: Vec<String>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<u32, crate::bindings::bulwark::plugin::redis::Error>,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<u32, crate::bindings::bulwark::plugin::redis::Error> {
                    verify_remote_state_prefixes(&self.permissions.state, &key)?;

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info.pool.get().map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?;

                        Ok(conn.srem(key, values).map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?)
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }

    /// Sets the time to live for the given key.
    fn expire<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
        ttl: u64,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<(), crate::bindings::bulwark::plugin::redis::Error>,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<(), crate::bindings::bulwark::plugin::redis::Error> {
                    verify_remote_state_prefixes(&self.permissions.state, &key)?;

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info.pool.get().map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?;

                        Ok(conn.expire(key, ttl as usize).map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?)
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }

    /// Sets the expiration for the given key to the given unix time.
    fn expire_at<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
        unix_time: u64,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<(), crate::bindings::bulwark::plugin::redis::Error>,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<(), crate::bindings::bulwark::plugin::redis::Error> {
                    verify_remote_state_prefixes(&self.permissions.state, &key)?;

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info.pool.get().map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?;

                        Ok(conn.expire_at(key, unix_time as usize).map_err(|err| {
                            crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string())
                        })?)
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }

    fn incr_rate_limit<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
        delta: i64,
        window: i64,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<
                            crate::bindings::bulwark::plugin::redis::Rate,
                            crate::bindings::bulwark::plugin::redis::Error,
                        >,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<crate::bindings::bulwark::plugin::redis::Rate, crate::bindings::bulwark::plugin::redis::Error> {
                    verify_remote_state_prefixes(&self.permissions.state, &key)?;

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info
                            .pool
                            .get()
                            .map_err(|err| crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string()))?;
                        let dt = Utc::now();
                        let timestamp: i64 = dt.timestamp();
                        let script = redis_info.registry.increment_rate_limit.clone();
                        // Invoke the script and map to our rate type
                        let (attempts, expiration) = script
                            .key(key)
                            .arg(delta)
                            .arg(window)
                            .arg(timestamp)
                            .invoke::<(i64, i64)>(conn.deref_mut())
                            .map_err(|err| crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string()))?;
                        Ok(crate::bindings::bulwark::plugin::redis::Rate {
                            attempts,
                            expiration,
                        })
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }

    fn check_rate_limit<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<
                            crate::bindings::bulwark::plugin::redis::Rate,
                            crate::bindings::bulwark::plugin::redis::Error,
                        >,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
            // Inner function to permit ? operator
            || -> Result<crate::bindings::bulwark::plugin::redis::Rate, crate::bindings::bulwark::plugin::redis::Error> {
                verify_remote_state_prefixes(&self.permissions.state, &key)?;

                if let Some(redis_info) = self.redis_info.clone() {
                    let mut conn = redis_info
                        .pool
                        .get()
                        .map_err(|err| crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string()))?;
                    let dt = Utc::now();
                    let timestamp: i64 = dt.timestamp();
                    let script = redis_info.registry.check_rate_limit.clone();
                    // Invoke the script and map to our rate type
                    let (attempts, expiration) = script
                        .key(key)
                        .arg(timestamp)
                        .invoke::<(i64, i64)>(conn.deref_mut())
                        .map_err(|err| crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string()))?;
                    Ok(crate::bindings::bulwark::plugin::redis::Rate {
                        attempts,
                        expiration,
                    })
                } else {
                    Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                        "no remote state configured".to_string(),
                    ))
                }
            }(),
        )
        })
    }

    fn incr_breaker<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
        success_delta: i64,
        failure_delta: i64,
        window: i64,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<
                            crate::bindings::bulwark::plugin::redis::Breaker,
                            crate::bindings::bulwark::plugin::redis::Error,
                        >,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<crate::bindings::bulwark::plugin::redis::Breaker, crate::bindings::bulwark::plugin::redis::Error> {
                    verify_remote_state_prefixes(&self.permissions.state, &key)?;

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info
                            .pool
                            .get()
                            .map_err(|err| crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string()))?;
                        let dt = Utc::now();
                        let timestamp: i64 = dt.timestamp();
                        let script = redis_info.registry.increment_breaker.clone();
                        // Invoke the script and map to our breaker type
                        let (
                            generation,
                            successes,
                            failures,
                            consecutive_successes,
                            consecutive_failures,
                            expiration,
                        ) = script
                            .key(key)
                            .arg(success_delta)
                            .arg(failure_delta)
                            .arg(window)
                            .arg(timestamp)
                            .invoke::<(i64, i64, i64, i64, i64, i64)>(conn.deref_mut())
                            .map_err(|err| crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string()))?;
                        Ok(crate::bindings::bulwark::plugin::redis::Breaker {
                            generation,
                            successes,
                            failures,
                            consecutive_successes,
                            consecutive_failures,
                            expiration,
                        })
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }

    fn check_breaker<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = wasmtime::Result<
                        Result<
                            crate::bindings::bulwark::plugin::redis::Breaker,
                            crate::bindings::bulwark::plugin::redis::Error,
                        >,
                    >,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'ctx: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            Ok(
                // Inner function to permit ? operator
                || -> Result<crate::bindings::bulwark::plugin::redis::Breaker, crate::bindings::bulwark::plugin::redis::Error> {
                    verify_remote_state_prefixes(&self.permissions.state, &key)?;

                    if let Some(redis_info) = self.redis_info.clone() {
                        let mut conn = redis_info
                            .pool
                            .get()
                            .map_err(|err| crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string()))?;
                        let dt = Utc::now();
                        let timestamp: i64 = dt.timestamp();
                        let script = redis_info.registry.check_breaker.clone();
                        // Invoke the script and map to our breaker type
                        let (
                            generation,
                            successes,
                            failures,
                            consecutive_successes,
                            consecutive_failures,
                            expiration,
                        ) = script
                            .key(key)
                            .arg(timestamp)
                            .invoke::<(i64, i64, i64, i64, i64, i64)>(conn.deref_mut())
                            .map_err(|err| crate::bindings::bulwark::plugin::redis::Error::Remote(err.to_string()))?;
                        Ok(crate::bindings::bulwark::plugin::redis::Breaker {
                            generation,
                            successes,
                            failures,
                            consecutive_successes,
                            consecutive_failures,
                            expiration,
                        })
                    } else {
                        Err(crate::bindings::bulwark::plugin::redis::Error::Remote(
                            "no remote state configured".to_string(),
                        ))
                    }
                }(),
            )
        })
    }
}

/// Ensures that access to any HTTP host has the appropriate permissions set.
fn verify_http_domains(
    // TODO: BTreeSet<String> instead, all the way up
    allowed_http_domains: &[String],
    authority: &str,
) -> Result<(), bulwark_wasm_sdk::Error> {
    let parsed_uri = Url::parse(format!("//{}/", authority).as_str()).map_err(|e| {
        bulwark_wasm_sdk::error!("invalid request authority <{}>: {}", authority, e)
    })?;
    let requested_domain = parsed_uri.domain().ok_or_else(|| {
        bulwark_wasm_sdk::error!("request authority must be a valid dns name <{}>", authority)
    })?;
    if !allowed_http_domains.contains(&requested_domain.to_string()) {
        return Err(bulwark_wasm_sdk::error!(
            "missing http permissions <{}>",
            authority
        ));
    }
    Ok(())
}

/// Ensures that access to any remote state key has the appropriate permissions set.
fn verify_remote_state_prefixes(
    // TODO: BTreeSet<String> instead, all the way up
    allowed_key_prefixes: &[String],
    key: &str,
) -> Result<(), crate::bindings::bulwark::plugin::redis::Error> {
    let key = key.to_string();
    if !allowed_key_prefixes
        .iter()
        .any(|prefix| key.starts_with(prefix))
    {
        return Err(crate::bindings::bulwark::plugin::redis::Error::Permission(
            key,
        ));
    }
    Ok(())
}
