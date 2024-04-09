//! The service module contains the main Envoy external processor service implementation.

use crate::{PluginGroupInstantiationError, ProcessingMessageError, RequestError, ResponseError};
use bulwark_config::Config;
use bulwark_sdk::Verdict;

use bulwark_host::{
    ForwardedIP, HandlerOutput, Plugin, PluginCtx, PluginExecutionError, PluginInstance,
    PluginLoadError, RedisCtx, ScriptRegistry,
};
use bulwark_sdk::Decision;
use envoy_control_plane::envoy::{
    config::core::v3::HeaderMap,
    extensions::filters::http::ext_proc::v3::{processing_mode, ProcessingMode},
    r#type::v3::HttpStatus,
    service::ext_proc::v3::{
        external_processor_server::ExternalProcessor, processing_request, processing_response,
        BodyResponse, CommonResponse, HeadersResponse, HttpBody, HttpHeaders, ImmediateResponse,
        ProcessingRequest, ProcessingResponse,
    },
};
use forwarded_header_value::ForwardedHeaderValue;
use futures::lock::Mutex;
use futures::{channel::mpsc::UnboundedSender, SinkExt, Stream};
use matchit::Router;
use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    pin::Pin,
    str::{self, FromStr},
    sync::Arc,
    time::Duration,
};
use tokio::{sync::RwLock, sync::Semaphore, task::JoinSet, time::timeout};
use tonic::Streaming;
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

macro_rules! format_f64 {
    ($expression:expr) => {
        tracing::field::display(crate::format::Float3Formatter($expression))
    };
}

extern crate redis;

type ExternalProcessorStream =
    Pin<Box<dyn Stream<Item = Result<ProcessingResponse, tonic::Status>> + Send>>;
type PluginList = Vec<Arc<Plugin>>;

/// A RouteTarget allows a router to map from a routing pattern to a plugin group and associated config values.
///
/// See [`bulwark_config::Resource`] for its configuration.
struct RouteTarget {
    plugins: PluginList,
    timeout: Option<u64>,
}

/// Helper function that joins everything in a joinset, ignoring success and raising warnings as needed
async fn join_all<T, F>(
    mut join_set: JoinSet<Result<Result<T, PluginExecutionError>, tokio::time::error::Elapsed>>,
    mut success: F,
) where
    F: FnMut(T),
    T: 'static,
{
    // efficiently hand execution off to the the tasks we're joining
    tokio::task::yield_now().await;

    while let Some(r) = join_set.join_next().await {
        match r {
            Ok(Ok(Ok(output))) => success(output),
            // These 3 errors are only logged, not bubbled up
            Ok(Ok(Err(err))) => {
                error!(
                    message = "plugin execution error",
                    elapsed = ?err,
                );
            }
            Ok(Err(err)) => {
                warn!(
                    message = "timeout on plugin execution",
                    elapsed = ?err,
                );
            }
            Err(err) => {
                warn!(
                    message = "join error on plugin execution",
                    error_message = ?err,
                );
            }
        }
    }
}

/// The `BulwarkProcessor` implements the primary envoy processing service logic via the [`ExternalProcessor`] trait.
///
/// The [`process`](BulwarkProcessor::process) function is the main request handler.
#[derive(Clone)]
pub struct BulwarkProcessor {
    // TODO: may need to have a plugin registry at some point
    router: Arc<RwLock<Router<RouteTarget>>>,
    redis_ctx: RedisCtx,
    request_semaphore: Arc<tokio::sync::Semaphore>,
    plugin_semaphore: Arc<tokio::sync::Semaphore>,
    thresholds: bulwark_config::Thresholds,
    proxy_hops: usize,
    // TODO: redis circuit breaker for health monitoring
}

#[tonic::async_trait]
impl ExternalProcessor for BulwarkProcessor {
    type ProcessStream = ExternalProcessorStream;

    /// Processes an incoming request, performing all Envoy-specific handling needed by [`bulwark_host`].
    #[instrument(name = "handle request", skip(self, tonic_request))]
    async fn process(
        &self,
        tonic_request: tonic::Request<Streaming<ProcessingRequest>>,
    ) -> Result<tonic::Response<ExternalProcessorStream>, tonic::Status> {
        let bulwark_processor = self.clone();
        let thresholds = self.thresholds;
        let proxy_hops = self.proxy_hops;
        let plugin_semaphore = self.plugin_semaphore.clone();

        let stream = tonic_request.into_inner();
        let (sender, receiver) = futures::channel::mpsc::unbounded();

        let arc_sender = Arc::new(Mutex::new(sender));
        let arc_stream = Arc::new(Mutex::new(stream));

        let child_span = tracing::info_span!("route request");
        let permit = self
            .request_semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        tokio::task::spawn(
            async move {
                if let Ok(request) = ProcessorContext::prepare_request(
                    arc_sender.clone(),
                    arc_stream.clone(),
                    proxy_hops,
                )
                .await
                {
                    let request = Arc::new(request);

                    info!(
                        message = "process request",
                        method = request.method().to_string(),
                        uri = request.uri().to_string(),
                        user_agent = request
                            .headers()
                            .get("User-Agent")
                            .map(|ua: &http::HeaderValue| ua.to_str().unwrap_or_default())
                    );

                    let router = bulwark_processor.router.read().await;
                    let route_result = router.at(request.uri().path());
                    // TODO: router needs to point to a struct that bundles the plugin set and associated config like timeout duration
                    // TODO: put default timeout in a constant somewhere central
                    let mut timeout_duration = Duration::from_millis(10);
                    match route_result {
                        Ok(route_match) => {
                            // TODO: may want to expose labels to logging after redaction
                            let mut router_labels = HashMap::new();
                            for (key, value) in route_match.params.iter() {
                                router_labels.insert(format!("route.{}", key), value.to_string());
                            }

                            let route_target = route_match.value;
                            // TODO: figure out how best to bubble the error out of the task and up to the parent
                            // TODO: figure out if tonic-error or some other option is the best way to convert to a tonic Status error
                            // TODO: we probably want to be initializing only when necessary now rather than on every request
                            let plugin_instances = bulwark_processor
                                .instantiate_plugins(&route_target.plugins)
                                .await
                                .unwrap();
                            if let Some(millis) = route_match.value.timeout {
                                timeout_duration = Duration::from_millis(millis);
                            }

                            let mut ctx = ProcessorContext {
                                sender: arc_sender,
                                stream: arc_stream,
                                plugin_semaphore,
                                plugin_instances: plugin_instances.clone(),
                                router_labels,
                                request: request.clone(),
                                response: None,
                                verdict: None,
                                combined_output: HandlerOutput::default(),
                                plugin_outputs: HashMap::new(),
                                thresholds,
                                timeout_duration,
                            };

                            ctx.execute_init_phase().await;

                            ctx.execute_request_enrichment_phase().await;
                            ctx.execute_request_decision_phase().await;

                            ctx.complete_request_phase().await;
                        }
                        Err(_) => {
                            // TODO: figure out how best to handle trailing slash errors, silent failure is probably undesirable
                            error!(uri = request.uri().to_string(), message = "match error");
                            // TODO: panic is undesirable, need to figure out if we should be returning a Status or changing the response or doing something else
                            panic!("match error");
                        }
                    };
                }
                drop(permit);
            }
            .instrument(child_span.or_current()),
        );
        return Ok(tonic::Response::new(Box::pin(receiver)));

        // // By default, just close the stream.
        // Ok(tonic::Response::new(Box::pin(futures::stream::empty())))
    }
}

impl BulwarkProcessor {
    /// Creates a new [`BulwarkProcessor`].
    ///
    /// # Arguments
    ///
    /// * `config` - The root of the Bulwark configuration structure to be used to initialize the service.
    pub async fn new(config: Config) -> Result<Self, PluginLoadError> {
        // Get all outcomes registered even if those outcomes don't happen immediately.
        metrics::register_counter!(
            "combined_decision",
            "outcome" => "trusted",
        );
        metrics::register_counter!(
            "combined_decision",
            "outcome" => "accepted",
        );
        metrics::register_counter!(
            "combined_decision",
            "outcome" => "suspected",
        );
        metrics::register_counter!(
            "combined_decision",
            "outcome" => "restricted",
        );
        metrics::register_histogram!("combined_decision_score");

        let redis_pool: Option<Arc<deadpool_redis::Pool>> =
            if let Some(redis_addr) = config.state.redis_uri.as_ref() {
                let cfg = deadpool_redis::Config {
                    url: Some(redis_addr.into()),
                    connection: None,
                    pool: Some(deadpool_redis::PoolConfig::new(
                        config.state.redis_pool_size,
                    )),
                };
                Some(Arc::new(
                    cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1))?,
                ))
            } else {
                None
            };

        let redis_ctx = RedisCtx {
            pool: redis_pool,
            registry: Arc::new(ScriptRegistry::default()),
        };

        let mut router: Router<RouteTarget> = Router::new();
        if config.resources.is_empty() {
            // TODO: return an init error not a plugin load error
            return Err(PluginLoadError::ResourceMissing);
        }
        for resource in &config.resources {
            let plugin_configs = resource.resolve_plugins(&config)?;
            let mut plugins: PluginList = Vec::with_capacity(plugin_configs.len());
            for plugin_config in plugin_configs {
                // TODO: pass in the plugin config
                debug!(
                    message = "load plugin",
                    path = plugin_config.path,
                    resource = resource.route
                );
                let plugin = Plugin::from_file(plugin_config.path.clone(), &config, plugin_config)?;
                plugins.push(Arc::new(plugin));
            }
            router
                .insert(
                    resource.route.clone(),
                    // TODO: the route target will probably need access to the route itself in the future
                    RouteTarget {
                        timeout: resource.timeout,
                        plugins,
                    },
                )
                .ok();
        }
        Ok(Self {
            router: Arc::new(RwLock::new(router)),
            request_semaphore: Arc::new(Semaphore::new(config.runtime.max_concurrent_requests)),
            plugin_semaphore: Arc::new(Semaphore::new(config.runtime.max_plugin_tasks)),
            thresholds: config.thresholds,
            proxy_hops: usize::from(config.service.proxy_hops),
            redis_ctx,
        })
    }

    async fn instantiate_plugins(
        &self,
        plugins: &PluginList,
    ) -> Result<Vec<Arc<Mutex<PluginInstance>>>, PluginGroupInstantiationError> {
        let mut plugin_instances = Vec::with_capacity(plugins.len());
        for plugin in plugins {
            let mut environment = HashMap::new();
            for key in &plugin.permissions().env {
                match std::env::var(key) {
                    Ok(value) => {
                        environment.insert(key.clone(), value);
                    }
                    Err(err) => {
                        warn!("plugin requested environment variable '{}' but it could not be provided: {}", key, err);
                    }
                }
            }
            let request_context =
                PluginCtx::new(plugin.clone(), environment, self.redis_ctx.clone())?;
            plugin_instances.push(Arc::new(Mutex::new(
                PluginInstance::new(plugin.clone(), request_context).await?,
            )));
        }
        Ok(plugin_instances)
    }

    async fn dispatch_init(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<(), PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance.handle_init().await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_init",
                "ref" => plugin_instance.plugin_reference(),
                "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_init",
                "ref" => plugin_instance.plugin_reference(),
                "result" => "error"
            ),
        }
        result
    }

    async fn dispatch_request_enrichment(
        plugin_instance: Arc<Mutex<PluginInstance>>,
        request: Arc<bulwark_sdk::Request>,
        labels: HashMap<String, String>,
    ) -> Result<HashMap<String, String>, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance
            .handle_request_enrichment(request, labels)
            .await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_request",
                "ref" => plugin_instance.plugin_reference(),
                "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_request",
                "ref" => plugin_instance.plugin_reference(),
                "result" => "error"
            ),
        }
        result
    }

    async fn dispatch_request_decision(
        plugin_instance: Arc<Mutex<PluginInstance>>,
        request: Arc<bulwark_sdk::Request>,
        labels: HashMap<String, String>,
    ) -> Result<HandlerOutput, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance
            .handle_request_decision(request, labels)
            .await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_request_decision",
                "ref" => plugin_instance.plugin_reference(),
                "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_request_decision",
                "ref" => plugin_instance.plugin_reference(),
                "result" => "error"
            ),
        }
        result
    }

    async fn dispatch_response_decision(
        plugin_instance: Arc<Mutex<PluginInstance>>,
        request: Arc<bulwark_sdk::Request>,
        response: Arc<bulwark_sdk::Response>,
        labels: HashMap<String, String>,
    ) -> Result<HandlerOutput, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance
            .handle_response_decision(request, response, labels)
            .await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_response_decision",
                "ref" => plugin_instance.plugin_reference(),
                "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_response_decision",
                "ref" => plugin_instance.plugin_reference(),
                "result" => "error"
            ),
        }
        result
    }

    async fn dispatch_decision_feedback(
        plugin_instance: Arc<Mutex<PluginInstance>>,
        request: Arc<bulwark_sdk::Request>,
        response: Arc<bulwark_sdk::Response>,
        labels: HashMap<String, String>,
        verdict: bulwark_sdk::Verdict,
    ) -> Result<(), PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance
            .handle_decision_feedback(request, response, labels, verdict)
            .await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_decision_feedback",
                "ref" => plugin_instance.plugin_reference(),
                "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_decision_feedback",
                "ref" => plugin_instance.plugin_reference(),
                "result" => "error"
            ),
        }
        result
    }
}

/// The `ProcessorContext` wraps values associated with a single request/response cycle.
struct ProcessorContext {
    sender: Arc<Mutex<UnboundedSender<Result<ProcessingResponse, tonic::Status>>>>,
    stream: Arc<Mutex<Streaming<ProcessingRequest>>>,
    plugin_semaphore: Arc<tokio::sync::Semaphore>,
    plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
    router_labels: HashMap<String, String>,
    request: Arc<bulwark_sdk::Request>,
    response: Option<Arc<bulwark_sdk::Response>>,
    verdict: Option<Verdict>,
    combined_output: HandlerOutput,
    plugin_outputs: HashMap<String, HandlerOutput>,
    thresholds: bulwark_config::Thresholds,
    timeout_duration: Duration,
}

impl ProcessorContext {
    async fn prepare_request(
        sender: Arc<Mutex<UnboundedSender<Result<ProcessingResponse, tonic::Status>>>>,
        stream: Arc<Mutex<Streaming<ProcessingRequest>>>,
        proxy_hops: usize,
    ) -> Result<bulwark_sdk::Request, RequestError> {
        if let Some(header_msg) = Self::get_request_header_message(stream.clone()).await? {
            // If there is no body, we have to skip these to avoid Envoy errors.
            let body = if !header_msg.end_of_stream {
                // We have to send a reply back before we can retrieve the request body
                Self::send_request_headers_message(sender).await?;

                Self::get_request_body_message(stream)
                    .await?
                    .map(|body_msg| body_msg.body)
            } else {
                None
            };

            // TODO: currently this information isn't used and isn't accessible to the plugin environment yet
            // TODO: does this go into a request extension?
            // TODO: :protocol?
            let _authority = Self::get_header_value(&header_msg.headers, ":authority")
                .ok_or(RequestError::MissingAuthority)?;
            let _scheme = Self::get_header_value(&header_msg.headers, ":scheme")
                .ok_or(RequestError::MissingScheme)?;

            let method = http::Method::from_str(
                Self::get_header_value(&header_msg.headers, ":method")
                    .ok_or(RequestError::MissingMethod)?,
            )?;
            let request_uri = Self::get_header_value(&header_msg.headers, ":path")
                .ok_or(RequestError::MissingPath)?;
            let mut request = http::Request::builder();
            let request_chunk = if let Some(body) = body {
                bytes::Bytes::from(body)
            } else {
                bytes::Bytes::new()
            };
            // No current access to HTTP version information via Envoy external processor
            request = request.method(method).uri(request_uri);
            match &header_msg.headers {
                Some(headers) => {
                    for header in &headers.headers {
                        // must not pass through Envoy pseudo headers here, http module treats them as invalid
                        if !header.key.starts_with(':') {
                            request = request.header(&header.key, &header.value);
                        }
                    }
                }
                None => {}
            }

            // TODO: remote IP should probably be received via an external attribute, but that doesn't seem to be currently supported by envoy?
            // NOTE: header keys must be sent in lower case
            if let Some(forwarded) = Self::get_header_value(&header_msg.headers, "forwarded") {
                if let Some(ip_addr) = Self::parse_forwarded_ip(forwarded, proxy_hops) {
                    request = request.extension(ForwardedIP(ip_addr));
                }
            } else if let Some(forwarded) =
                Self::get_header_value(&header_msg.headers, "x-forwarded-for")
            {
                if let Some(ip_addr) = Self::parse_x_forwarded_for_ip(forwarded, proxy_hops) {
                    request = request.extension(ForwardedIP(ip_addr));
                }
            }

            return Ok(request.body(request_chunk)?);
        }
        Err(RequestError::MissingHeaders)
    }

    async fn prepare_response(&mut self) -> Result<bulwark_sdk::Response, ResponseError> {
        if let Some(header_msg) = Self::get_response_headers_message(self.stream.clone()).await? {
            // If there is no body, we have to skip these to avoid Envoy errors.
            let body = if !header_msg.end_of_stream {
                // We have to send a reply back before we can retrieve the response body
                Self::send_response_headers_message(self.sender.clone()).await?;

                Self::get_response_body_message(self.stream.clone())
                    .await?
                    .map(|body_msg| body_msg.body)
            } else {
                None
            };

            let status = Self::get_header_value(&header_msg.headers, ":status")
                .ok_or(ResponseError::MissingStatus)?;

            let mut response = http::Response::builder();
            let response_chunk = if let Some(body) = body {
                bytes::Bytes::from(body)
            } else {
                bytes::Bytes::new()
            };
            response = response.status(status).version(self.request.version());
            match &header_msg.headers {
                Some(headers) => {
                    for header in &headers.headers {
                        // must not pass through Envoy pseudo headers here, http module treats them as invalid
                        if !header.key.starts_with(':') {
                            response = response.header(&header.key, &header.value);
                        }
                    }
                }
                None => {}
            }
            return Ok(response.body(response_chunk)?);
        }
        Err(ResponseError::MissingHeaders)
    }

    async fn execute_init_phase(&self) {
        let mut init_phase_tasks = JoinSet::new();
        for plugin_instance in self.plugin_instances.iter().cloned() {
            let init_phase_child_span = tracing::info_span!("execute handle_init",);
            let permit = self
                .plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            init_phase_tasks.spawn(
                timeout(self.timeout_duration, async move {
                    let result = BulwarkProcessor::dispatch_init(plugin_instance).await;
                    drop(permit);
                    result
                })
                .instrument(init_phase_child_span.or_current()),
            );
        }
        join_all(init_phase_tasks, |_| {}).await;
    }

    async fn execute_request_enrichment_phase(&mut self) {
        let mut enrichment_phase_tasks = JoinSet::new();
        for plugin_instance in self.plugin_instances.iter().cloned() {
            let enrichment_phase_child_span =
                tracing::info_span!("execute handle_request_enrichment",);
            let permit = self
                .plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            let request = self.request.clone();
            let router_labels = self.router_labels.clone();
            enrichment_phase_tasks.spawn(
                timeout(self.timeout_duration, async move {
                    let result = BulwarkProcessor::dispatch_request_enrichment(
                        plugin_instance,
                        request,
                        router_labels,
                    )
                    .await;
                    drop(permit);
                    result
                })
                .instrument(enrichment_phase_child_span.or_current()),
            );
        }

        let mut labels = self.router_labels.clone();
        join_all(enrichment_phase_tasks, |new_labels| {
            // Merge labels from each plugin
            labels.extend(new_labels);
        })
        .await;
        self.combined_output = HandlerOutput {
            decision: Decision::default(),
            tags: HashSet::new(),
            labels,
        };
    }

    async fn execute_request_decision_phase(&mut self) {
        let outputs: Arc<Mutex<Vec<HandlerOutput>>> =
            Arc::new(Mutex::new(Vec::with_capacity(self.plugin_instances.len())));
        let plugin_outputs: Arc<Mutex<HashMap<String, HandlerOutput>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let mut decision_phase_tasks = JoinSet::new();
        // The .iter().cloned() appears to be necessary
        #[allow(clippy::unnecessary_to_owned)]
        for plugin_instance in self.plugin_instances.iter().cloned() {
            let decision_phase_child_span = tracing::info_span!("execute handle_request_decision",);
            let permit = self
                .plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            let outputs = outputs.clone();
            let plugin_outputs = plugin_outputs.clone();
            let request = self.request.clone();
            // Need to be careful that we grab the labels emitted by the request phase and not the labels we started with.
            let labels = self.combined_output.labels.clone();
            decision_phase_tasks.spawn(
                timeout(self.timeout_duration, async move {
                    let output_result = BulwarkProcessor::dispatch_request_decision(
                        plugin_instance.clone(),
                        request,
                        labels,
                    )
                    .await;
                    if let Ok(output) = &output_result {
                        // Re-weight the decision based on its weighting value from the configuration
                        let plugin_instance = plugin_instance.lock().await;
                        let mut output = output.clone();
                        output.decision = output.decision.weight(plugin_instance.weight());

                        let decision = &output.decision;
                        info!(
                            message = "plugin decision",
                            name = plugin_instance.plugin_reference(),
                            accept = format_f64!(decision.accept),
                            restrict = format_f64!(decision.restrict),
                            unknown = format_f64!(decision.unknown),
                            score = format_f64!(decision.pignistic().restrict),
                            weight = format_f64!(plugin_instance.weight()),
                        );

                        let mut outputs = outputs.lock().await;
                        outputs.push(output.clone());
                        let mut plugin_outputs = plugin_outputs.lock().await;
                        plugin_outputs.insert(plugin_instance.plugin_reference(), output);
                    } else if let Err(err) = &output_result {
                        error!(message = "plugin error", error = err.to_string());
                        let mut outputs = outputs.lock().await;
                        outputs.push(HandlerOutput {
                            decision: bulwark_sdk::UNKNOWN,
                            tags: HashSet::from([String::from("error")]),
                            labels: HashMap::new(),
                        });
                    }
                    drop(permit);
                    output_result.map(|output| output.labels)
                })
                .instrument(decision_phase_child_span.or_current()),
            );
        }

        let mut labels = self.router_labels.clone();
        join_all(decision_phase_tasks, |new_labels| {
            // Merge labels from each plugin
            labels.extend(new_labels);
        })
        .await;

        let decision_vec: Vec<Decision>;
        {
            let outputs = outputs.lock().await;
            decision_vec = outputs.iter().map(|dc| dc.decision).collect();
            self.combined_output.tags.extend(
                outputs
                    .iter()
                    .flat_map(|dc| dc.tags.clone())
                    .collect::<HashSet<String>>(),
            );
        }
        let decision = Decision::combine_murphy(&decision_vec);

        let plugin_outputs = plugin_outputs.lock().await;
        self.combined_output = HandlerOutput {
            decision,
            tags: self.combined_output.tags.clone(),
            labels,
        };
        self.plugin_outputs = plugin_outputs.clone();
    }

    async fn execute_response_phase(&mut self) {
        let outputs: Arc<Mutex<Vec<HandlerOutput>>> =
            Arc::new(Mutex::new(Vec::with_capacity(self.plugin_instances.len())));
        let new_plugin_outputs: Arc<Mutex<HashMap<String, HandlerOutput>>> =
            Arc::new(Mutex::new(self.plugin_outputs.clone()));
        let mut response_phase_tasks = JoinSet::new();
        // The .iter().cloned() appears to be necessary
        #[allow(clippy::unnecessary_to_owned)]
        for plugin_instance in self.plugin_instances.iter().cloned() {
            let response_phase_child_span =
                tracing::info_span!("execute handle_response_decision",);
            let permit = self
                .plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            let request = self.request.clone();
            let response = self
                .response
                .clone()
                .expect("cannot execute response phase without response");
            // Need to be careful that we grab the labels emitted by the request phase and not the labels we started with.
            let labels = self.combined_output.labels.clone();
            let outputs = outputs.clone();
            let new_plugin_outputs = new_plugin_outputs.clone();
            let prior_plugin_outputs = self
                .plugin_outputs
                .get(&plugin_instance.lock().await.plugin_reference())
                .cloned();
            response_phase_tasks.spawn(
                timeout(self.timeout_duration, async move {
                    let output_result = BulwarkProcessor::dispatch_response_decision(
                        plugin_instance.clone(),
                        request,
                        response,
                        labels,
                    )
                    .await;
                    if let Ok(output) = &output_result {
                        // Re-weight the decision based on its weighting value from the configuration
                        let plugin_instance = plugin_instance.lock().await;
                        let mut output = output.clone();
                        output.decision = output.decision.weight(plugin_instance.weight());

                        if let Some(prior_plugin_outputs) = prior_plugin_outputs {
                            // If the prior output was non-zero and the new output was zero, then keep the prior output.
                            if !prior_plugin_outputs.decision.is_unknown()
                                && output.decision.is_unknown()
                            {
                                // The prior decision was already weighted and does not need to have weights applied.
                                output.decision = prior_plugin_outputs.decision;
                            }
                        }

                        let decision = &output.decision;
                        info!(
                            message = "plugin decision",
                            name = plugin_instance.plugin_reference(),
                            accept = format_f64!(decision.accept),
                            restrict = format_f64!(decision.restrict),
                            unknown = format_f64!(decision.unknown),
                            score = format_f64!(decision.pignistic().restrict),
                            weight = format_f64!(plugin_instance.weight()),
                        );

                        let mut outputs = outputs.lock().await;
                        outputs.push(output.clone());
                        let mut new_plugin_outputs = new_plugin_outputs.lock().await;
                        new_plugin_outputs.insert(plugin_instance.plugin_reference(), output);
                    } else if let Err(err) = &output_result {
                        error!(message = "plugin error", error = err.to_string());
                        let mut outputs = outputs.lock().await;
                        outputs.push(HandlerOutput {
                            decision: bulwark_sdk::UNKNOWN,
                            tags: HashSet::from([String::from("error")]),
                            labels: HashMap::new(),
                        });
                    }
                    drop(permit);
                    output_result.map(|output| output.labels)
                })
                .instrument(response_phase_child_span.or_current()),
            );
        }

        let mut labels = self.router_labels.clone();
        join_all(response_phase_tasks, |new_labels| {
            // Merge labels from each plugin
            labels.extend(new_labels);
        })
        .await;

        let decision_vec: Vec<Decision>;
        {
            let outputs = outputs.lock().await;
            decision_vec = outputs.iter().map(|dc| dc.decision).collect();
            self.combined_output.tags.extend(
                outputs
                    .iter()
                    .flat_map(|dc| dc.tags.clone())
                    .collect::<HashSet<String>>(),
            );
        }
        let decision = Decision::combine_murphy(&decision_vec);

        let new_plugin_outputs = new_plugin_outputs.lock().await;
        self.combined_output = HandlerOutput {
            decision,
            tags: self.combined_output.tags.clone(),
            labels,
        };
        self.plugin_outputs = new_plugin_outputs.clone();
    }

    async fn execute_decision_feedback(&mut self) {
        let verdict = self
            .verdict
            .as_ref()
            .expect("cannot execute feedback phase without verdict");
        metrics::increment_counter!(
            "combined_decision",
            "outcome" => verdict.outcome.to_string(),
        );
        metrics::histogram!(
            "combined_decision_score",
            verdict.decision.pignistic().restrict
        );

        let mut decisions: Vec<Decision> = Vec::with_capacity(self.plugin_instances.len());
        let mut feedback_phase_tasks = JoinSet::new();
        for plugin_instance in self.plugin_instances.iter().cloned() {
            let response_phase_child_span =
                tracing::info_span!("execute handle_decision_feedback",);
            let permit = self
                .plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            {
                // Make sure the plugin instance knows about the final combined decision
                let plugin_instance = plugin_instance.lock().await;
                let decision = self
                    .plugin_outputs
                    .get(&plugin_instance.plugin_reference())
                    .map(|output| output.decision)
                    // This could happen if the plugin panics.
                    .unwrap_or_else(|| {
                        warn!(
                            message = "plugin decision missing",
                            reference = &plugin_instance.plugin_reference(),
                        );
                        Decision::default()
                    });
                metrics::histogram!(
                    "decision_score",
                    decision.pignistic().restrict,
                    "ref" => plugin_instance.plugin_reference(),
                );
                // Measure the conflict between each individual decision and the combined decision
                metrics::histogram!(
                    "decision_conflict",
                    Decision::conflict(&[decision, verdict.decision]),
                    "ref" => plugin_instance.plugin_reference(),
                );
                decisions.push(decision);
            }
            let request = self.request.clone();
            let response = self
                .response
                .clone()
                .expect("cannot execute feedback phase without response");
            // Need to be careful that we grab the labels emitted by the request phase and not the labels we started with.
            let labels = self.combined_output.labels.clone();
            let verdict = verdict.clone();
            feedback_phase_tasks.spawn(
                timeout(self.timeout_duration, async move {
                    let result = BulwarkProcessor::dispatch_decision_feedback(
                        plugin_instance,
                        request,
                        response,
                        labels,
                        verdict,
                    )
                    .await;
                    drop(permit);
                    result
                })
                .instrument(response_phase_child_span.or_current()),
            );
        }
        join_all(feedback_phase_tasks, |_| {}).await;

        // Measure total conflict in the combined decision
        metrics::histogram!("combined_conflict", Decision::conflict(&decisions));

        // Capturing stdio is always the last thing that happens and feedback should always be the second-to-last.
        self.capture_stdio().await;
    }

    async fn complete_request_phase(&mut self) {
        let decision = self.combined_output.decision;
        let outcome = decision
            .outcome(
                self.thresholds.trust,
                self.thresholds.suspicious,
                self.thresholds.restrict,
            )
            .unwrap();

        info!(
            message = "combine decision",
            accept = format_f64!(decision.accept),
            restrict = format_f64!(decision.restrict),
            unknown = format_f64!(decision.unknown),
            score = format_f64!(decision.pignistic().restrict),
            outcome = outcome.to_string(),
            observe_only = self.thresholds.observe_only,
            // array values aren't handled well unfortunately, coercing to comma-separated values seems to be the best option
            tags = self
                .combined_output
                .tags
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>()
                .to_vec()
                .join(","),
        );
        metrics::increment_counter!(
            "plugin_request_phase_decision",
            "outcome" => outcome.to_string(),
            "observe_only" => self.thresholds.observe_only.to_string(),
        );

        let mut restricted = false;
        let end_of_stream = self.request.body().is_empty();
        match outcome {
            bulwark_sdk::Outcome::Trusted
            | bulwark_sdk::Outcome::Accepted
            // suspected requests are monitored but not rejected
            | bulwark_sdk::Outcome::Suspected => {
                let result = Self::send_allow_request_message(self.sender.clone(), end_of_stream).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    error!(message = format!("send error: {}", err));
                }
            },
            bulwark_sdk::Outcome::Restricted => {
                restricted = true;
                if !self.thresholds.observe_only {
                    info!(message = "process response", status = 403);
                    match Self::send_block_request_message(self.sender.clone()).await {
                        Ok(response) => {
                            // Normally we initiate feedback after the response phase, but if we're blocking the request
                            // in the request phase, we're also skipping the response phase and we need to do it here
                            // instead.
                            let verdict = Verdict {
                                decision,
                                outcome,
                                tags: self.combined_output.tags.iter().cloned().collect(),
                            };
                            self.response = Some(Arc::new(response));
                            self.verdict = Some(verdict);
                            self.execute_decision_feedback().await;
                        },
                        Err(err) => {
                            // TODO: must perform proper error handling on sender results, sending can fail
                            error!(message = format!("send error: {}", err));
                        },
                    }

                    // Short-circuit if restricted, we can skip the response phase
                    return;
                } else {
                    // In observe-only mode, we still perform decision feedback, but there won't be a response.
                    let verdict = Verdict {
                        decision,
                        outcome,
                        tags: self.combined_output.tags.iter().cloned().collect(),
                    };
                    self.verdict = Some(verdict);

                    // We need to set a response or decision feedback will panic.
                    // This response is what would have been sent if we had blocked, rather than what will actually
                    // be sent, since we're about to call send_allow_request_message and that instructs envoy that
                    // the processor no longer needs to continue processing the request or response.
                    let code: i32 = 403;
                    let body = "Access Denied\n";
                    // The generate function should be infallible here.
                    let response = Self::generate_block_response(code, body).expect("could not generate block response");
                    self.response = Some(Arc::new(response));

                    self.execute_decision_feedback().await;
                }

                let result = Self::send_allow_request_message(self.sender.clone(), end_of_stream).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    error!(message = format!("send error: {}", err));
                }
            },
        }

        // There's only a response phase if we haven't blocked.
        // Observe-only mode should also behave the same way as normal mode here.
        if !restricted {
            match self.prepare_response().await {
                Ok(response) => {
                    self.response = Some(Arc::new(response));
                    self.execute_response_phase().await;
                    self.complete_response_phase().await;
                }
                Err(err) => {
                    error!(message = format!("response error: {}", err));
                }
            }
        }
    }

    async fn complete_response_phase(&mut self) {
        let decision = self.combined_output.decision;
        let outcome = decision
            .outcome(
                self.thresholds.trust,
                self.thresholds.suspicious,
                self.thresholds.restrict,
            )
            .unwrap();

        info!(
            message = "combine decision",
            accept = format_f64!(decision.accept),
            restrict = format_f64!(decision.restrict),
            unknown = format_f64!(decision.unknown),
            score = format_f64!(decision.pignistic().restrict),
            outcome = outcome.to_string(),
            observe_only = self.thresholds.observe_only,
            // array values aren't handled well unfortunately, coercing to comma-separated values seems to be the best option
            tags = self
                .combined_output
                .tags
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>()
                .to_vec()
                .join(","),
        );
        metrics::increment_counter!(
            "plugin_response_phase_decision",
            "outcome" => outcome.to_string(),
            "observe_only" => self.thresholds.observe_only.to_string(),
        );

        let response = self
            .response
            .clone()
            .expect("cannot complete response phase without response");
        let end_of_stream = response.body().is_empty();
        match outcome {
            bulwark_sdk::Outcome::Trusted
            | bulwark_sdk::Outcome::Accepted
            // suspected requests are monitored but not rejected
            | bulwark_sdk::Outcome::Suspected => {
                info!(message = "process response", status = u16::from(response.status()));
                let result = Self::send_allow_response_message(self.sender.clone(), end_of_stream).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    error!(message = format!("send error: {}", err));
                }
            },
            bulwark_sdk::Outcome::Restricted => {
                if !self.thresholds.observe_only {
                    info!(message = "process response", status = 403);
                    let result = Self::send_block_response_message(self.sender.clone()).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        error!(message = format!("send error: {}", err));
                    }
                } else {
                    info!(message = "process response", status = u16::from(response.status()));
                    // Don't receive a body when we would have otherwise blocked if we weren't in monitor-only mode
                    let result = Self::send_allow_response_message(self.sender.clone(), end_of_stream).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        error!(message = format!("send error: {}", err));
                    }
                }
            }
        }

        let verdict = bulwark_sdk::Verdict {
            decision,
            outcome,
            tags: self.combined_output.tags.iter().cloned().collect(),
        };
        self.verdict = Some(verdict);
        self.execute_decision_feedback().await;
    }

    #[instrument(name = "plugin output", skip(self))]
    async fn capture_stdio(&self) {
        // TODO: refactor to process one plugin at a time and try to avoid having handle_decision_feedback join_all
        for plugin_instance in self.plugin_instances.iter() {
            let plugin_instance = plugin_instance.lock().await;
            let stdout = plugin_instance.stdio().stdout_buffer();
            let stderr = plugin_instance.stdio().stderr_buffer();
            if !stdout.is_empty() {
                let stdout = str::from_utf8(&stdout).unwrap();
                for line in stdout.lines() {
                    info!(
                        message = "stdout",
                        plugin = plugin_instance.plugin_reference(),
                        content = line
                    );
                }
            }
            if !stderr.is_empty() {
                let stderr = str::from_utf8(&stderr).unwrap();
                for line in stderr.lines() {
                    error!(
                        message = "stderr",
                        plugin = plugin_instance.plugin_reference(),
                        content = line
                    );
                }
            }
        }
    }

    /// Generates a response indicating the request has been blocked.
    fn generate_block_response(
        code: i32,
        body: &str,
    ) -> Result<bulwark_sdk::Response, http::Error> {
        http::response::Builder::new()
            .status(code as u16)
            .body(bytes::Bytes::from(body.to_string()))
    }

    async fn send_allow_request_message(
        sender: Arc<Mutex<UnboundedSender<Result<ProcessingResponse, tonic::Status>>>>,
        end_of_stream: bool,
    ) -> Result<(), ProcessingMessageError> {
        let mut sender = sender.lock().await;

        trace!("send_allow_request_message (ProcessingResponse)");
        let processing_reply = ProcessingResponse {
            // If the request did not have a body, we're responding to a
            // RequestHeaders message, otherwise we're responding to a
            // RequestBody message.
            response: if end_of_stream {
                Some(processing_response::Response::RequestHeaders(
                    HeadersResponse {
                        response: Some(CommonResponse::default()),
                    },
                ))
            } else {
                Some(processing_response::Response::RequestBody(
                    BodyResponse::default(),
                ))
            },
            ..Default::default()
        };
        Ok(sender.send(Ok(processing_reply)).await?)
    }

    async fn send_block_request_message(
        sender: Arc<Mutex<UnboundedSender<Result<ProcessingResponse, tonic::Status>>>>,
    ) -> Result<bulwark_sdk::Response, ProcessingMessageError> {
        let mut sender = sender.lock().await;

        trace!("send_block_request_message (ProcessingResponse)");
        // TODO: better default response + customizability
        let code: i32 = 403;
        let body = "Access Denied\n";
        let response = Self::generate_block_response(code, body)?;

        let processing_reply = ProcessingResponse {
            response: Some(processing_response::Response::ImmediateResponse(
                ImmediateResponse {
                    status: Some(HttpStatus { code }),
                    // TODO: add decision debug
                    details: "blocked by bulwark".to_string(),
                    body: body.to_string(),
                    headers: None,
                    grpc_status: None,
                },
            )),
            ..Default::default()
        };
        sender.send(Ok(processing_reply)).await?;
        Ok(response)
    }

    async fn send_allow_response_message(
        sender: Arc<Mutex<UnboundedSender<Result<ProcessingResponse, tonic::Status>>>>,
        end_of_stream: bool,
    ) -> Result<(), ProcessingMessageError> {
        let mut sender = sender.lock().await;

        trace!("send_allow_response_message (ProcessingResponse)");
        let processing_reply = ProcessingResponse {
            // If the response did not have a body, we're responding to a
            // ResponseHeaders message, otherwise we're responding to a
            // ResponseBody message.
            response: if end_of_stream {
                Some(processing_response::Response::ResponseHeaders(
                    HeadersResponse::default(),
                ))
            } else {
                Some(processing_response::Response::ResponseBody(
                    BodyResponse::default(),
                ))
            },
            ..Default::default()
        };
        Ok(sender.send(Ok(processing_reply)).await?)
    }

    async fn send_block_response_message(
        sender: Arc<Mutex<UnboundedSender<Result<ProcessingResponse, tonic::Status>>>>,
    ) -> Result<bulwark_sdk::Response, ProcessingMessageError> {
        let mut sender = sender.lock().await;

        trace!("send_block_response_message (ProcessingResponse)");
        // Send back a response indicating the request has been blocked.
        let code: i32 = 403;
        // TODO: better default response + customizability
        let body = "Access Denied\n";
        let response: bulwark_sdk::Response = http::response::Builder::new()
            .status(code as u16)
            .body(bytes::Bytes::from(body))?;
        let processing_reply = ProcessingResponse {
            response: Some(processing_response::Response::ImmediateResponse(
                ImmediateResponse {
                    status: Some(HttpStatus { code }),
                    // TODO: add decision debug
                    details: "blocked by bulwark".to_string(),
                    body: body.to_string(),
                    headers: None,
                    grpc_status: None,
                },
            )),
            ..Default::default()
        };
        sender.send(Ok(processing_reply)).await?;
        Ok(response)
    }

    async fn get_request_header_message(
        stream: Arc<Mutex<Streaming<ProcessingRequest>>>,
    ) -> Result<Option<HttpHeaders>, tonic::Status> {
        let mut stream = stream.lock().await;

        trace!("get_request_header_message (ProcessingRequest)");
        // TODO: if request attributes are eventually supported, we may need to extract both instead of just headers
        if let Some(next_msg) = stream.message().await? {
            if let Some(processing_request::Request::RequestHeaders(hdrs)) = next_msg.request {
                return Ok(Some(hdrs));
            }
        }
        Ok(None)
    }

    async fn send_request_headers_message(
        sender: Arc<Mutex<UnboundedSender<Result<ProcessingResponse, tonic::Status>>>>,
    ) -> Result<(), futures::channel::mpsc::SendError> {
        let mut sender = sender.lock().await;

        trace!("send_request_headers_message (ProcessingResponse)");
        let processing_reply = ProcessingResponse {
            response: Some(processing_response::Response::RequestHeaders(
                HeadersResponse {
                    response: Some(CommonResponse::default()),
                },
            )),
            mode_override: Some(ProcessingMode {
                request_body_mode: processing_mode::BodySendMode::BufferedPartial as i32,
                ..Default::default()
            }),
            ..Default::default()
        };
        sender.send(Ok(processing_reply)).await
    }

    async fn get_request_body_message(
        stream: Arc<Mutex<Streaming<ProcessingRequest>>>,
    ) -> Result<Option<HttpBody>, tonic::Status> {
        let mut stream = stream.lock().await;

        trace!("get_request_body_message (ProcessingRequest)");
        if let Some(next_msg) = stream.message().await? {
            if let Some(processing_request::Request::RequestBody(body)) = next_msg.request {
                return Ok(Some(body));
            }
        }
        Ok(None)
    }

    async fn get_response_headers_message(
        stream: Arc<Mutex<Streaming<ProcessingRequest>>>,
    ) -> Result<Option<HttpHeaders>, tonic::Status> {
        let mut stream = stream.lock().await;

        trace!("get_response_headers_message (ProcessingRequest)");
        if let Some(next_msg) = stream.message().await? {
            if let Some(processing_request::Request::ResponseHeaders(hdrs)) = next_msg.request {
                return Ok(Some(hdrs));
            }
        }
        Ok(None)
    }

    async fn send_response_headers_message(
        sender: Arc<Mutex<UnboundedSender<Result<ProcessingResponse, tonic::Status>>>>,
    ) -> Result<(), futures::channel::mpsc::SendError> {
        let mut sender = sender.lock().await;

        trace!("send_response_headers_message (ProcessingResponse)");
        let processing_reply = ProcessingResponse {
            response: Some(processing_response::Response::ResponseHeaders(
                HeadersResponse::default(),
            )),
            mode_override: Some(ProcessingMode {
                response_body_mode: processing_mode::BodySendMode::BufferedPartial as i32,
                ..Default::default()
            }),
            ..Default::default()
        };
        sender.send(Ok(processing_reply)).await
    }

    async fn get_response_body_message(
        stream: Arc<Mutex<Streaming<ProcessingRequest>>>,
    ) -> Result<Option<HttpBody>, tonic::Status> {
        let mut stream = stream.lock().await;

        trace!("get_response_body_message (ProcessingRequest)");
        if let Some(next_msg) = stream.message().await? {
            if let Some(processing_request::Request::ResponseBody(body)) = next_msg.request {
                return Ok(Some(body));
            }
        }
        Ok(None)
    }

    fn get_header_value<'a>(header_map: &'a Option<HeaderMap>, name: &str) -> Option<&'a str> {
        match header_map {
            Some(headers) => {
                for header in &headers.headers {
                    if header.key == name {
                        return Some(&header.value);
                    }
                }
                None
            }
            None => None,
        }
    }

    fn parse_forwarded_ip(forwarded: &str, proxy_hops: usize) -> Option<IpAddr> {
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

    fn parse_x_forwarded_for_ip(forwarded: &str, proxy_hops: usize) -> Option<IpAddr> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_forwarded() -> Result<(), Box<dyn std::error::Error>> {
        let test_cases = [
            ("", 0, None),
            ("", 1, None),
            ("bogus", 0, None),
            ("213!04]d$7n2;31d4%,hbq#", 0, None),
            (
                "for=192.0.2.43",
                1,
                Some("192.0.2.43".parse::<IpAddr>().unwrap()),
            ),
            (
                "for=192.0.2.43,for=198.51.100.17;by=203.0.113.60;proto=http;host=example.com",
                2,
                Some("192.0.2.43".parse::<IpAddr>().unwrap()),
            ),
            (
                "   for=192.0.2.43, for=198.51.100.17;  by=203.0.113.60; proto=http;host=example.com",
                2,
                Some("192.0.2.43".parse::<IpAddr>().unwrap()),
            ),
            (
                "for=192.0.2.43,for=198.51.100.17;by=203.0.113.60;proto=http;host=example.com",
                1,
                Some("198.51.100.17".parse::<IpAddr>().unwrap()),
            ),
            (
                "for=192.0.2.43,for=198.51.100.17;by=203.0.113.60;proto=http;host=example.com",
                3,
                None,
            ),
        ];

        for (forwarded, hops, expected) in test_cases {
            let parsed = ProcessorContext::parse_forwarded_ip(forwarded, hops);
            assert_eq!(parsed, expected);
        }

        Ok(())
    }

    #[test]
    fn test_parse_x_forwarded_for() -> Result<(), Box<dyn std::error::Error>> {
        let test_cases = [
            ("", 0, None),
            ("", 1, None),
            ("bogus", 0, None),
            ("213!04]d$7n2;31d4%,hbq#", 0, None),
            (
                "192.0.2.43",
                1,
                Some("192.0.2.43".parse::<IpAddr>().unwrap()),
            ),
            (
                "192.0.2.43,198.51.100.17",
                2,
                Some("192.0.2.43".parse::<IpAddr>().unwrap()),
            ),
            (
                "   192.0.2.43, 198.51.100.17 ",
                2,
                Some("192.0.2.43".parse::<IpAddr>().unwrap()),
            ),
            (
                "192.0.2.43,198.51.100.17",
                1,
                Some("198.51.100.17".parse::<IpAddr>().unwrap()),
            ),
            ("192.0.2.43,198.51.100.17", 3, None),
        ];

        for (forwarded, hops, expected) in test_cases {
            let parsed = ProcessorContext::parse_x_forwarded_for_ip(forwarded, hops);
            assert_eq!(parsed, expected);
        }

        Ok(())
    }
}
