#[doc(hidden)]
mod bulwark_host {
    wasmtime::component::bindgen!({
        world: "bulwark:plugin/host-api",
        async: true,
    });
}

#[doc(hidden)]
mod handlers {
    wasmtime::component::bindgen!({
        world: "bulwark:plugin/handlers",
        async: true,
    });
}

use {
    crate::{
        ContextInstantiationError, PluginExecutionError, PluginInstantiationError, PluginLoadError,
    },
    async_trait::async_trait,
    bulwark_config::ConfigSerializationError,
    bulwark_host::{DecisionInterface, OutcomeInterface},
    bulwark_wasm_sdk::{Decision, Outcome},
    chrono::Utc,
    redis::Commands,
    std::str::FromStr,
    std::{
        collections::BTreeSet,
        convert::From,
        net::IpAddr,
        ops::DerefMut,
        path::Path,
        sync::{Arc, Mutex, MutexGuard, RwLock},
    },
    url::Url,
    validator::Validate,
    wasmtime::component::{Component, Linker},
    wasmtime::{AsContextMut, Config, Engine, Store},
    wasmtime_wasi::preview2::{
        pipe::{ReadPipe, WritePipe},
        Table, WasiCtx, WasiCtxBuilder, WasiView,
    },
};

extern crate redis;

/// Wraps an [`IpAddr`] representing the remote IP for the incoming request.
///
/// In an architecture with proxies or load balancers in front of Bulwark, this IP will belong to the immediately
/// exterior proxy or load balancer rather than the IP address of the client that originated the request.
#[derive(Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct RemoteIP(pub IpAddr);
/// Wraps an [`IpAddr`] representing the forwarded IP for the incoming request.
///
/// In an architecture with proxies or load balancers in front of Bulwark, this IP will belong to the IP address
/// of the client that originated the request rather than the immediately exterior proxy or load balancer.
#[derive(Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ForwardedIP(pub IpAddr);

// TODO: from.rs

impl From<Arc<bulwark_wasm_sdk::Request>> for bulwark_host::RequestInterface {
    fn from(request: Arc<bulwark_wasm_sdk::Request>) -> Self {
        bulwark_host::RequestInterface {
            method: request.method().to_string(),
            uri: request.uri().to_string(),
            version: format!("{:?}", request.version()),
            headers: request
                .headers()
                .iter()
                .map(|(name, value)| (name.to_string(), value.as_bytes().to_vec()))
                .collect(),
            body_received: request.body().received,
            chunk_start: request.body().start,
            chunk_length: request.body().size,
            end_of_stream: request.body().end_of_stream,
            // TODO: figure out how to avoid the copy
            chunk: request.body().content.clone(),
        }
    }
}

impl From<Arc<bulwark_wasm_sdk::Response>> for bulwark_host::ResponseInterface {
    fn from(response: Arc<bulwark_wasm_sdk::Response>) -> Self {
        bulwark_host::ResponseInterface {
            // this unwrap should be okay since a non-zero u16 should always be coercible to u32
            status: response.status().as_u16().try_into().unwrap(),
            headers: response
                .headers()
                .iter()
                .map(|(name, value)| (name.to_string(), value.as_bytes().to_vec()))
                .collect(),
            body_received: response.body().received,
            chunk_start: response.body().start,
            chunk_length: response.body().size,
            end_of_stream: response.body().end_of_stream,
            // TODO: figure out how to avoid the copy
            chunk: response.body().content.clone(),
        }
    }
}

impl From<IpAddr> for bulwark_host::IpInterface {
    fn from(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                bulwark_host::IpInterface::V4((octets[0], octets[1], octets[2], octets[3]))
            }
            IpAddr::V6(v6) => {
                let segments = v6.segments();
                bulwark_host::IpInterface::V6((
                    segments[0],
                    segments[1],
                    segments[2],
                    segments[3],
                    segments[4],
                    segments[5],
                    segments[6],
                    segments[7],
                ))
            }
        }
    }
}

impl From<DecisionInterface> for Decision {
    fn from(decision: DecisionInterface) -> Self {
        Decision {
            accept: decision.accept,
            restrict: decision.restrict,
            unknown: decision.unknown,
        }
    }
}

impl From<Decision> for DecisionInterface {
    fn from(decision: Decision) -> Self {
        DecisionInterface {
            accept: decision.accept,
            restrict: decision.restrict,
            unknown: decision.unknown,
        }
    }
}

impl From<Outcome> for OutcomeInterface {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Trusted => OutcomeInterface::Trusted,
            Outcome::Accepted => OutcomeInterface::Accepted,
            Outcome::Suspected => OutcomeInterface::Suspected,
            Outcome::Restricted => OutcomeInterface::Restricted,
        }
    }
}

/// The primary output of a [`PluginInstance`]'s execution. Combines a [`Decision`] and a list of tags together.
///
/// Both the output of individual plugins as well as the combined decision output of a group of plugins may be
/// represented by `DecisionComponents`. The latter is the result of applying Dempster-Shafer combination to each
/// `decision` value in a [`DecisionComponents`] list and then taking the union set of all `tags` lists and forming
/// a new [`DecisionComponents`] with both results.
#[derive(Clone, Default)]
pub struct DecisionComponents {
    /// A `Decision` made by a plugin or a group of plugins
    pub decision: Decision,
    /// The tags applied by plugins to annotate a [`Decision`]
    pub tags: Vec<String>,
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

/// The RequestContext provides a store of information that needs to cross the plugin sandbox boundary.
pub struct RequestContext {
    /// The WASI context that determines how things like stdio map to our buffers.
    wasi_ctx: WasiCtx,
    /// The WASI table that maps handles to resources.
    wasi_table: Table,
    /// Context values that will not be modified.
    read_only_ctx: ReadOnlyContext,
    /// Context values that will be mutated by the guest environment.
    guest_mut_ctx: GuestMutableContext,
    /// Context values that will be mutated by the host environment.
    host_mut_ctx: HostMutableContext,
    /// The standard I/O buffers used by WASI and captured for logging.
    stdio: PluginStdio,
}

impl RequestContext {
    /// Creates a new `RequestContext`.
    ///
    /// # Arguments
    ///
    /// * `plugin` - The [`Plugin`] and its associated configuration.
    /// * `redis_info` - The Redis connection pool.
    /// * `params` - A key-value map that plugins use to pass values within the context of a request.
    ///     Any parameters captured by the router will be added to this before plugin execution.
    /// * `request` - The [`Request`](bulwark_wasm_sdk::Request) that plugins will be operating on.
    pub fn new(
        plugin: Arc<Plugin>,
        redis_info: Option<Arc<RedisInfo>>,
        http_client: Arc<reqwest::blocking::Client>,
        params: Arc<Mutex<bulwark_wasm_sdk::Map<String, bulwark_wasm_sdk::Value>>>,
        request: Arc<bulwark_wasm_sdk::Request>,
    ) -> Result<RequestContext, ContextInstantiationError> {
        let stdio = PluginStdio::default();
        let mut wasi_table = Table::new();
        let wasi_ctx = WasiCtxBuilder::new()
            .set_stdin(ReadPipe::from_shared(stdio.stdin.clone()))
            .set_stdout(WritePipe::from_shared(stdio.stdout.clone()))
            .set_stderr(WritePipe::from_shared(stdio.stderr.clone()))
            .build(&mut wasi_table)?;
        let client_ip = request
            .extensions()
            .get::<ForwardedIP>()
            .map(|forwarded_ip| bulwark_host::IpInterface::from(forwarded_ip.0));

        Ok(RequestContext {
            wasi_ctx,
            wasi_table,
            read_only_ctx: ReadOnlyContext {
                config: Arc::new(plugin.guest_config()?),
                permissions: plugin.permissions(),
                client_ip,
                redis_info,
                http_client,
            },
            guest_mut_ctx: GuestMutableContext {
                receive_request_body: Arc::new(Mutex::new(false)),
                receive_response_body: Arc::new(Mutex::new(false)),
                params,
                decision_components: DecisionComponents::default(),
            },
            host_mut_ctx: HostMutableContext::new(bulwark_host::RequestInterface::from(request)),
            stdio,
        })
    }
}

impl WasiView for RequestContext {
    fn table(&self) -> &Table {
        &self.wasi_table
    }

    fn table_mut(&mut self) -> &mut Table {
        &mut self.wasi_table
    }

    fn ctx(&self) -> &WasiCtx {
        &self.wasi_ctx
    }

    fn ctx_mut(&mut self) -> &mut WasiCtx {
        &mut self.wasi_ctx
    }
}

/// A singular detection plugin and provides the interface between WASM host and guest.
///
/// One `Plugin` may spawn many [`PluginInstance`]s, which will handle the incoming request data.
#[derive(Clone)]
pub struct Plugin {
    reference: String,
    config: Arc<bulwark_config::Plugin>,
    engine: Engine,
    component: Component,
}

impl Plugin {
    /// Creates and compiles a new [`Plugin`] from a [`String`] of
    /// [WAT](https://webassembly.github.io/spec/core/text/index.html)-formatted WASM.
    pub fn from_wat(
        name: String,
        wat: &str,
        config: &bulwark_config::Plugin,
    ) -> Result<Self, PluginLoadError> {
        Self::from_component(
            name,
            config,
            |engine| -> Result<Component, PluginLoadError> {
                Ok(Component::new(engine, wat.as_bytes())?)
            },
        )
    }

    /// Creates and compiles a new [`Plugin`] from a byte slice of WASM.
    ///
    /// The bytes it expects are what you'd get if you read in a `*.wasm` file.
    /// See [`Component::from_binary`].
    pub fn from_bytes(
        name: String,
        bytes: &[u8],
        config: &bulwark_config::Plugin,
    ) -> Result<Self, PluginLoadError> {
        Self::from_component(
            name,
            config,
            |engine| -> Result<Component, PluginLoadError> {
                Ok(Component::from_binary(engine, bytes)?)
            },
        )
    }

    /// Creates and compiles a new [`Plugin`] by reading in a file in either `*.wasm` or `*.wat` format.
    ///
    /// See [`Component::from_file`].
    pub fn from_file(
        path: impl AsRef<Path>,
        config: &bulwark_config::Plugin,
    ) -> Result<Self, PluginLoadError> {
        let name = config.reference.clone();
        Self::from_component(
            name,
            config,
            |engine| -> Result<Component, PluginLoadError> {
                Ok(Component::from_file(engine, &path)?)
            },
        )
    }

    /// Helper method for the other `from_*` functions.
    fn from_component<F>(
        reference: String,
        config: &bulwark_config::Plugin,
        mut get_component: F,
    ) -> Result<Self, PluginLoadError>
    where
        F: FnMut(&Engine) -> Result<Component, PluginLoadError>,
    {
        let mut wasm_config = Config::new();
        wasm_config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
        wasm_config.wasm_multi_memory(true);
        wasm_config.wasm_component_model(true);
        wasm_config.async_support(true);

        let engine = Engine::new(&wasm_config)?;
        let component = get_component(&engine)?;

        Ok(Plugin {
            reference,
            config: Arc::new(config.clone()),
            engine,
            component,
        })
    }

    /// Makes the guest's configuration available as serialized JSON bytes.
    fn guest_config(&self) -> Result<Vec<u8>, ConfigSerializationError> {
        // TODO: should guest config be required or optional?
        self.config.config_to_json()
    }

    /// Makes the permissions the plugin has been granted available to the guest environment.
    fn permissions(&self) -> bulwark_config::Permissions {
        self.config.permissions.clone()
    }
}

/// A collection of values that will not change over the lifecycle of a request/response.
struct ReadOnlyContext {
    /// Plugin-specific configuration. Stored as bytes and deserialized as JSON values by the SDK.
    ///
    /// There may be multiple instances of the same plugin with different values for this configuration
    /// causing the plugin behavior to be different. For instance, a plugin might define a pattern-matching
    /// algorithm in its code while reading the specific patterns to match from this configuration.
    config: Arc<Vec<u8>>,
    /// The set of permissions granted to a plugin.
    permissions: bulwark_config::Permissions,
    /// The IP address of the client that originated the request, if available.
    client_ip: Option<bulwark_host::IpInterface>,
    /// The Redis connection pool and its associated Lua scripts.
    redis_info: Option<Arc<RedisInfo>>,
    /// The HTTP client used to send outbound requests from plugins.
    http_client: Arc<reqwest::blocking::Client>,
}

/// A collection of values that the guest environment will mutate over the lifecycle of a request/response.
#[derive(Clone, Default)]
struct GuestMutableContext {
    /// Whether this plugin instance expects to process a request body.
    receive_request_body: Arc<Mutex<bool>>,
    /// Whether this plugin instance expects to process a response body.
    receive_response_body: Arc<Mutex<bool>>,
    /// The `params` are a key-value map shared between all plugin instances for a single request.
    params: Arc<Mutex<bulwark_wasm_sdk::Map<String, bulwark_wasm_sdk::Value>>>,
    /// The plugin's decision and tags annotating it.
    decision_components: DecisionComponents,
}

/// A collection of values that the host environment will mutate over the lifecycle of a request/response.
#[derive(Clone)]
struct HostMutableContext {
    /// The HTTP request received from the exterior client.
    request: Arc<Mutex<bulwark_host::RequestInterface>>,
    /// The HTTP response received from the interior service.
    response: Arc<Mutex<Option<bulwark_host::ResponseInterface>>>,
    /// The combined decision of all plugins at the end of the request phase.
    ///
    /// Accessible to plugins in the response and feedback phases.
    combined_decision: Arc<Mutex<Option<bulwark_host::DecisionInterface>>>,
    /// The combined union set of all tags attached by plugins across all phases.
    combined_tags: Arc<Mutex<Option<Vec<String>>>>,
    /// The decision outcome after the decision has been checked against configured thresholds.
    outcome: Arc<Mutex<Option<bulwark_host::OutcomeInterface>>>,
}

impl HostMutableContext {
    fn new(request: bulwark_host::RequestInterface) -> Self {
        HostMutableContext {
            request: Arc::new(Mutex::new(request)),
            response: Arc::new(Mutex::new(None)),
            combined_decision: Arc::new(Mutex::new(None)),
            combined_tags: Arc::new(Mutex::new(None)),
            outcome: Arc::new(Mutex::new(None)),
        }
    }
}

/// Wraps buffers to capture plugin stdio.
#[derive(Clone, Default)]
pub struct PluginStdio {
    stdin: Arc<RwLock<std::io::Cursor<Vec<u8>>>>,
    stdout: Arc<RwLock<std::io::Cursor<Vec<u8>>>>,
    stderr: Arc<RwLock<std::io::Cursor<Vec<u8>>>>,
}

impl PluginStdio {
    pub fn into_inner(&self) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        (
            self.stdin
                .read()
                .expect("poisoned mutex")
                .clone()
                .into_inner(),
            self.stdout
                .read()
                .expect("poisoned mutex")
                .clone()
                .into_inner(),
            self.stderr
                .read()
                .expect("poisoned mutex")
                .clone()
                .into_inner(),
        )
    }
}

/// An instance of a [`Plugin`], associated with a [`RequestContext`].
pub struct PluginInstance {
    /// A reference to the parent `Plugin` and its configuration.
    plugin: Arc<Plugin>,
    /// The WASM store that holds state associated with the incoming request.
    store: Store<RequestContext>,
    handlers: handlers::Handlers,
    receive_request_body: Arc<Mutex<bool>>,
    receive_response_body: Arc<Mutex<bool>>,
    /// All plugin-visible state that the host environment will mutate over the lifecycle of a request/response.
    host_mut_ctx: HostMutableContext,
    /// The buffers for `stdin`, `stdout`, and `stderr` used by the plugin for I/O.
    stdio: PluginStdio,
}

impl PluginInstance {
    /// Instantiates a [`Plugin`], creating a new `PluginInstance`.
    ///
    /// # Arguments
    ///
    /// * `plugin` - The plugin we are creating a `PluginInstance` for.
    /// * `request_context` - The request context stores all of the state associated with an incoming request and its corresponding response.
    pub async fn new(
        plugin: Arc<Plugin>,
        request_context: RequestContext,
    ) -> Result<PluginInstance, PluginInstantiationError> {
        // Clone the request/response body receive flags so we can provide them to the service layer.
        let receive_request_body = request_context.guest_mut_ctx.receive_request_body.clone();
        let receive_response_body = request_context.guest_mut_ctx.receive_response_body.clone();

        // Clone the host mutable context so that we can make changes to the interior of our request context from the parent.
        let host_mut_ctx = request_context.host_mut_ctx.clone();

        // Clone the stdio so we can read the captured stdout and stderr buffers after execution has completed.
        let stdio = request_context.stdio.clone();

        // TODO: do we need to retain a reference to the linker value anywhere? explore how other wasm-based systems use it.
        // convert from normal request struct to wasm request interface
        let mut linker: Linker<RequestContext> = Linker::new(&plugin.engine);

        wasmtime_wasi::preview2::wasi::command::add_to_linker(&mut linker)?;

        let mut store = Store::new(&plugin.engine, request_context);
        bulwark_host::HostApi::add_to_linker(&mut linker, |ctx: &mut RequestContext| ctx)?;

        // We discard the instance for this because we only use the generated interface to make calls

        let (handlers, _) =
            handlers::Handlers::instantiate_async(&mut store, &plugin.component, &linker).await?;

        Ok(PluginInstance {
            plugin,
            store,
            handlers,
            receive_request_body,
            receive_response_body,
            host_mut_ctx,
            stdio,
        })
    }

    /// Returns `stdout` and `stderr` captured during plugin execution.
    pub fn stdio(&self) -> PluginStdio {
        self.stdio.clone()
    }

    /// Returns whether this plugin instance expects to process a request body.
    pub fn receive_request_body(&self) -> bool {
        let receive_request_body = self.receive_request_body.lock().expect("poisoned mutex");
        *receive_request_body
    }

    /// Returns whether this plugin instance expects to process a response body.
    pub fn receive_response_body(&self) -> bool {
        let receive_response_body = self.receive_response_body.lock().expect("poisoned mutex");
        *receive_response_body
    }

    /// Returns the configured weight value for tuning [`Decision`] values.
    pub fn weight(&self) -> f64 {
        self.plugin.config.weight
    }

    /// Records a [`Request`](bulwark_wasm_sdk::Request) so that it will be accessible to the plugin guest
    /// environment. Overwrites the existing `Request`.
    pub fn record_request(&mut self, request: Arc<bulwark_wasm_sdk::Request>) {
        let mut interior_request = self.host_mut_ctx.request.lock().expect("poisoned mutex");
        *interior_request = bulwark_host::RequestInterface::from(request);
    }

    /// Records a [`Response`](bulwark_wasm_sdk::Response) so that it will be accessible to the plugin guest
    /// environment.
    pub fn record_response(&mut self, response: Arc<bulwark_wasm_sdk::Response>) {
        let mut interior_response = self.host_mut_ctx.response.lock().expect("poisoned mutex");
        *interior_response = Some(bulwark_host::ResponseInterface::from(response));
    }

    /// Records the combined [`Decision`], it's tags, and the associated [`Outcome`] so that they will be accessible
    /// to the plugin guest environment.
    pub fn record_combined_decision(
        &mut self,
        decision_components: &DecisionComponents,
        outcome: Outcome,
    ) {
        let mut interior_decision = self
            .host_mut_ctx
            .combined_decision
            .lock()
            .expect("poisoned mutex");
        *interior_decision = Some(decision_components.decision.into());
        let mut interior_outcome = self.host_mut_ctx.outcome.lock().expect("poisoned mutex");
        *interior_outcome = Some(outcome.into());
    }

    /// Returns the plugin's identifier.
    pub fn plugin_reference(&self) -> String {
        self.plugin.reference.clone()
    }

    /// Executes the guest's `init` function.
    pub async fn handle_init(&mut self) -> Result<(), PluginExecutionError> {
        let result = self
            .handlers
            .call_on_init(self.store.as_context_mut())
            .await?;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_on_init",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_on_init",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }

        Ok(())
    }

    /// Executes the guest's `on_request` function.
    pub async fn handle_request(&mut self) -> Result<(), PluginExecutionError> {
        let result = self
            .handlers
            .call_on_request(self.store.as_context_mut())
            .await?;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_on_request",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_on_request",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }

        Ok(())
    }

    /// Executes the guest's `on_request_decision` function.
    pub async fn handle_request_decision(&mut self) -> Result<(), PluginExecutionError> {
        let result = self
            .handlers
            .call_on_request_decision(self.store.as_context_mut())
            .await?;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_on_request_decision",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_on_request_decision",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }

        Ok(())
    }

    /// Executes the guest's `on_response_decision` function.
    pub async fn handle_response_decision(&mut self) -> Result<(), PluginExecutionError> {
        let result = self
            .handlers
            .call_on_response_decision(self.store.as_context_mut())
            .await?;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_on_request_body_decision",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_on_request_body_decision",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }

        Ok(())
    }

    /// Executes the guest's `on_request_body_decision` function.
    pub async fn handle_request_body_decision(&mut self) -> Result<(), PluginExecutionError> {
        let result = self
            .handlers
            .call_on_request_body_decision(self.store.as_context_mut())
            .await?;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_on_response_decision",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_on_response_decision",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }

        Ok(())
    }

    /// Executes the guest's `on_response_body_decision` function.
    pub async fn handle_response_body_decision(&mut self) -> Result<(), PluginExecutionError> {
        let result = self
            .handlers
            .call_on_response_body_decision(self.store.as_context_mut())
            .await?;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_on_response_body_decision",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_on_response_body_decision",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }

        Ok(())
    }

    /// Executes the guest's `on_decision_feedback` function.
    pub async fn handle_decision_feedback(&mut self) -> Result<(), PluginExecutionError> {
        let result = self
            .handlers
            .call_on_decision_feedback(self.store.as_context_mut())
            .await?;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_on_decision_feedback",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_on_decision_feedback",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }

        Ok(())
    }

    /// Returns the decision components from the [`RequestContext`].
    pub fn decision(&mut self) -> DecisionComponents {
        let ctx = self.store.data();

        ctx.guest_mut_ctx.decision_components.clone()
    }
}

#[async_trait]
impl bulwark_host::HostApiImports for RequestContext {
    /// Returns the guest environment's configuration value as serialized JSON.
    async fn get_config(&mut self) -> Result<Vec<u8>, wasmtime::Error> {
        Ok(self.read_only_ctx.config.to_vec())
    }

    /// Returns a named value from the request context's params.
    ///
    /// # Arguments
    ///
    /// * `key` - The key name corresponding to the param value.
    async fn get_param_value(
        &mut self,
        key: String,
    ) -> Result<Result<Vec<u8>, bulwark_host::ParamError>, wasmtime::Error> {
        let params = self.guest_mut_ctx.params.lock().expect("poisoned mutex");
        let value = params.get(&key).unwrap_or(&bulwark_wasm_sdk::Value::Null);
        match serde_json::to_vec(value) {
            Ok(bytes) => Ok(Ok(bytes)),
            Err(err) => Ok(Err(bulwark_host::ParamError::Json(err.to_string()))),
        }
    }

    /// Set a named value in the request context's params.
    ///
    /// # Arguments
    ///
    /// * `key` - The key name corresponding to the param value.
    /// * `value` - The value to record. Values are serialized JSON.
    async fn set_param_value(
        &mut self,
        key: String,
        value: Vec<u8>,
    ) -> Result<Result<(), bulwark_host::ParamError>, wasmtime::Error> {
        let mut params = self.guest_mut_ctx.params.lock().expect("poisoned mutex");
        match serde_json::from_slice(&value) {
            Ok(value) => {
                params.insert(key, value);
                Ok(Ok(()))
            }
            Err(err) => Ok(Err(bulwark_host::ParamError::Json(err.to_string()))),
        }
    }

    /// Returns a named environment variable value as bytes.
    ///
    /// # Arguments
    ///
    /// * `key` - The environment variable name. Case-sensitive.
    async fn get_env_bytes(
        &mut self,
        key: String,
    ) -> Result<Result<Vec<u8>, bulwark_host::EnvError>, wasmtime::Error> {
        let allowed_env_vars = self
            .read_only_ctx
            .permissions
            .env
            .iter()
            .cloned()
            .collect::<BTreeSet<String>>();
        if !allowed_env_vars.contains(&key) {
            return Ok(Err(bulwark_host::EnvError::Permission(key)));
        }
        match std::env::var(&key) {
            Ok(var) => Ok(Ok(var.as_bytes().to_vec())),
            Err(err) => match err {
                std::env::VarError::NotPresent => Ok(Err(bulwark_host::EnvError::Missing(key))),
                std::env::VarError::NotUnicode(_) => {
                    Ok(Err(bulwark_host::EnvError::NotUnicode(key)))
                }
            },
        }
    }

    /// Returns the incoming request associated with the request context.
    async fn get_request(&mut self) -> Result<bulwark_host::RequestInterface, wasmtime::Error> {
        let request = self.host_mut_ctx.request.lock().expect("poisoned mutex");
        Ok(request.clone())
    }

    /// Returns the response received from the interior service.
    ///
    /// If called from `on_request` or `on_request_decision`, it will return `None` since a response
    /// is not yet available.
    async fn get_response(
        &mut self,
    ) -> Result<Option<bulwark_host::ResponseInterface>, wasmtime::Error> {
        let response: MutexGuard<Option<bulwark_host::ResponseInterface>> =
            self.host_mut_ctx.response.lock().expect("poisoned mutex");
        Ok(response.to_owned())
    }

    /// Determines whether the request body will be received by the plugin in the `on_request_body_decision` handler.
    async fn receive_request_body(&mut self, body: bool) -> Result<(), wasmtime::Error> {
        let mut receive_request_body = self
            .guest_mut_ctx
            .receive_request_body
            .lock()
            .expect("poisoned mutex");
        *receive_request_body = body;
        Ok(())
    }

    /// Determines whether the response body will be received by the plugin in the `on_response_body_decision` handler.
    async fn receive_response_body(&mut self, body: bool) -> Result<(), wasmtime::Error> {
        let mut receive_response_body = self
            .guest_mut_ctx
            .receive_response_body
            .lock()
            .expect("poisoned mutex");
        *receive_response_body = body;
        Ok(())
    }

    /// Returns the originating client's IP address, if available.
    async fn get_client_ip(
        &mut self,
    ) -> Result<Option<bulwark_host::IpInterface>, wasmtime::Error> {
        Ok(self.read_only_ctx.client_ip)
    }

    /// Begins an outbound request. Returns a request ID used by `add_request_header` and `set_request_body`.
    ///
    /// # Arguments
    ///
    /// * `method` - The HTTP method
    /// * `uri` - The absolute URI of the resource to request
    async fn send_request(
        &mut self,
        request: bulwark_host::RequestInterface,
    ) -> Result<Result<bulwark_host::ResponseInterface, bulwark_host::HttpError>, wasmtime::Error>
    {
        Ok(
            // Inner function to permit ? operator
            || -> Result<bulwark_host::ResponseInterface, bulwark_host::HttpError> {
                verify_http_domains(&self.read_only_ctx.permissions.http, &request.uri)?;

                let method = reqwest::Method::from_str(&request.method)
                    .map_err(|_| bulwark_host::HttpError::InvalidMethod(request.method.clone()))?;

                let mut builder = self.read_only_ctx.http_client.request(method, &request.uri);
                for (name, value) in request.headers {
                    builder = builder.header(name, value);
                }

                if !request.end_of_stream {
                    return Err(bulwark_host::HttpError::UnavailableContent(
                        "the entire request body must be available".to_string(),
                    ));
                } else if request.chunk_start != 0 {
                    return Err(bulwark_host::HttpError::InvalidStart(
                        "chunk start must be 0".to_string(),
                    ));
                } else if request.chunk_length > 16384 {
                    return Err(bulwark_host::HttpError::ContentTooLarge(
                        "the entire request body must be 16384 bytes or less".to_string(),
                    ));
                }

                builder = builder.body(request.chunk);

                let response = builder
                    .send()
                    .map_err(|err| bulwark_host::HttpError::Transmit(err.to_string()))?;
                let status: u32 = response.status().as_u16() as u32;
                // need to read headers before body because retrieving body bytes will move the response
                let headers: Vec<(String, Vec<u8>)> = response
                    .headers()
                    .iter()
                    .map(|(name, value)| (name.to_string(), value.as_bytes().to_vec()))
                    .collect();
                let body = response.bytes().unwrap().to_vec();
                let content_length: u64 = body.len() as u64;
                Ok(bulwark_host::ResponseInterface {
                    status,
                    headers,
                    body_received: true,
                    chunk: body,
                    chunk_start: 0,
                    chunk_length: content_length,
                    end_of_stream: true,
                })
            }(),
        )
    }

    /// Records the decision value the plugin wants to return.
    ///
    /// # Arguments
    ///
    /// * `decision` - The [`Decision`] output of the plugin.
    async fn set_decision(
        &mut self,
        decision: bulwark_host::DecisionInterface,
    ) -> Result<Result<(), bulwark_host::DecisionError>, wasmtime::Error> {
        let decision = Decision::from(decision);
        // Validate on both the guest and the host because we can't guarantee usage of the SDK.
        match decision.validate() {
            Ok(_) => {
                self.guest_mut_ctx.decision_components.decision = decision;
                Ok(Ok(()))
            }
            Err(err) => Ok(Err(bulwark_host::DecisionError::Invalid(err.to_string()))),
        }
    }

    /// Records the tags the plugin wants to associate with its decision.
    ///
    /// # Arguments
    ///
    /// * `tags` - The list of tags to associate with a [`Decision`].
    async fn set_tags(&mut self, tags: Vec<String>) -> Result<(), wasmtime::Error> {
        self.guest_mut_ctx.decision_components.tags = tags;
        Ok(())
    }

    /// Records additional tags the plugin wants to associate with its decision. Existing tags will be kept.
    ///
    /// # Arguments
    ///
    /// * `tags` - The list of tags to associate with a [`Decision`].
    async fn append_tags(&mut self, mut tags: Vec<String>) -> Result<Vec<String>, wasmtime::Error> {
        self.guest_mut_ctx
            .decision_components
            .tags
            .append(&mut tags);
        Ok(self.guest_mut_ctx.decision_components.tags.clone())
    }

    /// Returns the combined decision, if available.
    ///
    /// Typically used in the feedback phase.
    async fn get_combined_decision(
        &mut self,
    ) -> Result<Option<bulwark_host::DecisionInterface>, wasmtime::Error> {
        let combined_decision: MutexGuard<Option<bulwark_host::DecisionInterface>> = self
            .host_mut_ctx
            .combined_decision
            .lock()
            .expect("poisoned mutex");
        Ok(combined_decision.to_owned())
    }

    /// Returns the combined set of tags associated with a decision, if available.
    ///
    /// Typically used in the feedback phase.
    async fn get_combined_tags(&mut self) -> Result<Option<Vec<String>>, wasmtime::Error> {
        let combined_tags: MutexGuard<Option<Vec<String>>> = self
            .host_mut_ctx
            .combined_tags
            .lock()
            .expect("poisoned mutex");
        Ok(combined_tags.to_owned())
    }

    /// Returns the outcome of the combined decision, if available.
    ///
    /// Typically used in the feedback phase.
    async fn get_outcome(
        &mut self,
    ) -> Result<Option<bulwark_host::OutcomeInterface>, wasmtime::Error> {
        let outcome: MutexGuard<Option<bulwark_host::OutcomeInterface>> =
            self.host_mut_ctx.outcome.lock().expect("poisoned mutex");
        Ok(outcome.to_owned())
    }

    /// Returns the named state value retrieved from Redis.
    ///
    /// Also used to retrieve a counter value.
    ///
    /// # Arguments
    ///
    /// * `key` - The key name corresponding to the state value.
    async fn get_remote_state(
        &mut self,
        key: String,
    ) -> Result<Result<Vec<u8>, bulwark_host::StateError>, wasmtime::Error> {
        Ok(
            // Inner function to permit ? operator
            || -> Result<Vec<u8>, bulwark_host::StateError> {
                verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

                if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
                    let mut conn = redis_info
                        .pool
                        .get()
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;

                    Ok(conn
                        .get(key)
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?)
                } else {
                    Err(bulwark_host::StateError::Remote(
                        "no remote state configured".to_string(),
                    ))
                }
            }(),
        )
    }

    /// Set a named value in Redis.
    ///
    /// # Arguments
    ///
    /// * `key` - The key name corresponding to the state value.
    /// * `value` - The value to record. Values are byte strings, but may be interpreted differently by Redis depending on context.
    async fn set_remote_state(
        &mut self,
        key: String,
        value: Vec<u8>,
    ) -> Result<Result<(), bulwark_host::StateError>, wasmtime::Error> {
        Ok(
            // Inner function to permit ? operator
            || -> Result<(), bulwark_host::StateError> {
                verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

                if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
                    let mut conn = redis_info
                        .pool
                        .get()
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;

                    conn.set::<String, Vec<u8>, redis::Value>(key, value)
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
                    Ok(())
                } else {
                    Err(bulwark_host::StateError::Remote(
                        "no remote state configured".to_string(),
                    ))
                }
            }(),
        )
    }

    /// Increments a named counter in Redis.
    ///
    /// # Arguments
    ///
    /// * `key` - The key name corresponding to the state counter.
    async fn increment_remote_state(
        &mut self,
        key: String,
    ) -> Result<Result<i64, bulwark_host::StateError>, wasmtime::Error> {
        self.increment_remote_state_by(key, 1).await
    }

    /// Increments a named counter in Redis by a specified delta value.
    ///
    /// # Arguments
    ///
    /// * `key` - The key name corresponding to the state counter.
    /// * `delta` - The amount to increase the counter by.
    async fn increment_remote_state_by(
        &mut self,
        key: String,
        delta: i64,
    ) -> Result<Result<i64, bulwark_host::StateError>, wasmtime::Error> {
        Ok(
            // Inner function to permit ? operator
            || -> Result<i64, bulwark_host::StateError> {
                verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

                if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
                    let mut conn = redis_info
                        .pool
                        .get()
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;

                    Ok(conn
                        .incr(key, delta)
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?)
                } else {
                    Err(bulwark_host::StateError::Remote(
                        "no remote state configured".to_string(),
                    ))
                }
            }(),
        )
    }

    /// Sets an expiration on a named value in Redis.
    ///
    /// # Arguments
    ///
    /// * `key` - The key name corresponding to the state value.
    /// * `ttl` - The time-to-live for the value in seconds.
    async fn set_remote_ttl(
        &mut self,
        key: String,
        ttl: i64,
    ) -> Result<Result<(), bulwark_host::StateError>, wasmtime::Error> {
        Ok(
            // Inner function to permit ? operator
            || -> Result<(), bulwark_host::StateError> {
                verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

                if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
                    let mut conn = redis_info
                        .pool
                        .get()
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;

                    conn.expire::<String, redis::Value>(key, ttl as usize)
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
                    Ok(())
                } else {
                    Err(bulwark_host::StateError::Remote(
                        "no remote state configured".to_string(),
                    ))
                }
            }(),
        )
    }

    /// Increments a rate limit, returning the number of attempts so far and the expiration time.
    ///
    /// The rate limiter is a counter over a period of time. At the end of the period, it will expire,
    /// beginning a new period. Window periods should be set to the longest amount of time that a client should
    /// be locked out for. The plugin is responsible for performing all rate-limiting logic with the counter
    /// value it receives.
    ///
    /// # Arguments
    ///
    /// * `key` - The key name corresponding to the state counter.
    /// * `delta` - The amount to increase the counter by.
    /// * `window` - How long each period should be in seconds.
    async fn increment_rate_limit(
        &mut self,
        key: String,
        delta: i64,
        window: i64,
    ) -> Result<Result<bulwark_host::RateInterface, bulwark_host::StateError>, wasmtime::Error>
    {
        Ok(
            // Inner function to permit ? operator
            || -> Result<bulwark_host::RateInterface, bulwark_host::StateError> {
                verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

                if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
                    let mut conn = redis_info
                        .pool
                        .get()
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
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
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
                    Ok(bulwark_host::RateInterface {
                        attempts,
                        expiration,
                    })
                } else {
                    Err(bulwark_host::StateError::Remote(
                        "no remote state configured".to_string(),
                    ))
                }
            }(),
        )
    }

    /// Checks a rate limit, returning the number of attempts so far and the expiration time.
    ///
    /// See `increment_rate_limit`.
    ///
    /// # Arguments
    ///
    /// * `key` - The key name corresponding to the state counter.
    async fn check_rate_limit(
        &mut self,
        key: String,
    ) -> Result<Result<bulwark_host::RateInterface, bulwark_host::StateError>, wasmtime::Error>
    {
        Ok(
            // Inner function to permit ? operator
            || -> Result<bulwark_host::RateInterface, bulwark_host::StateError> {
                verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

                if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
                    let mut conn = redis_info
                        .pool
                        .get()
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
                    let dt = Utc::now();
                    let timestamp: i64 = dt.timestamp();
                    let script = redis_info.registry.check_rate_limit.clone();
                    // Invoke the script and map to our rate type
                    let (attempts, expiration) = script
                        .key(key)
                        .arg(timestamp)
                        .invoke::<(i64, i64)>(conn.deref_mut())
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
                    Ok(bulwark_host::RateInterface {
                        attempts,
                        expiration,
                    })
                } else {
                    Err(bulwark_host::StateError::Remote(
                        "no remote state configured".to_string(),
                    ))
                }
            }(),
        )
    }

    /// Increments a circuit breaker, returning the generation count, success count, failure count,
    /// consecutive success count, consecutive failure count, and expiration time.
    ///
    /// The plugin is responsible for performing all circuit-breaking logic with the counter
    /// values it receives. The host environment does as little as possible to maximize how much
    /// control the plugin has over the behavior of the breaker.
    ///
    /// # Arguments
    ///
    /// * `key` - The key name corresponding to the state counter.
    /// * `success_delta` - The amount to increase the success counter by. Generally zero on failure.
    /// * `failure_delta` - The amount to increase the failure counter by. Generally zero on success.
    /// * `window` - How long each period should be in seconds.
    async fn increment_breaker(
        &mut self,
        key: String,
        success_delta: i64,
        failure_delta: i64,
        window: i64,
    ) -> Result<Result<bulwark_host::BreakerInterface, bulwark_host::StateError>, wasmtime::Error>
    {
        Ok(
            // Inner function to permit ? operator
            || -> Result<bulwark_host::BreakerInterface, bulwark_host::StateError> {
                verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

                if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
                    let mut conn = redis_info
                        .pool
                        .get()
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
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
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
                    Ok(bulwark_host::BreakerInterface {
                        generation,
                        successes,
                        failures,
                        consecutive_successes,
                        consecutive_failures,
                        expiration,
                    })
                } else {
                    Err(bulwark_host::StateError::Remote(
                        "no remote state configured".to_string(),
                    ))
                }
            }(),
        )
    }

    /// Checks a circuit breaker, returning the generation count, success count, failure count,
    /// consecutive success count, consecutive failure count, and expiration time.
    ///
    /// See `increment_breaker`.
    ///
    /// # Arguments
    ///
    /// * `key` - The key name corresponding to the state counter.
    async fn check_breaker(
        &mut self,
        key: String,
    ) -> Result<Result<bulwark_host::BreakerInterface, bulwark_host::StateError>, wasmtime::Error>
    {
        Ok(
            // Inner function to permit ? operator
            || -> Result<bulwark_host::BreakerInterface, bulwark_host::StateError> {
                verify_remote_state_prefixes(&self.read_only_ctx.permissions.state, &key)?;

                if let Some(redis_info) = self.read_only_ctx.redis_info.clone() {
                    let mut conn = redis_info
                        .pool
                        .get()
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
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
                        .map_err(|err| bulwark_host::StateError::Remote(err.to_string()))?;
                    Ok(bulwark_host::BreakerInterface {
                        generation,
                        successes,
                        failures,
                        consecutive_successes,
                        consecutive_failures,
                        expiration,
                    })
                } else {
                    Err(bulwark_host::StateError::Remote(
                        "no remote state configured".to_string(),
                    ))
                }
            }(),
        )
    }
}

/// Ensures that access to any HTTP host has the appropriate permissions set.
fn verify_http_domains(
    // TODO: BTreeSet<String> instead, all the way up
    allowed_http_domains: &[String],
    uri: &str,
) -> Result<(), bulwark_host::HttpError> {
    let parsed_uri =
        Url::parse(uri).map_err(|_| bulwark_host::HttpError::InvalidUri(uri.to_string()))?;
    let requested_domain = parsed_uri
        .domain()
        .ok_or_else(|| bulwark_host::HttpError::InvalidUri(uri.to_string()))?;
    if !allowed_http_domains.contains(&requested_domain.to_string()) {
        return Err(bulwark_host::HttpError::Permission(uri.to_string()));
    }
    Ok(())
}

/// Ensures that access to any remote state key has the appropriate permissions set.
fn verify_remote_state_prefixes(
    // TODO: BTreeSet<String> instead, all the way up
    allowed_key_prefixes: &[String],
    key: &str,
) -> Result<(), bulwark_host::StateError> {
    let key = key.to_string();
    if !allowed_key_prefixes
        .iter()
        .any(|prefix| key.starts_with(prefix))
    {
        return Err(bulwark_host::StateError::Permission(key));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapt_wasm_output(
        wasm_bytes: Vec<u8>,
        adapter_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let component = wit_component::ComponentEncoder::default()
            .module(&wasm_bytes)?
            .validate(true)
            .adapter("wasi_snapshot_preview1", &adapter_bytes)?
            .encode()?;

        Ok(component.to_vec())
    }

    #[test]
    fn test_wasm_execution() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../tests/bulwark_blank_slate.wasm");
        let adapter_bytes = include_bytes!("../tests/wasi_snapshot_preview1.reactor.wasm");
        let adapted_component = adapt_wasm_output(wasm_bytes.to_vec(), adapter_bytes.to_vec())?;
        let plugin = Arc::new(Plugin::from_bytes(
            "bulwark-blank-slate.wasm".to_string(),
            &adapted_component,
            &bulwark_config::Plugin::default(),
        )?);
        let request = Arc::new(
            http::Request::builder()
                .method("GET")
                .uri("/")
                .version(http::Version::HTTP_11)
                .body(bulwark_wasm_sdk::NO_BODY)?,
        );
        let params = Arc::new(Mutex::new(bulwark_wasm_sdk::Map::new()));
        let request_context = RequestContext::new(
            plugin.clone(),
            None,
            Arc::new(reqwest::blocking::Client::new()),
            params,
            request,
        )?;
        let mut plugin_instance =
            tokio_test::block_on(PluginInstance::new(plugin, request_context))?;
        let decision_components = plugin_instance.decision();
        assert_eq!(decision_components.decision.accept, 0.0);
        assert_eq!(decision_components.decision.restrict, 0.0);
        assert_eq!(decision_components.decision.unknown, 1.0);
        assert_eq!(decision_components.tags, vec![""; 0]);

        Ok(())
    }

    #[test]
    fn test_wasm_logic() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../tests/bulwark_evil_bit.wasm");
        let adapter_bytes = include_bytes!("../tests/wasi_snapshot_preview1.reactor.wasm");
        let adapted_component = adapt_wasm_output(wasm_bytes.to_vec(), adapter_bytes.to_vec())?;
        let plugin = Arc::new(Plugin::from_bytes(
            "bulwark-evil-bit.wasm".to_string(),
            &adapted_component,
            &bulwark_config::Plugin::default(),
        )?);

        let request = Arc::new(
            http::Request::builder()
                .method("POST")
                .uri("/example")
                .version(http::Version::HTTP_11)
                .header("Content-Type", "application/json")
                .body(bulwark_wasm_sdk::UNAVAILABLE_BODY)?,
        );
        let params = Arc::new(Mutex::new(bulwark_wasm_sdk::Map::new()));
        let request_context = RequestContext::new(
            plugin.clone(),
            None,
            Arc::new(reqwest::blocking::Client::new()),
            params,
            request,
        )?;
        let mut typical_plugin_instance =
            tokio_test::block_on(PluginInstance::new(plugin.clone(), request_context))?;
        tokio_test::block_on(typical_plugin_instance.handle_request_decision())?;
        let typical_decision = typical_plugin_instance.decision();
        assert_eq!(typical_decision.decision.accept, 0.0);
        assert_eq!(typical_decision.decision.restrict, 0.0);
        assert_eq!(typical_decision.decision.unknown, 1.0);
        assert_eq!(typical_decision.tags, vec![""; 0]);

        let request = Arc::new(
            http::Request::builder()
                .method("POST")
                .uri("/example")
                .version(http::Version::HTTP_11)
                .header("Content-Type", "application/json")
                .header("Evil", "true")
                .body(bulwark_wasm_sdk::UNAVAILABLE_BODY)?,
        );
        let params = Arc::new(Mutex::new(bulwark_wasm_sdk::Map::new()));
        let request_context = RequestContext::new(
            plugin.clone(),
            None,
            Arc::new(reqwest::blocking::Client::new()),
            params,
            request,
        )?;
        let mut evil_plugin_instance =
            tokio_test::block_on(PluginInstance::new(plugin, request_context))?;
        tokio_test::block_on(evil_plugin_instance.handle_request_decision())?;
        let evil_decision = evil_plugin_instance.decision();
        assert_eq!(evil_decision.decision.accept, 0.0);
        assert_eq!(evil_decision.decision.restrict, 1.0);
        assert_eq!(evil_decision.decision.unknown, 0.0);
        assert_eq!(evil_decision.tags, vec!["evil"]);

        Ok(())
    }
}
