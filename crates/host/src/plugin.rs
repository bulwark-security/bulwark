use crate::PluginCtx;
use crate::{PluginExecutionError, PluginInstantiationError, PluginLoadError};
use anyhow::Context as _;
use bulwark_config::{PluginAccess, PluginVerification};
use bulwark_sdk::Decision;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use secrecy::ExposeSecret;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use wasmtime::component::{Component, Linker};
use wasmtime::{AsContextMut, Config, Engine, Store};
use wasmtime_wasi::{pipe::MemoryOutputPipe, HostOutputStream, StdoutStream};
use wasmtime_wasi_http::body::HyperIncomingBody;
use wasmtime_wasi_http::{WasiHttpImpl, WasiHttpView};

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
    /// The labels applied by plugins to enrich the request.
    pub labels: HashMap<String, String>,
}

impl HandlerOutput {
    /// Extends the labels with the provided ones.
    ///
    /// See [`HashMap::extend`] for more information.
    pub fn extend_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels.extend(labels);
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

    /// Creates and compiles a new [`Plugin`] from configuration.
    ///
    /// See [`bulwark_config::Plugin`].
    pub fn from_config(
        host_config: &bulwark_config::Config,
        guest_config: &bulwark_config::Plugin,
    ) -> Result<Self, PluginLoadError> {
        Self::from_component(
            guest_config.reference.clone(),
            host_config,
            guest_config,
            |engine| -> Result<Component, PluginLoadError> {
                match &guest_config.location {
                    bulwark_config::PluginLocation::Local(path) => {
                        Ok(Component::from_file(engine, path)?)
                    }
                    bulwark_config::PluginLocation::Remote(uri) => {
                        let client = reqwest::blocking::Client::new();
                        let mut request = client.get(uri.clone());
                        if let PluginAccess::Header(authorization_secret) = &guest_config.access {
                            let secret =
                                host_config.secret(authorization_secret).ok_or_else(|| {
                                    PluginLoadError::SecretMissing(authorization_secret.clone())
                                })?;
                            // In this case, secrecy::Secret might be overkill, because we immediately discard the value,
                            // but it's probably a good habit to be using it anytime we touch a secret.
                            let authorization_value = match &secret.location {
                                bulwark_config::SecretLocation::EnvVar(env_var) => {
                                    std::env::var_os(env_var)
                                        .map(|value| {
                                            secrecy::Secret::from(
                                                value.to_string_lossy().to_string(),
                                            )
                                        })
                                        .ok_or(PluginLoadError::SecretMissing(
                                            secret.reference.clone(),
                                        ))?
                                }
                                bulwark_config::SecretLocation::File(path) => {
                                    secrecy::Secret::from(
                                        std::fs::read(path)
                                            .map(|value| {
                                                String::from_utf8_lossy(value.as_slice())
                                                    .to_string()
                                            })
                                            .map_err(|err| {
                                                PluginLoadError::SecretUnreadable(
                                                    secret.reference.clone(),
                                                    err,
                                                )
                                            })?,
                                    )
                                }
                            };
                            request = request.header(
                                reqwest::header::AUTHORIZATION,
                                authorization_value.expose_secret(),
                            );
                        }
                        let bytes = request.send()?.bytes()?;
                        if let PluginVerification::Sha256(digest) = &guest_config.verification {
                            // The expected digest should already be in raw byte form here, not hex-encoded.
                            let mut hasher = Sha256::new();
                            hasher.update(&bytes[..]);
                            let plugin_digest = hasher.finalize();
                            if plugin_digest.as_slice() != &digest[..] {
                                // Need to make both sides hex-encoded to make the error message readable.
                                return Err(PluginLoadError::VerificationError(
                                    "sha256".to_string(),
                                    hex::encode(&digest[..]),
                                    hex::encode(plugin_digest.as_slice()),
                                ));
                            }
                        }
                        Ok(Component::from_binary(engine, &bytes[..])?)
                    }
                    bulwark_config::PluginLocation::Bytes(bytes) => {
                        Ok(Component::from_binary(engine, &bytes[..])?)
                    }
                }
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
    pub(crate) fn guest_config(&self) -> &bulwark_sdk::Map<String, bulwark_sdk::Value> {
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

/// An instance of a [`Plugin`], associated with a [`PluginCtx`].
pub struct PluginInstance {
    /// A reference to the parent `Plugin` and its configuration.
    plugin: Arc<Plugin>,
    /// The WASM store that holds state associated with the incoming request.
    store: Store<PluginCtx>,
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
        plugin_ctx: PluginCtx,
    ) -> Result<PluginInstance, PluginInstantiationError> {
        fn type_annotate_http<F>(f: F) -> F
        where
            F: Fn(&mut PluginCtx) -> WasiHttpImpl<&mut PluginCtx>,
        {
            f
        }
        let closure = type_annotate_http::<_>(|t| WasiHttpImpl(t));

        // Clone the stdio so we can read the captured stdout and stderr buffers after execution has completed.
        let stdio = plugin_ctx.stdio.clone();

        let mut linker: Linker<PluginCtx> = Linker::new(&plugin.engine);
        let mut store = Store::new(&plugin.engine, plugin_ctx);

        wasmtime_wasi::add_to_linker_async(&mut linker).context(format!(
            "failed to link wasi interface for '{}'",
            plugin.reference
        ))?;
        wasmtime_wasi_http::bindings::wasi::http::types::add_to_linker_get_host(
            &mut linker,
            closure,
        )
        .context(format!(
            "failed to link `wasi:http/types` interface for '{}'",
            plugin.reference
        ))?;
        wasmtime_wasi_http::bindings::wasi::http::outgoing_handler::add_to_linker_get_host(
            &mut linker,
            closure,
        )
        .context(format!(
            "failed to link `wasi:http/outgoing-handler` interface for '{}'",
            plugin.reference
        ))?;
        bindings::bulwark::plugin::config::add_to_linker(&mut linker, |t| t).context(format!(
            "failed to link `bulwark:plugin/config` interface for '{}'",
            plugin.reference
        ))?;
        bindings::bulwark::plugin::redis::add_to_linker(&mut linker, |t| t).context(format!(
            "failed to link `bulwark:plugin/redis` interface for '{}'",
            plugin.reference
        ))?;
        bindings::bulwark::plugin::types::add_to_linker(&mut linker, |t| t).context(format!(
            "failed to link `bulwark:plugin/types` interface for '{}'",
            plugin.reference
        ))?;

        // We discard the instance for this because we only use the generated interface to make calls

        let (http_detection, _) =
            bindings::HttpDetection::instantiate_async(&mut store, &plugin.component, &linker)
                .await
                .context(format!("failed to instantiate '{}'", plugin.reference))?;

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
            Ok(Ok(_)) => metrics::counter!(
                "plugin_on_init",
                "ref" => self.plugin_reference(), "result" => "ok"
            )
            .increment(1),
            Ok(Err(_)) | Err(_) => metrics::counter!(
                "plugin_on_init",
                "ref" => self.plugin_reference(), "result" => "error"
            )
            .increment(1),
        }

        // Initialization doesn't return anything unless there's an error
        result??;
        Ok(())
    }

    /// Executes the guest's `on_request` function.
    pub async fn handle_request_enrichment(
        &mut self,
        request: Arc<bulwark_sdk::http::Request>,
        labels: HashMap<String, String>,
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
        let labels: Vec<(String, String)> = labels.into_iter().collect();
        let result = self
            .http_detection
            .bulwark_plugin_http_handlers()
            .call_handle_request_enrichment(
                self.store.as_context_mut(),
                incoming_request_handle,
                labels.as_slice(),
            )
            .await;
        match result {
            Ok(Ok(_)) => metrics::counter!(
                "plugin_on_request",
                "ref" => self.plugin_reference(), "result" => "ok"
            )
            .increment(1),
            Ok(Err(_)) | Err(_) => metrics::counter!(
                "plugin_on_request",
                "ref" => self.plugin_reference(), "result" => "error"
            )
            .increment(1),
        }
        let labels: HashMap<String, String> = result??.into_iter().collect();

        Ok(labels)
    }

    /// Executes the guest's `on_request_decision` function.
    pub async fn handle_request_decision(
        &mut self,
        request: Arc<bulwark_sdk::http::Request>,
        labels: HashMap<String, String>,
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

        let labels: Vec<(String, String)> = labels.into_iter().collect();
        let result = self
            .http_detection
            .bulwark_plugin_http_handlers()
            .call_handle_request_decision(
                self.store.as_context_mut(),
                incoming_request_handle,
                labels.as_slice(),
            )
            .await;
        match result {
            Ok(Ok(_)) => metrics::counter!(
                "plugin_on_request_decision",
                "ref" => self.plugin_reference(), "result" => "ok"
            )
            .increment(1),
            Ok(Err(_)) | Err(_) => metrics::counter!(
                "plugin_on_request_decision",
                "ref" => self.plugin_reference(), "result" => "error"
            )
            .increment(1),
        }

        Ok(result??.into())
    }

    /// Executes the guest's `on_response_decision` function.
    pub async fn handle_response_decision(
        &mut self,
        request: Arc<bulwark_sdk::http::Request>,
        response: Arc<bulwark_sdk::http::Response>,
        labels: HashMap<String, String>,
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
        };
        let incoming_response_handle = self
            .store
            .as_context_mut()
            .data_mut()
            .new_incoming_response(incoming_response)?;

        let labels: Vec<(String, String)> = labels.into_iter().collect();
        let result = self
            .http_detection
            .bulwark_plugin_http_handlers()
            .call_handle_response_decision(
                self.store.as_context_mut(),
                incoming_request_handle,
                incoming_response_handle,
                labels.as_slice(),
            )
            .await;
        match result {
            Ok(Ok(_)) => metrics::counter!(
                "plugin_on_request_body_decision",
                "ref" => self.plugin_reference(), "result" => "ok"
            )
            .increment(1),
            Ok(Err(_)) | Err(_) => metrics::counter!(
                "plugin_on_request_body_decision",
                "ref" => self.plugin_reference(), "result" => "error"
            )
            .increment(1),
        }

        Ok(result??.into())
    }

    /// Executes the guest's `on_decision_feedback` function.
    pub async fn handle_decision_feedback(
        &mut self,
        request: Arc<bulwark_sdk::http::Request>,
        response: Arc<bulwark_sdk::http::Response>,
        labels: HashMap<String, String>,
        verdict: bulwark_sdk::Verdict,
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
        };
        let incoming_response_handle = self
            .store
            .as_context_mut()
            .data_mut()
            .new_incoming_response(incoming_response)?;

        let labels: Vec<(String, String)> = labels.into_iter().collect();
        let result = self
            .http_detection
            .bulwark_plugin_http_handlers()
            .call_handle_decision_feedback(
                self.store.as_context_mut(),
                incoming_request_handle,
                incoming_response_handle,
                labels.as_slice(),
                &verdict.into(),
            )
            .await;
        match result {
            Ok(Ok(_)) => metrics::counter!(
                "plugin_on_decision_feedback",
                "ref" => self.plugin_reference(), "result" => "ok"
            )
            .increment(1),
            Ok(Err(_)) | Err(_) => metrics::counter!(
                "plugin_on_decision_feedback",
                "ref" => self.plugin_reference(), "result" => "error"
            )
            .increment(1),
        }

        // Decision feedback doesn't return anything unless there's an error
        result??;
        Ok(())
    }
}
