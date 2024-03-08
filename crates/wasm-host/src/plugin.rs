use anyhow::Context as _;
use wasmtime_wasi_http::body::HyperIncomingBody;

mod latest {
    pub mod http {
        pub use wasmtime_wasi_http::bindings::wasi::http::*;
    }
}

#[doc(hidden)]
pub(crate) mod bindings {
    wasmtime::component::bindgen!({
        world: "bulwark:plugin/http-detection",
        async: true,
        with: {
            "wasi:http/types/incoming-response": super::latest::http::types::IncomingResponse,
            "wasi:http/types/incoming-request": super::latest::http::types::IncomingRequest,
            "wasi:http/types/incoming-body": super::latest::http::types::IncomingBody,
            "wasi:http/types/outgoing-response": super::latest::http::types::OutgoingResponse,
            "wasi:http/types/outgoing-request": super::latest::http::types::OutgoingRequest,
            "wasi:http/types/outgoing-body": super::latest::http::types::OutgoingBody,
            "wasi:http/types/fields": super::latest::http::types::Fields,
            "wasi:http/types/response-outparam": super::latest::http::types::ResponseOutparam,
            "wasi:http/types/future-incoming-response": super::latest::http::types::FutureIncomingResponse,
            "wasi:http/types/future-trailers": super::latest::http::types::FutureTrailers,
        }
    });
}

use {
    crate::PluginContext,
    crate::{PluginExecutionError, PluginInstantiationError, PluginLoadError},
    bulwark_wasm_sdk::Decision,
    http_body_util::{combinators::BoxBody, BodyExt, Empty, Full},
    std::{
        collections::{HashMap, HashSet},
        net::IpAddr,
        path::Path,
        sync::Arc,
    },
    wasmtime::component::{Component, Linker},
    wasmtime::{AsContextMut, Config, Engine, Store},
    wasmtime_wasi::preview2::{pipe::MemoryOutputPipe, HostOutputStream, StdoutStream},
    wasmtime_wasi_http::WasiHttpView,
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

/// The primary output of a [`PluginInstance`]'s execution. Combines a [`Decision`] and a list of tags together.
///
/// Both the output of individual plugins as well as the combined decision output of a group of plugins may be
/// represented by `HandlerOutput`. The latter is the result of applying Dempster-Shafer combination to each
/// `decision` value in a [`HandlerOutput`] list and then taking the union set of all `tags` lists and forming
/// a new [`HandlerOutput`] with both results.
#[derive(Clone, Default)]
pub struct HandlerOutput {
    /// A `Decision` made by a plugin or a group of plugins
    pub decision: Decision,
    /// The tags applied by plugins to annotate a [`Decision`]
    pub tags: HashSet<String>,
    /// The parameters applied by plugins to enrich the request.
    pub params: HashMap<String, String>,
}

impl HandlerOutput {
    // Extends the parameters with the provided `params`.
    //
    // See [`HashMap::extend`] for more information.
    pub fn extend_params(mut self, params: HashMap<String, String>) -> Self {
        self.params.extend(params);
        self
    }
}

/// A singular detection plugin and provides the interface between WASM host and guest.
///
/// One `Plugin` may spawn many [`PluginInstance`]s, which will handle the incoming request data.
#[derive(Clone)]
pub struct Plugin {
    reference: String,
    host_config: Arc<bulwark_config::Config>,
    guest_config: Arc<bulwark_config::Plugin>,
    engine: Engine,
    component: Component,
}

impl Plugin {
    /// Creates and compiles a new [`Plugin`] from a [`String`] of
    /// [WAT](https://webassembly.github.io/spec/core/text/index.html)-formatted WASM.
    pub fn from_wat(
        name: String,
        wat: &str,
        host_config: &bulwark_config::Config,
        guest_config: &bulwark_config::Plugin,
    ) -> Result<Self, PluginLoadError> {
        Self::from_component(
            name,
            host_config,
            guest_config,
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
        host_config: &bulwark_config::Config,
        guest_config: &bulwark_config::Plugin,
    ) -> Result<Self, PluginLoadError> {
        Self::from_component(
            name,
            host_config,
            guest_config,
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
        host_config: &bulwark_config::Config,
        guest_config: &bulwark_config::Plugin,
    ) -> Result<Self, PluginLoadError> {
        let name = guest_config.reference.clone();
        Self::from_component(
            name,
            host_config,
            guest_config,
            |engine| -> Result<Component, PluginLoadError> {
                Ok(Component::from_file(engine, &path)?)
            },
        )
    }

    /// Helper method for the other `from_*` functions.
    fn from_component<F>(
        reference: String,
        host_config: &bulwark_config::Config,
        guest_config: &bulwark_config::Plugin,
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
            host_config: Arc::new(host_config.clone()),
            guest_config: Arc::new(guest_config.clone()),
            engine,
            component,
        })
    }

    /// Makes the host's configuration available to host functions.
    pub(crate) fn host_config(&self) -> &bulwark_config::Config {
        &self.host_config
    }

    /// Makes the guest's configuration available to the guest environment.
    pub(crate) fn guest_config(&self) -> &bulwark_wasm_sdk::Map<String, bulwark_wasm_sdk::Value> {
        &self.guest_config.config
    }

    /// Makes the permissions the plugin has been granted available to the guest environment.
    pub fn permissions(&self) -> &bulwark_config::Permissions {
        &self.guest_config.permissions
    }
}

/// Allows the host to capture plugin standard IO and record it to the log.
#[derive(Clone)]
pub(crate) struct BufStdoutStream(MemoryOutputPipe);

impl BufStdoutStream {
    pub fn contents(&self) -> bytes::Bytes {
        self.0.contents()
    }

    pub(crate) fn writer(&self) -> impl HostOutputStream {
        self.0.clone()
    }
}

impl Default for BufStdoutStream {
    fn default() -> Self {
        Self(MemoryOutputPipe::new(usize::MAX))
    }
}

impl StdoutStream for BufStdoutStream {
    fn stream(&self) -> Box<dyn HostOutputStream> {
        Box::new(self.writer())
    }

    fn isatty(&self) -> bool {
        false
    }
}

/// Wraps buffers to capture plugin stdio.
#[derive(Clone, Default)]
pub struct PluginStdio {
    pub(crate) stdout: BufStdoutStream,
    pub(crate) stderr: BufStdoutStream,
}

impl PluginStdio {
    pub fn stdout_buffer(&self) -> Vec<u8> {
        self.stdout.contents().to_vec()
    }

    pub fn stderr_buffer(&self) -> Vec<u8> {
        self.stderr.contents().to_vec()
    }
}

/// An instance of a [`Plugin`], associated with a [`RequestContext`].
pub struct PluginInstance {
    /// A reference to the parent `Plugin` and its configuration.
    plugin: Arc<Plugin>,
    /// The WASM store that holds state associated with the incoming request.
    store: Store<PluginContext>,
    /// The HTTP detection world from the WIT interface.
    http_detection: bindings::HttpDetection,
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
        plugin_context: PluginContext,
    ) -> Result<PluginInstance, PluginInstantiationError> {
        // Clone the stdio so we can read the captured stdout and stderr buffers after execution has completed.
        let stdio = plugin_context.stdio.clone();

        let mut linker: Linker<PluginContext> = Linker::new(&plugin.engine);
        let mut store = Store::new(&plugin.engine, plugin_context);

        wasmtime_wasi::preview2::command::add_to_linker(&mut linker)?;
        wasmtime_wasi_http::bindings::wasi::http::types::add_to_linker(&mut linker, |ctx| ctx)
            .context("failed to link `wasi:http/types` interface")?;
        wasmtime_wasi_http::bindings::wasi::http::outgoing_handler::add_to_linker(
            &mut linker,
            |ctx| ctx,
        )
        .context("failed to link `wasi:http/outgoing-handler` interface")?;
        bindings::bulwark::plugin::config::add_to_linker(&mut linker, |t| t)
            .context("failed to link `bulwark:plugin/config` interface")?;
        bindings::bulwark::plugin::redis::add_to_linker(&mut linker, |t| t)
            .context("failed to link `bulwark:plugin/redis` interface")?;
        bindings::bulwark::plugin::types::add_to_linker(&mut linker, |t| t)
            .context("failed to link `bulwark:plugin/types` interface")?;

        // We discard the instance for this because we only use the generated interface to make calls

        let (http_detection, _) =
            bindings::HttpDetection::instantiate_async(&mut store, &plugin.component, &linker)
                .await?;

        Ok(PluginInstance {
            plugin,
            store,
            http_detection,
            stdio,
        })
    }

    /// Returns `stdout` and `stderr` captured during plugin execution.
    pub fn stdio(&self) -> PluginStdio {
        self.stdio.clone()
    }

    /// Returns the configured weight value for tuning [`Decision`] values.
    pub fn weight(&self) -> f64 {
        self.plugin.guest_config.weight
    }

    /// Returns the plugin's identifier.
    pub fn plugin_reference(&self) -> String {
        self.plugin.reference.clone()
    }

    /// Executes the guest's `init` function.
    pub async fn handle_init(&mut self) -> Result<(), PluginExecutionError> {
        let result = self
            .http_detection
            .bulwark_plugin_http_handlers()
            .call_handle_init(self.store.as_context_mut())
            .await;
        match result {
            Ok(Ok(_)) => metrics::increment_counter!(
                "plugin_on_init",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Ok(Err(_)) | Err(_) => metrics::increment_counter!(
                "plugin_on_init",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }

        // Initialization doesn't return anything unless there's an error
        result??;
        Ok(())
    }

    /// Executes the guest's `on_request` function.
    pub async fn handle_request_enrichment(
        &mut self,
        request: Arc<bulwark_wasm_sdk::Request>,
        params: HashMap<String, String>,
    ) -> Result<HashMap<String, String>, PluginExecutionError> {
        let incoming_request: http::Request<HyperIncomingBody> = (*request).clone().map(|body| {
            if !body.is_empty() {
                // Body is already read into a large buffer
                BoxBody::new(Full::new(body).map_err(|_| unreachable!()))
            } else {
                BoxBody::new(Empty::new().map_err(|_| unreachable!()))
            }
        });
        let incoming_request_handle = self
            .store
            .as_context_mut()
            .data_mut()
            .new_incoming_request(incoming_request)?;

        // TODO: need to determine if automatic calls to remove_forbidden_headers are going to be a problem
        let params: Vec<(String, String)> = params.into_iter().collect();
        let result = self
            .http_detection
            .bulwark_plugin_http_handlers()
            .call_handle_request_enrichment(
                self.store.as_context_mut(),
                incoming_request_handle,
                params.as_slice(),
            )
            .await;
        match result {
            Ok(Ok(_)) => metrics::increment_counter!(
                "plugin_on_request",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Ok(Err(_)) | Err(_) => metrics::increment_counter!(
                "plugin_on_request",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }
        let params: HashMap<String, String> = result??.into_iter().collect();

        Ok(params)
    }

    /// Executes the guest's `on_request_decision` function.
    pub async fn handle_request_decision(
        &mut self,
        request: Arc<bulwark_wasm_sdk::Request>,
        params: HashMap<String, String>,
    ) -> Result<HandlerOutput, PluginExecutionError> {
        let incoming_request: http::Request<HyperIncomingBody> = (*request).clone().map(|body| {
            if !body.is_empty() {
                // Body is already read into a large buffer
                BoxBody::new(Full::new(body).map_err(|_| unreachable!()))
            } else {
                BoxBody::new(Empty::new().map_err(|_| unreachable!()))
            }
        });
        let incoming_request_handle = self
            .store
            .as_context_mut()
            .data_mut()
            .new_incoming_request(incoming_request)?;

        let params: Vec<(String, String)> = params.into_iter().collect();
        let result = self
            .http_detection
            .bulwark_plugin_http_handlers()
            .call_handle_request_decision(
                self.store.as_context_mut(),
                incoming_request_handle,
                params.as_slice(),
            )
            .await;
        match result {
            Ok(Ok(_)) => metrics::increment_counter!(
                "plugin_on_request_decision",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Ok(Err(_)) | Err(_) => metrics::increment_counter!(
                "plugin_on_request_decision",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }

        Ok(result??.into())
    }

    /// Executes the guest's `on_response_decision` function.
    pub async fn handle_response_decision(
        &mut self,
        request: Arc<bulwark_wasm_sdk::Request>,
        response: Arc<bulwark_wasm_sdk::Response>,
        params: HashMap<String, String>,
    ) -> Result<HandlerOutput, PluginExecutionError> {
        let incoming_request: http::Request<HyperIncomingBody> = (*request).clone().map(|body| {
            if !body.is_empty() {
                // Body is already read into a large buffer
                BoxBody::new(Full::new(body).map_err(|_| unreachable!()))
            } else {
                BoxBody::new(Empty::new().map_err(|_| unreachable!()))
            }
        });
        let incoming_request_handle = self
            .store
            .as_context_mut()
            .data_mut()
            .new_incoming_request(incoming_request)?;

        let (parts, body) = (*response).clone().into_parts();

        let incoming_response = wasmtime_wasi_http::types::HostIncomingResponse {
            status: parts.status.as_u16(),
            headers: parts.headers,
            body: Some(wasmtime_wasi_http::body::HostIncomingBody::new(
                match !body.is_empty() {
                    // Body is already read into a large buffer
                    true => BoxBody::new(Full::new(body).map_err(|_| unreachable!())),
                    false => BoxBody::new(Empty::new().map_err(|_| unreachable!())),
                },
                std::time::Duration::from_millis(600 * 1000),
            )),
            // No-op worker
            worker: Arc::new(wasmtime_wasi::preview2::spawn(async {})),
        };
        let incoming_response_handle = self
            .store
            .as_context_mut()
            .data_mut()
            .new_incoming_response(incoming_response)?;

        let params: Vec<(String, String)> = params.into_iter().collect();
        let result = self
            .http_detection
            .bulwark_plugin_http_handlers()
            .call_handle_response_decision(
                self.store.as_context_mut(),
                incoming_request_handle,
                incoming_response_handle,
                params.as_slice(),
            )
            .await;
        match result {
            Ok(Ok(_)) => metrics::increment_counter!(
                "plugin_on_request_body_decision",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Ok(Err(_)) | Err(_) => metrics::increment_counter!(
                "plugin_on_request_body_decision",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }

        Ok(result??.into())
    }

    /// Executes the guest's `on_decision_feedback` function.
    pub async fn handle_decision_feedback(
        &mut self,
        request: Arc<bulwark_wasm_sdk::Request>,
        response: Arc<bulwark_wasm_sdk::Response>,
        params: HashMap<String, String>,
        verdict: bulwark_wasm_sdk::Verdict,
    ) -> Result<(), PluginExecutionError> {
        let incoming_request: http::Request<HyperIncomingBody> = (*request).clone().map(|body| {
            if !body.is_empty() {
                BoxBody::new(Full::new(body).map_err(|_| unreachable!()))
            } else {
                BoxBody::new(Empty::new().map_err(|_| unreachable!()))
            }
        });
        let incoming_request_handle = self
            .store
            .as_context_mut()
            .data_mut()
            .new_incoming_request(incoming_request)?;

        let (parts, body) = (*response).clone().into_parts();

        let incoming_response = wasmtime_wasi_http::types::HostIncomingResponse {
            status: parts.status.as_u16(),
            headers: parts.headers,
            body: Some(wasmtime_wasi_http::body::HostIncomingBody::new(
                match !body.is_empty() {
                    // Body is already read into a large buffer
                    true => BoxBody::new(Full::new(body).map_err(|_| unreachable!())),
                    false => BoxBody::new(Empty::new().map_err(|_| unreachable!())),
                },
                std::time::Duration::from_millis(600 * 1000),
            )),
            // No-op worker
            worker: Arc::new(wasmtime_wasi::preview2::spawn(async {})),
        };
        let incoming_response_handle = self
            .store
            .as_context_mut()
            .data_mut()
            .new_incoming_response(incoming_response)?;

        let params: Vec<(String, String)> = params.into_iter().collect();
        let result = self
            .http_detection
            .bulwark_plugin_http_handlers()
            .call_handle_decision_feedback(
                self.store.as_context_mut(),
                incoming_request_handle,
                incoming_response_handle,
                params.as_slice(),
                &verdict.into(),
            )
            .await;
        match result {
            Ok(Ok(_)) => metrics::increment_counter!(
                "plugin_on_decision_feedback",
                "ref" => self.plugin_reference(), "result" => "ok"
            ),
            Ok(Err(_)) | Err(_) => metrics::increment_counter!(
                "plugin_on_decision_feedback",
                "ref" => self.plugin_reference(), "result" => "error"
            ),
        }

        // Decision feedback doesn't return anything unless there's an error
        result??;
        Ok(())
    }
}
