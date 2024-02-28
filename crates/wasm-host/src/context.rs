use {
    crate::ContextInstantiationError,
    crate::{Plugin, PluginStdio},
    core::{future::Future, marker::Send, pin::Pin},
    std::{collections::HashMap, sync::Arc},
    wasmtime::component::Resource,
    wasmtime_wasi::preview2::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiView},
    wasmtime_wasi_http::types::{
        HostFutureIncomingResponse, HostIncomingResponse, OutgoingRequest,
    },
    wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView},
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
    /// Plugin-specific configuration. Stored as bytes and deserialized as JSON values by the SDK.
    ///
    /// There may be multiple instances of the same plugin with different values for this configuration
    /// causing the plugin behavior to be different. For instance, a plugin might define a pattern-matching
    /// algorithm in its code while reading the specific patterns to match from this configuration.
    config: Arc<Vec<u8>>, // TODO: store this as a native type instead of serializing
    /// The set of permissions granted to a plugin.
    permissions: bulwark_config::Permissions,
    /// The Redis connection pool and its associated Lua scripts.
    redis_info: Option<Arc<RedisInfo>>,
    /// The HTTP client used to send outbound requests from plugins.
    http_client: Arc<reqwest::blocking::Client>,
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
        http_client: Arc<reqwest::blocking::Client>,
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
            config: Arc::new(plugin.guest_config()?),
            permissions: plugin.permissions(),
            redis_info,
            http_client,
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
        Err(anyhow::anyhow!("Not implemented yet"))
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
        Box::pin(async move {
            // First convert bytes to JSON, then extract the keys
            serde_json::to_value(self.config.to_vec())
                .map_err(wasmtime::Error::new)
                .map(|value| {
                    value
                        .as_object()
                        .map_or(vec![], |obj| obj.keys().cloned().collect())
                })
        })
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
            // First convert bytes to JSON, then extract the value we're after
            Ok(serde_json::to_value(self.config.to_vec())
            .map_err(|e| {
                crate::bindings::bulwark::plugin::config::Error::InvalidSerialization(
                    e.to_string(),
                )
            })
            .and_then(|value| {
                    value
                        .as_object()
                        .map_or(Ok(None), |obj| {
                            // Use match to invert the Option<Result<V, E>> to Result<Option<V>, E>
                            match obj.get(&key) {
                                Some(value) => {
                                    value.clone()
                                    .try_into()
                                    .map_err(|e: &'static str| {
                                        crate::bindings::bulwark::plugin::config::Error::InvalidConversion(
                                            e.to_string(),
                                        )
                                    })
                                    .map(Some)
                                },
                                None => Ok(None),
                            }
                        })
                }))
        })
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
        todo!()
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
        todo!()
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
        todo!()
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
        todo!()
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
        todo!()
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
        todo!()
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
        todo!()
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
        todo!()
    }

    /// Sets the time to live for the given key.
    fn expire<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
        ttl: i64,
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
        todo!()
    }

    /// Sets the expiration for the given key to the given unix time.
    fn expire_at<'ctx, 'async_trait>(
        &'ctx mut self,
        key: String,
        unix_time: i64,
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
        todo!()
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
        todo!()
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
        todo!()
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
        todo!()
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
        todo!()
    }
}

// #[async_trait]
// impl bulwark_host::HostApiImports for PluginContext {
//     /// Returns the guest environment's configuration value as serialized JSON.
//     async fn get_config(&mut self) -> Result<Vec<u8>, wasmtime::Error> {
//         Ok(self.read_only_ctx.config.to_vec())
//     }

//     /// Returns a named value from the request context's params.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The key name corresponding to the param value.
//     async fn get_param_value(
//         &mut self,
//         key: String,
//     ) -> Result<Result<Vec<u8>, bulwark_host::ParamError>, wasmtime::Error> {
//         let params = self.guest_mut_ctx.params.lock().expect("poisoned mutex");
//         let value = params.get(&key).unwrap_or(&bulwark_wasm_sdk::Value::Null);
//         match serde_json::to_vec(value) {
//             Ok(bytes) => Ok(Ok(bytes)),
//             Err(err) => Ok(Err(bulwark_host::ParamError::Json(err.to_string()))),
//         }
//     }

//     /// Set a named value in the request context's params.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The key name corresponding to the param value.
//     /// * `value` - The value to record. Values are serialized JSON.
//     async fn set_param_value(
//         &mut self,
//         key: String,
//         value: Vec<u8>,
//     ) -> Result<Result<(), bulwark_host::ParamError>, wasmtime::Error> {
//         let mut params = self.guest_mut_ctx.params.lock().expect("poisoned mutex");
//         match serde_json::from_slice(&value) {
//             Ok(value) => {
//                 params.insert(key, value);
//                 Ok(Ok(()))
//             }
//             Err(err) => Ok(Err(bulwark_host::ParamError::Json(err.to_string()))),
//         }
//     }

//     /// Returns a named environment variable value as bytes.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The environment variable name. Case-sensitive.
//     async fn get_env_bytes(
//         &mut self,
//         key: String,
//     ) -> Result<Result<Vec<u8>, bulwark_host::EnvError>, wasmtime::Error> {
//         let allowed_env_vars = self
//             .read_only_ctx
//             .permissions
//             .env
//             .iter()
//             .cloned()
//             .collect::<BTreeSet<String>>();
//         if !allowed_env_vars.contains(&key) {
//             return Ok(Err(bulwark_host::EnvError::Permission(key)));
//         }
//         match std::env::var(&key) {
//             Ok(var) => Ok(Ok(var.as_bytes().to_vec())),
//             Err(err) => match err {
//                 std::env::VarError::NotPresent => Ok(Err(bulwark_host::EnvError::Missing(key))),
//                 std::env::VarError::NotUnicode(_) => {
//                     Ok(Err(bulwark_host::EnvError::NotUnicode(key)))
//                 }
//             },
//         }
//     }

//     /// Returns the incoming request associated with the request context.
//     async fn get_request(&mut self) -> Result<bulwark_host::RequestInterface, wasmtime::Error> {
//         let request = self.host_mut_ctx.request.lock().expect("poisoned mutex");
//         Ok(request.clone())
//     }

//     /// Returns the response received from the interior service.
//     ///
//     /// If called from `on_request` or `on_request_decision`, it will return `None` since a response
//     /// is not yet available.
//     async fn get_response(
//         &mut self,
//     ) -> Result<Option<bulwark_host::ResponseInterface>, wasmtime::Error> {
//         let response: MutexGuard<Option<bulwark_host::ResponseInterface>> =
//             self.host_mut_ctx.response.lock().expect("poisoned mutex");
//         Ok(response.to_owned())
//     }

//     /// Determines whether the request body will be received by the plugin in the `on_request_body_decision` handler.
//     async fn receive_request_body(&mut self, body: bool) -> Result<(), wasmtime::Error> {
//         let mut receive_request_body = self
//             .guest_mut_ctx
//             .receive_request_body
//             .lock()
//             .expect("poisoned mutex");
//         *receive_request_body = body;
//         Ok(())
//     }

//     /// Determines whether the response body will be received by the plugin in the `on_response_body_decision` handler.
//     async fn receive_response_body(&mut self, body: bool) -> Result<(), wasmtime::Error> {
//         let mut receive_response_body = self
//             .guest_mut_ctx
//             .receive_response_body
//             .lock()
//             .expect("poisoned mutex");
//         *receive_response_body = body;
//         Ok(())
//     }

//     /// Returns the originating client's IP address, if available.
//     async fn get_client_ip(
//         &mut self,
//     ) -> Result<Option<bulwark_host::IpInterface>, wasmtime::Error> {
//         Ok(self.read_only_ctx.client_ip)
//     }

//     /// Begins an outbound request. Returns a request ID used by `add_request_header` and `set_request_body`.
//     ///
//     /// # Arguments
//     ///
//     /// * `method` - The HTTP method
//     /// * `uri` - The absolute URI of the resource to request
//     async fn send_request(
//         &mut self,
//         request: bulwark_host::RequestInterface,
//     ) -> Result<Result<bulwark_host::ResponseInterface, bulwark_host::HttpError>, wasmtime::Error>
//     {
//         Ok(
//             // Inner function to permit ? operator
//             || -> Result<bulwark_host::ResponseInterface, bulwark_host::HttpError> {
//                 verify_http_domains(&self.read_only_ctx.permissions.http, &request.uri)?;

//                 let method = reqwest::Method::from_str(&request.method)
//                     .map_err(|_| bulwark_host::HttpError::InvalidMethod(request.method.clone()))?;

//                 let mut builder = self.read_only_ctx.http_client.request(method, &request.uri);
//                 for (name, value) in request.headers {
//                     builder = builder.header(name, value);
//                 }

//                 if !request.end_of_stream {
//                     return Err(bulwark_host::HttpError::UnavailableContent(
//                         "the entire request body must be available".to_string(),
//                     ));
//                 } else if request.chunk_start != 0 {
//                     return Err(bulwark_host::HttpError::InvalidStart(
//                         "chunk start must be 0".to_string(),
//                     ));
//                 } else if request.chunk_length > 16384 {
//                     return Err(bulwark_host::HttpError::ContentTooLarge(
//                         "the entire request body must be 16384 bytes or less".to_string(),
//                     ));
//                 }

//                 builder = builder.body(request.chunk);

//                 let response = builder
//                     .send()
//                     .map_err(|err| bulwark_host::HttpError::Transmit(err.to_string()))?;
//                 let status: u32 = response.status().as_u16() as u32;
//                 // need to read headers before body because retrieving body bytes will move the response
//                 let headers: Vec<(String, Vec<u8>)> = response
//                     .headers()
//                     .iter()
//                     .map(|(name, value)| (name.to_string(), value.as_bytes().to_vec()))
//                     .collect();
//                 let body = response.bytes().unwrap().to_vec();
//                 let content_length: u64 = body.len() as u64;
//                 Ok(bulwark_host::ResponseInterface {
//                     status,
//                     headers,
//                     body_received: true,
//                     chunk: body,
//                     chunk_start: 0,
//                     chunk_length: content_length,
//                     end_of_stream: true,
//                 })
//             }(),
//         )
//     }

//     /// Records the decision value the plugin wants to return.
//     ///
//     /// # Arguments
//     ///
//     /// * `decision` - The [`Decision`] output of the plugin.
//     async fn set_decision(
//         &mut self,
//         decision: bindings::Decision,
//     ) -> Result<Result<(), bulwark_host::DecisionError>, wasmtime::Error> {
//         let decision = Decision::from(decision);
//         // Validate on both the guest and the host because we can't guarantee usage of the SDK.
//         match decision.validate() {
//             Ok(_) => {
//                 self.guest_mut_ctx.decision_components.decision = decision;
//                 Ok(Ok(()))
//             }
//             Err(err) => Ok(Err(bulwark_host::DecisionError::Invalid(err.to_string()))),
//         }
//     }

//     /// Records the tags the plugin wants to associate with its decision.
//     ///
//     /// # Arguments
//     ///
//     /// * `tags` - The list of tags to associate with a [`Decision`].
//     async fn set_tags(&mut self, tags: Vec<String>) -> Result<(), wasmtime::Error> {
//         self.guest_mut_ctx.decision_components.tags = tags;
//         Ok(())
//     }

//     /// Records additional tags the plugin wants to associate with its decision. Existing tags will be kept.
//     ///
//     /// # Arguments
//     ///
//     /// * `tags` - The list of tags to associate with a [`Decision`].
//     async fn append_tags(&mut self, mut tags: Vec<String>) -> Result<Vec<String>, wasmtime::Error> {
//         self.guest_mut_ctx
//             .decision_components
//             .tags
//             .append(&mut tags);
//         Ok(self.guest_mut_ctx.decision_components.tags.clone())
//     }

//     /// Returns the combined decision, if available.
//     ///
//     /// Typically used in the feedback phase.
//     async fn get_combined_decision(
//         &mut self,
//     ) -> Result<Option<bindings::Decision>, wasmtime::Error> {
//         let combined_decision: MutexGuard<Option<bindings::Decision>> = self
//             .host_mut_ctx
//             .combined_decision
//             .lock()
//             .expect("poisoned mutex");
//         Ok(combined_decision.to_owned())
//     }

//     /// Returns the combined set of tags associated with a decision, if available.
//     ///
//     /// Typically used in the feedback phase.
//     async fn get_combined_tags(&mut self) -> Result<Option<Vec<String>>, wasmtime::Error> {
//         let combined_tags: MutexGuard<Option<Vec<String>>> = self
//             .host_mut_ctx
//             .combined_tags
//             .lock()
//             .expect("poisoned mutex");
//         Ok(combined_tags.to_owned())
//     }

//     /// Returns the outcome of the combined decision, if available.
//     ///
//     /// Typically used in the feedback phase.
//     async fn get_outcome(
//         &mut self,
//     ) -> Result<Option<bulwark_host::OutcomeInterface>, wasmtime::Error> {
//         let outcome: MutexGuard<Option<bulwark_host::OutcomeInterface>> =
//             self.host_mut_ctx.outcome.lock().expect("poisoned mutex");
//         Ok(outcome.to_owned())
//     }

//     /// Returns the named state value retrieved from Redis.
//     ///
//     /// Also used to retrieve a counter value.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The key name corresponding to the state value.
//     async fn get_remote_state(
//         &mut self,
//         key: String,
//     ) -> Result<Result<Vec<u8>, bulwark_host::StateError>, wasmtime::Error> {
//         Ok(
//             // Inner function to permit ? operator
//             || -> Result<Vec<u8>, bulwark_host::StateError> {
//                 verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

//                 if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
//                     let mut conn = redis_info
//                         .pool
//                         .get()
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;

//                     Ok(conn
//                         .get(key)
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?)
//                 } else {
//                     Err(bulwark_host::StateError::Remote(
//                         "no remote state configured".to_string(),
//                     ))
//                 }
//             }(),
//         )
//     }

//     /// Set a named value in Redis.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The key name corresponding to the state value.
//     /// * `value` - The value to record. Values are byte strings, but may be interpreted differently by Redis depending on context.
//     async fn set_remote_state(
//         &mut self,
//         key: String,
//         value: Vec<u8>,
//     ) -> Result<Result<(), bulwark_host::StateError>, wasmtime::Error> {
//         Ok(
//             // Inner function to permit ? operator
//             || -> Result<(), bulwark_host::StateError> {
//                 verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

//                 if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
//                     let mut conn = redis_info
//                         .pool
//                         .get()
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;

//                     conn.set::<String, Vec<u8>, redis::Value>(key, value)
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
//                     Ok(())
//                 } else {
//                     Err(bulwark_host::StateError::Remote(
//                         "no remote state configured".to_string(),
//                     ))
//                 }
//             }(),
//         )
//     }

//     /// Increments a named counter in Redis.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The key name corresponding to the state counter.
//     async fn increment_remote_state(
//         &mut self,
//         key: String,
//     ) -> Result<Result<i64, bulwark_host::StateError>, wasmtime::Error> {
//         self.increment_remote_state_by(key, 1).await
//     }

//     /// Increments a named counter in Redis by a specified delta value.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The key name corresponding to the state counter.
//     /// * `delta` - The amount to increase the counter by.
//     async fn increment_remote_state_by(
//         &mut self,
//         key: String,
//         delta: i64,
//     ) -> Result<Result<i64, bulwark_host::StateError>, wasmtime::Error> {
//         Ok(
//             // Inner function to permit ? operator
//             || -> Result<i64, bulwark_host::StateError> {
//                 verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

//                 if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
//                     let mut conn = redis_info
//                         .pool
//                         .get()
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;

//                     Ok(conn
//                         .incr(key, delta)
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?)
//                 } else {
//                     Err(bulwark_host::StateError::Remote(
//                         "no remote state configured".to_string(),
//                     ))
//                 }
//             }(),
//         )
//     }

//     /// Sets an expiration on a named value in Redis.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The key name corresponding to the state value.
//     /// * `ttl` - The time-to-live for the value in seconds.
//     async fn set_remote_ttl(
//         &mut self,
//         key: String,
//         ttl: i64,
//     ) -> Result<Result<(), bulwark_host::StateError>, wasmtime::Error> {
//         Ok(
//             // Inner function to permit ? operator
//             || -> Result<(), bulwark_host::StateError> {
//                 verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

//                 if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
//                     let mut conn = redis_info
//                         .pool
//                         .get()
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;

//                     conn.expire::<String, redis::Value>(key, ttl as usize)
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
//                     Ok(())
//                 } else {
//                     Err(bulwark_host::StateError::Remote(
//                         "no remote state configured".to_string(),
//                     ))
//                 }
//             }(),
//         )
//     }

//     /// Increments a rate limit, returning the number of attempts so far and the expiration time.
//     ///
//     /// The rate limiter is a counter over a period of time. At the end of the period, it will expire,
//     /// beginning a new period. Window periods should be set to the longest amount of time that a client should
//     /// be locked out for. The plugin is responsible for performing all rate-limiting logic with the counter
//     /// value it receives.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The key name corresponding to the state counter.
//     /// * `delta` - The amount to increase the counter by.
//     /// * `window` - How long each period should be in seconds.
//     async fn increment_rate_limit(
//         &mut self,
//         key: String,
//         delta: i64,
//         window: i64,
//     ) -> Result<Result<bulwark_host::RateInterface, bulwark_host::StateError>, wasmtime::Error>
//     {
//         Ok(
//             // Inner function to permit ? operator
//             || -> Result<bulwark_host::RateInterface, bulwark_host::StateError> {
//                 verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

//                 if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
//                     let mut conn = redis_info
//                         .pool
//                         .get()
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
//                     let dt = Utc::now();
//                     let timestamp: i64 = dt.timestamp();
//                     let script = redis_info.registry.increment_rate_limit.clone();
//                     // Invoke the script and map to our rate type
//                     let (attempts, expiration) = script
//                         .key(key)
//                         .arg(delta)
//                         .arg(window)
//                         .arg(timestamp)
//                         .invoke::<(i64, i64)>(conn.deref_mut())
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
//                     Ok(bulwark_host::RateInterface {
//                         attempts,
//                         expiration,
//                     })
//                 } else {
//                     Err(bulwark_host::StateError::Remote(
//                         "no remote state configured".to_string(),
//                     ))
//                 }
//             }(),
//         )
//     }

//     /// Checks a rate limit, returning the number of attempts so far and the expiration time.
//     ///
//     /// See `increment_rate_limit`.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The key name corresponding to the state counter.
//     async fn check_rate_limit(
//         &mut self,
//         key: String,
//     ) -> Result<Result<bulwark_host::RateInterface, bulwark_host::StateError>, wasmtime::Error>
//     {
//         Ok(
//             // Inner function to permit ? operator
//             || -> Result<bulwark_host::RateInterface, bulwark_host::StateError> {
//                 verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

//                 if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
//                     let mut conn = redis_info
//                         .pool
//                         .get()
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
//                     let dt = Utc::now();
//                     let timestamp: i64 = dt.timestamp();
//                     let script = redis_info.registry.check_rate_limit.clone();
//                     // Invoke the script and map to our rate type
//                     let (attempts, expiration) = script
//                         .key(key)
//                         .arg(timestamp)
//                         .invoke::<(i64, i64)>(conn.deref_mut())
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
//                     Ok(bulwark_host::RateInterface {
//                         attempts,
//                         expiration,
//                     })
//                 } else {
//                     Err(bulwark_host::StateError::Remote(
//                         "no remote state configured".to_string(),
//                     ))
//                 }
//             }(),
//         )
//     }

//     /// Increments a circuit breaker, returning the generation count, success count, failure count,
//     /// consecutive success count, consecutive failure count, and expiration time.
//     ///
//     /// The plugin is responsible for performing all circuit-breaking logic with the counter
//     /// values it receives. The host environment does as little as possible to maximize how much
//     /// control the plugin has over the behavior of the breaker.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The key name corresponding to the state counter.
//     /// * `success_delta` - The amount to increase the success counter by. Generally zero on failure.
//     /// * `failure_delta` - The amount to increase the failure counter by. Generally zero on success.
//     /// * `window` - How long each period should be in seconds.
//     async fn increment_breaker(
//         &mut self,
//         key: String,
//         success_delta: i64,
//         failure_delta: i64,
//         window: i64,
//     ) -> Result<Result<bulwark_host::BreakerInterface, bulwark_host::StateError>, wasmtime::Error>
//     {
//         Ok(
//             // Inner function to permit ? operator
//             || -> Result<bulwark_host::BreakerInterface, bulwark_host::StateError> {
//                 verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

//                 if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
//                     let mut conn = redis_info
//                         .pool
//                         .get()
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
//                     let dt = Utc::now();
//                     let timestamp: i64 = dt.timestamp();
//                     let script = redis_info.registry.increment_breaker.clone();
//                     // Invoke the script and map to our breaker type
//                     let (
//                         generation,
//                         successes,
//                         failures,
//                         consecutive_successes,
//                         consecutive_failures,
//                         expiration,
//                     ) = script
//                         .key(key)
//                         .arg(success_delta)
//                         .arg(failure_delta)
//                         .arg(window)
//                         .arg(timestamp)
//                         .invoke::<(i64, i64, i64, i64, i64, i64)>(conn.deref_mut())
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
//                     Ok(bulwark_host::BreakerInterface {
//                         generation,
//                         successes,
//                         failures,
//                         consecutive_successes,
//                         consecutive_failures,
//                         expiration,
//                     })
//                 } else {
//                     Err(bulwark_host::StateError::Remote(
//                         "no remote state configured".to_string(),
//                     ))
//                 }
//             }(),
//         )
//     }

//     /// Checks a circuit breaker, returning the generation count, success count, failure count,
//     /// consecutive success count, consecutive failure count, and expiration time.
//     ///
//     /// See `increment_breaker`.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - The key name corresponding to the state counter.
//     async fn check_breaker(
//         &mut self,
//         key: String,
//     ) -> Result<Result<bulwark_host::BreakerInterface, bulwark_host::StateError>, wasmtime::Error>
//     {
//         Ok(
//             // Inner function to permit ? operator
//             || -> Result<bulwark_host::BreakerInterface, bulwark_host::StateError> {
//                 verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

//                 if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
//                     let mut conn = redis_info
//                         .pool
//                         .get()
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
//                     let dt = Utc::now();
//                     let timestamp: i64 = dt.timestamp();
//                     let script = redis_info.registry.check_breaker.clone();
//                     // Invoke the script and map to our breaker type
//                     let (
//                         generation,
//                         successes,
//                         failures,
//                         consecutive_successes,
//                         consecutive_failures,
//                         expiration,
//                     ) = script
//                         .key(key)
//                         .arg(timestamp)
//                         .invoke::<(i64, i64, i64, i64, i64, i64)>(conn.deref_mut())
//                         .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
//                     Ok(bulwark_host::BreakerInterface {
//                         generation,
//                         successes,
//                         failures,
//                         consecutive_successes,
//                         consecutive_failures,
//                         expiration,
//                     })
//                 } else {
//                     Err(bulwark_host::StateError::Remote(
//                         "no remote state configured".to_string(),
//                     ))
//                 }
//             }(),
//         )
//     }
// }

// /// Ensures that access to any HTTP host has the appropriate permissions set.
// fn verify_http_domains(
//     // TODO: BTreeSet<String> instead, all the way up
//     allowed_http_domains: &[String],
//     uri: &str,
// ) -> Result<(), bulwark_host::HttpError> {
//     let parsed_uri =
//         Url::parse(uri).map_err(|_| bulwark_host::HttpError::InvalidUri(uri.to_string()))?;
//     let requested_domain = parsed_uri
//         .domain()
//         .ok_or_else(|| bulwark_host::HttpError::InvalidUri(uri.to_string()))?;
//     if !allowed_http_domains.contains(&requested_domain.to_string()) {
//         return Err(bulwark_host::HttpError::Permission(uri.to_string()));
//     }
//     Ok(())
// }

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
