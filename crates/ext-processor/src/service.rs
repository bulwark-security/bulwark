//! The service module contains the main Envoy external processor service implementation.

use bulwark_wasm_sdk::Verdict;

use {
    crate::{PluginGroupInstantiationError, ProcessingMessageError, RequestError, ResponseError},
    bulwark_config::Config,
    bulwark_wasm_host::{
        ForwardedIP, HandlerOutput, Plugin, PluginContext, PluginExecutionError, PluginInstance,
        PluginLoadError, RedisInfo, ScriptRegistry,
    },
    bulwark_wasm_sdk::Decision,
    envoy_control_plane::envoy::{
        config::core::v3::{HeaderMap, HeaderValue, HeaderValueOption},
        extensions::filters::http::ext_proc::v3::{processing_mode, ProcessingMode},
        r#type::v3::HttpStatus,
        service::ext_proc::v3::{
            external_processor_server::ExternalProcessor, processing_request, processing_response,
            BodyResponse, CommonResponse, HeaderMutation, HeadersResponse, HttpBody, HttpHeaders,
            ImmediateResponse, ProcessingRequest, ProcessingResponse,
        },
    },
    forwarded_header_value::ForwardedHeaderValue,
    futures::lock::Mutex,
    futures::{channel::mpsc::UnboundedSender, SinkExt, Stream},
    matchit::Router,
    std::{
        collections::{HashMap, HashSet},
        net::IpAddr,
        pin::Pin,
        str,
        str::FromStr,
        sync::Arc,
        time::Duration,
    },
    tokio::{sync::RwLock, sync::Semaphore, task::JoinSet, time::timeout},
    tonic::Streaming,
    tracing::{debug, error, info, instrument, trace, warn, Instrument},
};

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
            Ok(Ok(Ok(params))) => success(params),
            Ok(Ok(Err(err))) => {
                // This error is only logged, not bubbled up
                error!(
                    message = "plugin execution error",
                    elapsed = ?err,
                );
            }
            Ok(Err(err)) => {
                // This error is only logged, not bubbled up
                warn!(
                    message = "timeout on plugin execution",
                    elapsed = ?err,
                );
            }
            Err(err) => {
                // This error is only logged, not bubbled up
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
    redis_info: Option<Arc<RedisInfo>>,
    http_client: Arc<reqwest::blocking::Client>,
    request_semaphore: Arc<tokio::sync::Semaphore>,
    plugin_semaphore: Arc<tokio::sync::Semaphore>,
    thresholds: bulwark_config::Thresholds,
    proxy_hops: usize,
    // TODO: redis circuit breaker for health monitoring
}

#[tonic::async_trait]
impl ExternalProcessor for BulwarkProcessor {
    type ProcessStream = ExternalProcessorStream;

    /// Processes an incoming request, performing all Envoy-specific handling needed by [`bulwark_wasm_host`].
    #[instrument(name = "handle request", skip(self, tonic_request))]
    async fn process(
        &self,
        tonic_request: tonic::Request<Streaming<ProcessingRequest>>,
    ) -> Result<tonic::Response<ExternalProcessorStream>, tonic::Status> {
        // Clone to allow the processor to move into a task
        let bulwark_processor = self.clone();

        let mut stream = tonic_request.into_inner();
        let (mut sender, receiver) = futures::channel::mpsc::unbounded();

        let child_span = tracing::info_span!("route request");
        let permit = self
            .request_semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        tokio::task::spawn(
            async move {
                if let Ok(request) = bulwark_processor
                    .prepare_request(&mut sender, &mut stream)
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
                            // TODO: may want to expose params to logging after redaction
                            let mut params = HashMap::new();
                            for (key, value) in route_match.params.iter() {
                                params.insert(format!("param.{}", key), value.to_string());
                            }

                            let route_target = route_match.value;
                            // TODO: figure out how to bubble the error out of the task and up to the parent
                            // TODO: figure out if tonic-error or some other option is the best way to convert to a tonic Status error
                            // TODO: we probably want to be initializing only when necessary now rather than on every request
                            let plugin_instances = bulwark_processor
                                .instantiate_plugins(&route_target.plugins)
                                .await
                                .unwrap();
                            if let Some(millis) = route_match.value.timeout {
                                timeout_duration = Duration::from_millis(millis);
                            }

                            bulwark_processor
                                .execute_init_phase(plugin_instances.clone(), timeout_duration)
                                .await;

                            let (output, output_by_plugin) = bulwark_processor
                                .execute_request_phase(
                                    plugin_instances.clone(),
                                    request.clone(),
                                    params.clone(),
                                    timeout_duration,
                                )
                                .await;
                            params.extend(output.params.clone());
                            bulwark_processor
                                .complete_request_phase(
                                    sender,
                                    stream,
                                    plugin_instances,
                                    request.clone(),
                                    output,
                                    output_by_plugin,
                                    timeout_duration,
                                )
                                .await;
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
    pub fn new(config: Config) -> Result<Self, PluginLoadError> {
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

        let redis_info = if let Some(remote_state_addr) = config.service.remote_state_uri.as_ref() {
            let pool_size = config.service.remote_state_pool_size;
            // TODO: better error handling instead of unwrap/panic
            let client = redis::Client::open(remote_state_addr.as_str()).unwrap();
            // TODO: make pool size configurable
            Some(Arc::new(RedisInfo {
                // TODO: better error handling instead of unwrap/panic
                pool: r2d2::Pool::builder()
                    .max_size(pool_size)
                    .build(client)
                    .unwrap(),
                registry: ScriptRegistry::default(),
            }))
        } else {
            None
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
                let plugin = Plugin::from_file(plugin_config.path.clone(), plugin_config)?;
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
            redis_info,
            http_client: Arc::new(reqwest::blocking::Client::new()),
            request_semaphore: Arc::new(Semaphore::new(config.runtime.max_concurrent_requests)),
            plugin_semaphore: Arc::new(Semaphore::new(config.runtime.max_plugin_tasks)),
            thresholds: config.thresholds,
            proxy_hops: usize::from(config.service.proxy_hops),
        })
    }

    async fn prepare_request(
        &self,
        sender: &mut UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        stream: &mut Streaming<ProcessingRequest>,
    ) -> Result<bulwark_wasm_sdk::Request, RequestError> {
        if let Some(header_msg) = Self::get_request_header_message(stream).await? {
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
                if let Some(ip_addr) = Self::parse_forwarded_ip(forwarded, self.proxy_hops) {
                    request = request.extension(ForwardedIP(ip_addr));
                }
            } else if let Some(forwarded) =
                Self::get_header_value(&header_msg.headers, "x-forwarded-for")
            {
                if let Some(ip_addr) = Self::parse_x_forwarded_for_ip(forwarded, self.proxy_hops) {
                    request = request.extension(ForwardedIP(ip_addr));
                }
            }

            return Ok(request.body(request_chunk)?);
        }
        Err(RequestError::MissingHeaders)
    }

    async fn prepare_response(
        &self,
        sender: &mut UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        stream: &mut Streaming<ProcessingRequest>,
        version: http::Version,
    ) -> Result<bulwark_wasm_sdk::Response, ResponseError> {
        if let Some(header_msg) = Self::get_response_headers_message(stream).await? {
            // If there is no body, we have to skip these to avoid Envoy errors.
            let body = if !header_msg.end_of_stream {
                // We have to send a reply back before we can retrieve the response body
                Self::send_response_headers_message(sender).await?;

                Self::get_response_body_message(stream)
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
            response = response.status(status).version(version);
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
            let request_context = PluginContext::new(
                plugin.clone(),
                environment,
                self.redis_info.clone(),
                self.http_client.clone(),
            )?;
            plugin_instances.push(Arc::new(Mutex::new(
                PluginInstance::new(plugin.clone(), request_context).await?,
            )));
        }
        Ok(plugin_instances)
    }

    async fn execute_init_phase(
        &self,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
    ) {
        let mut init_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances.iter().cloned() {
            let init_phase_child_span = tracing::info_span!("execute on_init",);
            let permit = self
                .plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            init_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    let result = Self::dispatch_init(plugin_instance).await;
                    drop(permit);
                    result
                })
                .instrument(init_phase_child_span.or_current()),
            );
        }
        join_all(init_phase_tasks, |_| {}).await;
    }

    async fn execute_request_phase(
        &self,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        request: Arc<bulwark_wasm_sdk::Request>,
        params: HashMap<String, String>,
        timeout_duration: std::time::Duration,
    ) -> (HandlerOutput, HashMap<String, HandlerOutput>) {
        // TODO: given all the clones of params, should probably be an Arc
        let params: HashMap<String, String> = params
            .clone()
            .into_iter()
            .chain(
                self.execute_request_enrichment_phase(
                    plugin_instances.clone(),
                    request.clone(),
                    params.clone(),
                    timeout_duration,
                )
                .await,
            )
            .collect();
        let (output, output_by_plugin) = self
            .execute_request_decision_phase(
                plugin_instances,
                request,
                params.clone(),
                timeout_duration,
            )
            .await;
        // Merge params again
        let output = output.extend_params(params);
        (output, output_by_plugin)
    }

    async fn execute_request_enrichment_phase(
        &self,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        request: Arc<bulwark_wasm_sdk::Request>,
        params: HashMap<String, String>,
        timeout_duration: std::time::Duration,
    ) -> HashMap<String, String> {
        let mut enrichment_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances.iter().cloned() {
            let enrichment_phase_child_span = tracing::info_span!("execute on_request",);
            let permit = self
                .plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            let request = request.clone();
            let params = params.clone();
            enrichment_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    let result =
                        Self::dispatch_request_enrichment(plugin_instance, request, params).await;
                    drop(permit);
                    result
                })
                .instrument(enrichment_phase_child_span.or_current()),
            );
        }

        let mut params = params.clone();
        join_all(enrichment_phase_tasks, |new_params| {
            // Merge params from each plugin
            params.extend(new_params);
        })
        .await;
        params
    }

    async fn execute_request_decision_phase(
        &self,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        request: Arc<bulwark_wasm_sdk::Request>,
        params: HashMap<String, String>,
        timeout_duration: std::time::Duration,
    ) -> (HandlerOutput, HashMap<String, HandlerOutput>) {
        let outputs: Arc<Mutex<Vec<HandlerOutput>>> =
            Arc::new(Mutex::new(Vec::with_capacity(plugin_instances.len())));
        let output_by_plugin: Arc<Mutex<HashMap<String, HandlerOutput>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let mut decision_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances {
            let decision_phase_child_span = tracing::info_span!("execute on_request_decision",);
            let permit = self
                .plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            let outputs = outputs.clone();
            let output_by_plugin = output_by_plugin.clone();
            let request = request.clone();
            let params = params.clone();
            decision_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    let output_result =
                        Self::dispatch_request_decision(plugin_instance.clone(), request, params)
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
                            accept = decision.accept,
                            restrict = decision.restrict,
                            unknown = decision.unknown,
                            score = decision.pignistic().restrict,
                            weight = plugin_instance.weight(),
                        );

                        let mut outputs = outputs.lock().await;
                        outputs.push(output.clone());
                        let mut output_by_plugin = output_by_plugin.lock().await;
                        output_by_plugin.insert(plugin_instance.plugin_reference(), output);
                    } else if let Err(err) = &output_result {
                        error!(message = "plugin error", error = err.to_string());
                        let mut outputs = outputs.lock().await;
                        outputs.push(HandlerOutput {
                            decision: bulwark_wasm_sdk::UNKNOWN,
                            tags: HashSet::from([String::from("error")]),
                            params: HashMap::new(),
                        });
                    }
                    drop(permit);
                    output_result.map(|output| output.params)
                })
                .instrument(decision_phase_child_span.or_current()),
            );
        }

        let mut params = params.clone();
        join_all(decision_phase_tasks, |new_params| {
            // Merge params from each plugin
            params.extend(new_params);
        })
        .await;

        let decision_vec: Vec<Decision>;
        let tags: HashSet<String>;
        {
            let outputs = outputs.lock().await;
            decision_vec = outputs.iter().map(|dc| dc.decision).collect();
            tags = outputs.iter().flat_map(|dc| dc.tags.clone()).collect();
        }
        let decision = Decision::combine_murphy(&decision_vec);

        let output_by_plugin = output_by_plugin.lock().await;
        (
            HandlerOutput {
                decision,
                tags: tags.into_iter().collect(),
                params,
            },
            output_by_plugin.clone(),
        )
    }

    async fn execute_response_phase(
        &self,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        request: Arc<bulwark_wasm_sdk::Request>,
        response: Arc<bulwark_wasm_sdk::Response>,
        params: HashMap<String, String>,
        output_by_plugin: HashMap<String, HandlerOutput>,
        timeout_duration: std::time::Duration,
    ) -> (HandlerOutput, HashMap<String, HandlerOutput>) {
        let outputs: Arc<Mutex<Vec<HandlerOutput>>> =
            Arc::new(Mutex::new(Vec::with_capacity(plugin_instances.len())));
        let new_output_by_plugin: Arc<Mutex<HashMap<String, HandlerOutput>>> =
            Arc::new(Mutex::new(output_by_plugin.clone()));
        let mut response_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances {
            let response_phase_child_span = tracing::info_span!("execute on_response_decision",);
            let permit = self
                .plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            let request = request.clone();
            let response = response.clone();
            let params = params.clone();
            let outputs = outputs.clone();
            let new_output_by_plugin = new_output_by_plugin.clone();
            let prior_output_by_plugin = output_by_plugin
                .get(&plugin_instance.lock().await.plugin_reference())
                .map(|h| h.clone());
            response_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    let output_result = Self::dispatch_response_decision(
                        plugin_instance.clone(),
                        request,
                        response,
                        params,
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
                            accept = decision.accept,
                            restrict = decision.restrict,
                            unknown = decision.unknown,
                            score = decision.pignistic().restrict,
                            weight = plugin_instance.weight(),
                        );

                        if let Some(prior_output_by_plugin) = prior_output_by_plugin {
                            // If the prior output was non-zero and the new output was zero, then keep the prior output.
                            if !prior_output_by_plugin.decision.is_unknown()
                                && output.decision.is_unknown()
                            {
                                // The prior decision was already weighted and does not need to have weights applied.
                                output.decision = prior_output_by_plugin.decision;
                            }
                        }

                        let mut outputs = outputs.lock().await;
                        outputs.push(output.clone());
                        let mut new_output_by_plugin = new_output_by_plugin.lock().await;
                        new_output_by_plugin.insert(plugin_instance.plugin_reference(), output);
                    } else if let Err(err) = &output_result {
                        error!(message = "plugin error", error = err.to_string());
                        let mut outputs = outputs.lock().await;
                        outputs.push(HandlerOutput {
                            decision: bulwark_wasm_sdk::UNKNOWN,
                            tags: HashSet::from([String::from("error")]),
                            params: HashMap::new(),
                        });
                    }
                    drop(permit);
                    output_result.map(|output| output.params)
                })
                .instrument(response_phase_child_span.or_current()),
            );
        }

        let mut params = params.clone();
        join_all(response_phase_tasks, |new_params| {
            // Merge params from each plugin
            params.extend(new_params);
        })
        .await;

        let decision_vec: Vec<Decision>;
        let tags: HashSet<String>;
        {
            let outputs = outputs.lock().await;
            decision_vec = outputs.iter().map(|dc| dc.decision).collect();
            tags = outputs.iter().flat_map(|dc| dc.tags.clone()).collect();
        }
        let decision = Decision::combine_murphy(&decision_vec);

        let new_output_by_plugin = new_output_by_plugin.lock().await;
        (
            HandlerOutput {
                decision,
                tags: tags.into_iter().collect(),
                params,
            },
            new_output_by_plugin.clone(),
        )
    }

    async fn execute_decision_feedback(
        &self,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        request: Arc<bulwark_wasm_sdk::Request>,
        response: Arc<bulwark_wasm_sdk::Response>,
        params: HashMap<String, String>,
        verdict: Verdict,
        output_by_plugin: HashMap<String, HandlerOutput>,
        timeout_duration: std::time::Duration,
    ) {
        metrics::increment_counter!(
            "combined_decision",
            "outcome" => verdict.outcome.to_string(),
        );
        metrics::histogram!(
            "combined_decision_score",
            verdict.decision.pignistic().restrict
        );

        let mut decisions: Vec<Decision> = Vec::with_capacity(plugin_instances.len());
        let mut feedback_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances.iter().cloned() {
            let response_phase_child_span = tracing::info_span!("execute on_decision_feedback",);
            let permit = self
                .plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            {
                // Make sure the plugin instance knows about the final combined decision
                let plugin_instance = plugin_instance.lock().await;
                let decision = output_by_plugin
                    .get(&plugin_instance.plugin_reference())
                    .expect("missing plugin output")
                    .decision;
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
            let request = request.clone();
            let response = response.clone();
            let params = params.clone();
            let verdict = verdict.clone();
            feedback_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    let result = Self::dispatch_decision_feedback(
                        plugin_instance,
                        request,
                        response,
                        params,
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
        Self::capture_stdio(plugin_instances).await;
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
        request: Arc<bulwark_wasm_sdk::Request>,
        params: HashMap<String, String>,
    ) -> Result<HashMap<String, String>, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance
            .handle_request_enrichment(request, params)
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
        request: Arc<bulwark_wasm_sdk::Request>,
        params: HashMap<String, String>,
    ) -> Result<HandlerOutput, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance
            .handle_request_decision(request, params)
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
        request: Arc<bulwark_wasm_sdk::Request>,
        response: Arc<bulwark_wasm_sdk::Response>,
        params: HashMap<String, String>,
    ) -> Result<HandlerOutput, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance
            .handle_response_decision(request, response, params)
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
        request: Arc<bulwark_wasm_sdk::Request>,
        response: Arc<bulwark_wasm_sdk::Response>,
        params: HashMap<String, String>,
        verdict: bulwark_wasm_sdk::Verdict,
    ) -> Result<(), PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance
            .handle_decision_feedback(request, response, params, verdict)
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

    async fn complete_request_phase(
        &self,
        mut sender: UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        mut stream: Streaming<ProcessingRequest>,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        request: Arc<bulwark_wasm_sdk::Request>,
        output: HandlerOutput,
        output_by_plugin: HashMap<String, HandlerOutput>,
        timeout_duration: std::time::Duration,
    ) {
        let decision = output.decision;
        let outcome = decision
            .outcome(
                self.thresholds.trust,
                self.thresholds.suspicious,
                self.thresholds.restrict,
            )
            .unwrap();

        info!(
            message = "combine decision",
            accept = decision.accept,
            restrict = decision.restrict,
            unknown = decision.unknown,
            score = decision.pignistic().restrict,
            outcome = outcome.to_string(),
            observe_only = self.thresholds.observe_only,
            // array values aren't handled well unfortunately, coercing to comma-separated values seems to be the best option
            tags = output
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
        let end_of_stream = request.body().len() == 0;
        match outcome {
            bulwark_wasm_sdk::Outcome::Trusted
            | bulwark_wasm_sdk::Outcome::Accepted
            // suspected requests are monitored but not rejected
            | bulwark_wasm_sdk::Outcome::Suspected => {
                let result = Self::send_allow_request_message(&sender, end_of_stream).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    error!(message = format!("send error: {}", err));
                }
            },
            bulwark_wasm_sdk::Outcome::Restricted => {
                restricted = true;
                if !self.thresholds.observe_only {
                    info!(message = "process response", status = 403);
                    match Self::send_block_request_message(&sender).await {
                        Ok(response) => {
                            // Normally we initiate feedback after the response phase, but if we skip the response phase
                            // we need to do it here instead.
                            let verdict = bulwark_wasm_sdk::Verdict {
                                decision,
                                outcome,
                                tags: output.tags.iter().cloned().collect(),
                            };
                            self.execute_decision_feedback(
                                plugin_instances.clone(),
                                request,
                                Arc::new(response),
                                output.params,
                                verdict,
                                output_by_plugin,
                                timeout_duration,
                            ).await;
                        },
                        Err(err) => {
                            // TODO: must perform proper error handling on sender results, sending can fail
                            error!(message = format!("send error: {}", err));
                        },
                    }

                    // Short-circuit if restricted, we can skip the response phase
                    return;
                }

                // Don't receive a body when we would have otherwise blocked if we weren't in monitor-only mode
                let result = Self::send_allow_request_message(&sender, end_of_stream).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    error!(message = format!("send error: {}", err));
                }
            },
        }

        // There's only a response phase if we haven't blocked.
        // Observe-only mode should also behave the same way as normal mode here.
        if !restricted {
            match self
                .prepare_response(&mut sender, &mut stream, request.version())
                .await
            {
                Ok(response) => {
                    let response = Arc::new(response);
                    let (output, output_by_plugin) = self
                        .execute_response_phase(
                            plugin_instances.clone(),
                            request.clone(),
                            response.clone(),
                            output.params,
                            output_by_plugin,
                            timeout_duration,
                        )
                        .await;
                    self.complete_response_phase(
                        &mut sender,
                        plugin_instances,
                        request,
                        response,
                        output,
                        output_by_plugin,
                        timeout_duration,
                    )
                    .await;
                }
                Err(err) => {
                    error!(message = format!("response error: {}", err));
                }
            }
        }
    }

    async fn complete_response_phase(
        &self,
        sender: &mut UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        request: Arc<bulwark_wasm_sdk::Request>,
        response: Arc<bulwark_wasm_sdk::Response>,
        output: HandlerOutput,
        output_by_plugin: HashMap<String, HandlerOutput>,
        timeout_duration: std::time::Duration,
    ) {
        let decision = output.decision;
        let outcome = decision
            .outcome(
                self.thresholds.trust,
                self.thresholds.suspicious,
                self.thresholds.restrict,
            )
            .unwrap();

        info!(
            message = "combine decision",
            accept = decision.accept,
            restrict = decision.restrict,
            unknown = decision.unknown,
            score = decision.pignistic().restrict,
            outcome = outcome.to_string(),
            observe_only = self.thresholds.observe_only,
            // array values aren't handled well unfortunately, coercing to comma-separated values seems to be the best option
            tags = output
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

        let end_of_stream = response.body().is_empty();
        match outcome {
            bulwark_wasm_sdk::Outcome::Trusted
            | bulwark_wasm_sdk::Outcome::Accepted
            // suspected requests are monitored but not rejected
            | bulwark_wasm_sdk::Outcome::Suspected => {
                info!(message = "process response", status = u16::from(response.status()));
                let result = Self::send_allow_response_message(sender, end_of_stream).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    error!(message = format!("send error: {}", err));
                }
            },
            bulwark_wasm_sdk::Outcome::Restricted => {
                if !self.thresholds.observe_only {
                    info!(message = "process response", status = 403);
                    let result = Self::send_block_response_message(sender).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        error!(message = format!("send error: {}", err));
                    }
                } else {
                    info!(message = "process response", status = u16::from(response.status()));
                    // Don't receive a body when we would have otherwise blocked if we weren't in monitor-only mode
                    let result = Self::send_allow_response_message(sender, end_of_stream).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        error!(message = format!("send error: {}", err));
                    }
                }
            }
        }

        let verdict = bulwark_wasm_sdk::Verdict {
            decision,
            outcome,
            tags: output.tags.iter().cloned().collect(),
        };
        self.execute_decision_feedback(
            plugin_instances.clone(),
            request,
            response,
            output.params,
            verdict,
            output_by_plugin,
            timeout_duration,
        )
        .await;
    }

    #[instrument(name = "plugin output", skip(plugin_instances))]
    async fn capture_stdio(plugin_instances: Vec<Arc<Mutex<PluginInstance>>>) {
        // TODO: refactor to process one plugin at a time and try to avoid having handle_decision_feedback join_all
        for plugin_instance in plugin_instances {
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

    async fn send_allow_request_message(
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        end_of_stream: bool,
    ) -> Result<(), ProcessingMessageError> {
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
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
    ) -> Result<bulwark_wasm_sdk::Response, ProcessingMessageError> {
        trace!("send_block_request_message (ProcessingResponse)");
        // Send back a response indicating the request has been blocked.
        let code: i32 = 403;
        // TODO: better default response + customizability
        let body = "Access Denied\n";
        let response: bulwark_wasm_sdk::Response = http::response::Builder::new()
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

    async fn send_allow_response_message(
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        end_of_stream: bool,
    ) -> Result<(), ProcessingMessageError> {
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
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
    ) -> Result<bulwark_wasm_sdk::Response, ProcessingMessageError> {
        trace!("send_block_response_message (ProcessingResponse)");
        // Send back a response indicating the request has been blocked.
        let code: i32 = 403;
        // TODO: better default response + customizability
        let body = "Access Denied\n";
        let response: bulwark_wasm_sdk::Response = http::response::Builder::new()
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
        stream: &mut Streaming<ProcessingRequest>,
    ) -> Result<Option<HttpHeaders>, tonic::Status> {
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
        sender: &mut UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
    ) -> Result<(), futures::channel::mpsc::SendError> {
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
        stream: &mut Streaming<ProcessingRequest>,
    ) -> Result<Option<HttpBody>, tonic::Status> {
        trace!("get_request_body_message (ProcessingRequest)");
        if let Some(next_msg) = stream.message().await? {
            if let Some(processing_request::Request::RequestBody(body)) = next_msg.request {
                return Ok(Some(body));
            }
        }
        Ok(None)
    }

    async fn get_response_headers_message(
        stream: &mut Streaming<ProcessingRequest>,
    ) -> Result<Option<HttpHeaders>, tonic::Status> {
        trace!("get_response_headers_message (ProcessingRequest)");
        if let Some(next_msg) = stream.message().await? {
            if let Some(processing_request::Request::ResponseHeaders(hdrs)) = next_msg.request {
                return Ok(Some(hdrs));
            }
        }
        Ok(None)
    }

    async fn send_response_headers_message(
        sender: &mut UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
    ) -> Result<(), futures::channel::mpsc::SendError> {
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
        stream: &mut Streaming<ProcessingRequest>,
    ) -> Result<Option<HttpBody>, tonic::Status> {
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

    fn insert_header(cr: &mut CommonResponse, key: &str, value: &str) {
        let new_header = HeaderValueOption {
            header: Some(HeaderValue {
                key: key.into(),
                value: value.into(),
            }),
            ..Default::default()
        };
        match &mut cr.header_mutation {
            Some(hm) => hm.set_headers.push(new_header),
            None => {
                let mut new_hm = HeaderMutation::default();
                new_hm.set_headers.push(new_header);
                cr.header_mutation = Some(new_hm);
            }
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
            let parsed = BulwarkProcessor::parse_forwarded_ip(forwarded, hops);
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
            let parsed = BulwarkProcessor::parse_x_forwarded_for_ip(forwarded, hops);
            assert_eq!(parsed, expected);
        }

        Ok(())
    }
}
