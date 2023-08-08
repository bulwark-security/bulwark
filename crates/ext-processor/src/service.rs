//! The service module contains the main Envoy external processor service implementation.

use {
    crate::{
        serialize_decision_sfv, serialize_tags_sfv, PluginGroupInstantiationError,
        ProcessingMessageError, RequestError, ResponseError, SfvError,
    },
    bulwark_config::{Config, Thresholds},
    bulwark_wasm_host::{
        DecisionComponents, ForwardedIP, Plugin, PluginExecutionError, PluginInstance,
        PluginLoadError, RedisInfo, RequestContext, ScriptRegistry,
    },
    bulwark_wasm_sdk::{BodyChunk, Decision},
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
        collections::HashSet, net::IpAddr, pin::Pin, str, str::FromStr, sync::Arc, time::Duration,
    },
    tokio::{sync::RwLock, sync::Semaphore, task::JoinSet, time::timeout},
    tonic::Streaming,
    tracing::{debug, error, info, instrument, warn, Instrument},
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
async fn join_all(mut join_set: JoinSet<Result<(), tokio::time::error::Elapsed>>) {
    // efficiently hand execution off to the the tasks we're joining
    tokio::task::yield_now().await;

    while let Some(r) = join_set.join_next().await {
        match r {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                warn!(
                    message = "timeout on plugin execution",
                    elapsed = ?e,
                );
            }
            Err(e) => {
                warn!(
                    message = "join error on plugin execution",
                    error_message = ?e,
                );
            }
        }
    }
}

/// The `BulwarkProcessor` implements the primary envoy processing service logic via the [`ExternalProcessor`] trait.
///
/// The [`process`](BulwarkProcessor::process) function is the main request handler.
pub struct BulwarkProcessor {
    // TODO: may need to have a plugin registry at some point
    router: Arc<RwLock<Router<RouteTarget>>>,
    redis_info: Option<Arc<RedisInfo>>,
    http_client: Arc<reqwest::blocking::Client>,
    request_semaphore: Arc<tokio::sync::Semaphore>,
    plugin_semaphore: Arc<tokio::sync::Semaphore>,
    thresholds: bulwark_config::Thresholds,
    hops: usize,
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
        let mut stream = tonic_request.into_inner();
        let thresholds = self.thresholds;
        if let Ok(http_req) = Self::prepare_request(&mut stream, self.hops).await {
            let redis_info = self.redis_info.clone();
            let http_req = Arc::new(http_req);
            let router = self.router.clone();

            info!(
                message = "process request",
                method = http_req.method().to_string(),
                uri = http_req.uri().to_string(),
                user_agent = http_req
                    .headers()
                    .get("User-Agent")
                    .map(|ua: &http::HeaderValue| ua.to_str().unwrap_or_default())
            );

            let child_span = tracing::info_span!("route request");
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            let http_client = self.http_client.clone();
            let request_semaphore = self.request_semaphore.clone();
            let plugin_semaphore = self.plugin_semaphore.clone();
            let permit = request_semaphore
                .acquire_owned()
                .await
                .expect("semaphore closed");
            tokio::task::spawn(
                async move {
                    let http_req = http_req.clone();
                    let router = router.read().await;
                    let route_result = router.at(http_req.uri().path());
                    // TODO: router needs to point to a struct that bundles the plugin set and associated config like timeout duration
                    // TODO: put default timeout in a constant somewhere central
                    let mut timeout_duration = Duration::from_millis(10);
                    match route_result {
                        Ok(route_match) => {
                            // TODO: may want to expose params to logging after redaction
                            let route_target = route_match.value;
                            // TODO: figure out how to bubble the error out of the task and up to the parent
                            // TODO: figure out if tonic-error or some other option is the best way to convert to a tonic Status error
                            let plugin_instances = Self::instantiate_plugins(
                                &route_target.plugins,
                                redis_info.clone(),
                                http_client,
                                http_req.clone(),
                                route_match.params,
                            )
                            .await
                            .unwrap();
                            if let Some(millis) = route_match.value.timeout {
                                timeout_duration = Duration::from_millis(millis);
                            }

                            Self::execute_init_phase(
                                plugin_instances.clone(),
                                timeout_duration,
                                plugin_semaphore.clone(),
                            )
                            .await;

                            let combined = Self::execute_request_phase(
                                plugin_instances.clone(),
                                timeout_duration,
                                plugin_semaphore.clone(),
                            )
                            .await;

                            Self::handle_request_phase_decision(
                                sender,
                                stream,
                                http_req.clone(),
                                combined,
                                thresholds,
                                plugin_instances,
                                timeout_duration,
                                plugin_semaphore,
                            )
                            .await;
                        }
                        Err(_) => {
                            // TODO: figure out how best to handle trailing slash errors, silent failure is probably undesirable
                            error!(uri = http_req.uri().to_string(), message = "match error");
                            // TODO: panic is undesirable, need to figure out if we should be returning a Status or changing the response or doing something else
                            panic!("match error");
                        }
                    };
                    drop(permit);
                }
                .instrument(child_span.or_current()),
            );
            return Ok(tonic::Response::new(Box::pin(receiver)));
        }
        // By default, just close the stream.
        Ok(tonic::Response::new(Box::pin(futures::stream::empty())))
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
            hops: usize::from(config.service.proxy_hops),
        })
    }

    async fn prepare_request(
        stream: &mut Streaming<ProcessingRequest>,
        proxy_hops: usize,
    ) -> Result<bulwark_wasm_sdk::Request, RequestError> {
        if let Some(header_msg) = Self::get_request_headers(stream).await {
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
            let request_chunk = if header_msg.end_of_stream {
                bulwark_wasm_sdk::NO_BODY
            } else {
                // Have to obtain this from envoy later if we need it
                bulwark_wasm_sdk::UNAVAILABLE_BODY
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

    async fn merge_request_body(
        http_req: Arc<bulwark_wasm_sdk::Request>,
        stream: &mut Streaming<ProcessingRequest>,
    ) -> Arc<bulwark_wasm_sdk::Request> {
        if let Some(body_msg) = Self::get_request_body(stream).await {
            let mut merged_req = http::Request::builder();
            let request_chunk = BodyChunk {
                received: true,
                start: 0,
                size: body_msg.body.len() as u64,
                end_of_stream: body_msg.end_of_stream,
                content: body_msg.body,
            };
            merged_req = merged_req
                .method(http_req.method())
                .uri(http_req.uri())
                .version(http_req.version());
            for (name, value) in http_req.headers() {
                merged_req = merged_req.header(name, value);
            }
            if let Some(forwarded_ext) = http_req.extensions().get::<ForwardedIP>().copied() {
                merged_req = merged_req.extension(forwarded_ext);
            }

            return Arc::new(
                merged_req
                    .body(request_chunk)
                    .expect("request should already have validated"),
            );
        }
        // No body received, use the original request
        http_req
    }

    async fn prepare_response(
        stream: &mut Streaming<ProcessingRequest>,
        version: http::Version,
    ) -> Result<bulwark_wasm_sdk::Response, ResponseError> {
        if let Some(header_msg) = Self::get_response_headers(stream).await {
            let status = Self::get_header_value(&header_msg.headers, ":status")
                .ok_or(ResponseError::MissingStatus)?;

            let mut response = http::Response::builder();
            let response_chunk = if header_msg.end_of_stream {
                bulwark_wasm_sdk::NO_BODY
            } else {
                // Have to obtain this from envoy later if we need it
                bulwark_wasm_sdk::UNAVAILABLE_BODY
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

    async fn merge_response_body(
        http_resp: Arc<bulwark_wasm_sdk::Response>,
        stream: &mut Streaming<ProcessingRequest>,
    ) -> Arc<bulwark_wasm_sdk::Response> {
        if let Some(body_msg) = Self::get_response_body(stream).await {
            let mut merged_resp = http::Response::builder();
            let response_chunk = BodyChunk {
                received: true,
                start: 0,
                size: body_msg.body.len() as u64,
                end_of_stream: body_msg.end_of_stream,
                content: body_msg.body,
            };
            merged_resp = merged_resp
                .status(http_resp.status())
                .version(http_resp.version());
            for (name, value) in http_resp.headers() {
                merged_resp = merged_resp.header(name, value);
            }
            return Arc::new(
                merged_resp
                    .body(response_chunk)
                    .expect("response should already have validated"),
            );
        }
        // No body received, use the original request
        http_resp
    }

    async fn instantiate_plugins(
        plugins: &PluginList,
        redis_info: Option<Arc<RedisInfo>>,
        http_client: Arc<reqwest::blocking::Client>,
        http_req: Arc<bulwark_wasm_sdk::Request>,
        params: matchit::Params<'_, '_>,
    ) -> Result<Vec<Arc<Mutex<PluginInstance>>>, PluginGroupInstantiationError> {
        let mut plugin_instances = Vec::with_capacity(plugins.len());
        let mut shared_params = bulwark_wasm_sdk::Map::new();
        for (key, value) in params.iter() {
            let wrapped_value = bulwark_wasm_sdk::Value::String(value.to_string());
            shared_params.insert(format!("param.{}", key), wrapped_value);
        }
        // Host environment needs a sync mutex
        let shared_params = Arc::new(std::sync::Mutex::new(shared_params));
        for plugin in plugins {
            let request_context = RequestContext::new(
                plugin.clone(),
                redis_info.clone(),
                http_client.clone(),
                shared_params.clone(),
                http_req.clone(),
            )?;
            plugin_instances.push(Arc::new(Mutex::new(
                PluginInstance::new(plugin.clone(), request_context).await?,
            )));
        }
        Ok(plugin_instances)
    }

    async fn execute_init_phase(
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) {
        let mut init_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances.iter().cloned() {
            let init_phase_child_span = tracing::info_span!("execute on_init",);
            let permit = plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            init_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    // TODO: avoid unwraps
                    Self::execute_on_init(plugin_instance).await.unwrap();
                    drop(permit);
                })
                .instrument(init_phase_child_span.or_current()),
            );
        }
        join_all(init_phase_tasks).await;
    }

    async fn execute_request_phase(
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) -> DecisionComponents {
        Self::execute_request_enrichment_phase(
            plugin_instances.clone(),
            timeout_duration,
            plugin_semaphore.clone(),
        )
        .await;
        Self::execute_request_decision_phase(plugin_instances, timeout_duration, plugin_semaphore)
            .await
    }

    async fn execute_request_enrichment_phase(
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) {
        let mut enrichment_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances.iter().cloned() {
            let enrichment_phase_child_span = tracing::info_span!("execute on_request",);
            let permit = plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            enrichment_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    // TODO: avoid unwraps
                    Self::execute_on_request(plugin_instance).await.unwrap();
                    drop(permit);
                })
                .instrument(enrichment_phase_child_span.or_current()),
            );
        }
        join_all(enrichment_phase_tasks).await;
    }

    async fn execute_request_decision_phase(
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) -> DecisionComponents {
        let decision_components = Arc::new(Mutex::new(Vec::with_capacity(plugin_instances.len())));
        let mut decision_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances {
            let decision_phase_child_span = tracing::info_span!("execute on_request_decision",);
            let permit = plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            let decision_components = decision_components.clone();
            decision_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    let decision_result =
                        Self::execute_on_request_decision(plugin_instance.clone()).await;
                    if let Ok(mut decision_component) = decision_result {
                        {
                            // Re-weight the decision based on its weighting value from the configuration
                            let plugin_instance = plugin_instance.lock().await;
                            decision_component.decision =
                                decision_component.decision.weight(plugin_instance.weight());

                            let decision = &decision_component.decision;
                            info!(
                                message = "plugin decision",
                                name = plugin_instance.plugin_reference(),
                                accept = decision.accept,
                                restrict = decision.restrict,
                                unknown = decision.unknown,
                                score = decision.pignistic().restrict,
                            );
                        }
                        let mut decision_components = decision_components.lock().await;
                        decision_components.push(decision_component);
                    } else if let Err(err) = decision_result {
                        info!(message = "plugin error", error = err.to_string());
                        let mut decision_components = decision_components.lock().await;
                        decision_components.push(DecisionComponents {
                            decision: bulwark_wasm_sdk::UNKNOWN,
                            tags: vec![String::from("error")],
                        });
                    }
                    drop(permit);
                })
                .instrument(decision_phase_child_span.or_current()),
            );
        }
        join_all(decision_phase_tasks).await;

        let decision_vec: Vec<Decision>;
        let tags: HashSet<String>;
        {
            let decision_components = decision_components.lock().await;
            decision_vec = decision_components.iter().map(|dc| dc.decision).collect();
            tags = decision_components
                .iter()
                .flat_map(|dc| dc.tags.clone())
                .collect();
        }
        let decision = Decision::combine_murphy(&decision_vec);

        DecisionComponents {
            decision,
            tags: tags.into_iter().collect(),
        }
    }

    async fn execute_request_body_phase(
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        request: Arc<bulwark_wasm_sdk::Request>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) -> DecisionComponents {
        let decision_components = Arc::new(Mutex::new(Vec::with_capacity(plugin_instances.len())));
        let mut decision_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances {
            let decision_phase_child_span =
                tracing::info_span!("execute on_request_body_decision",);
            let permit = plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            {
                // Update plugin instances with the new request and its body
                let mut plugin_instance = plugin_instance.lock().await;
                let request = request.clone();
                plugin_instance.record_request(request);
            }
            let decision_components = decision_components.clone();
            decision_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    let decision_result =
                        Self::execute_on_request_body_decision(plugin_instance.clone()).await;
                    if let Ok(mut decision_component) = decision_result {
                        {
                            // Re-weight the decision based on its weighting value from the configuration
                            let plugin_instance = plugin_instance.lock().await;
                            decision_component.decision =
                                decision_component.decision.weight(plugin_instance.weight());

                            let decision = &decision_component.decision;
                            info!(
                                message = "plugin decision",
                                name = plugin_instance.plugin_reference(),
                                accept = decision.accept,
                                restrict = decision.restrict,
                                unknown = decision.unknown,
                                score = decision.pignistic().restrict,
                            );
                        }
                        let mut decision_components = decision_components.lock().await;
                        decision_components.push(decision_component);
                    } else if let Err(err) = decision_result {
                        info!(message = "plugin error", error = err.to_string());
                        let mut decision_components = decision_components.lock().await;
                        decision_components.push(DecisionComponents {
                            decision: bulwark_wasm_sdk::UNKNOWN,
                            tags: vec![String::from("error")],
                        });
                    }
                    drop(permit);
                })
                .instrument(decision_phase_child_span.or_current()),
            );
        }
        join_all(decision_phase_tasks).await;

        let decision_vec: Vec<Decision>;
        let tags: HashSet<String>;
        {
            let decision_components = decision_components.lock().await;
            decision_vec = decision_components.iter().map(|dc| dc.decision).collect();
            tags = decision_components
                .iter()
                .flat_map(|dc| dc.tags.clone())
                .collect();
        }
        let decision = Decision::combine_murphy(&decision_vec);

        DecisionComponents {
            decision,
            tags: tags.into_iter().collect(),
        }
    }

    async fn execute_response_phase(
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        response: Arc<http::Response<BodyChunk>>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) -> DecisionComponents {
        let decision_components = Arc::new(Mutex::new(Vec::with_capacity(plugin_instances.len())));
        let mut response_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances {
            let response_phase_child_span = tracing::info_span!("execute on_response_decision",);
            let permit = plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            {
                // Make sure the plugin instance knows about the response
                let mut plugin_instance = plugin_instance.lock().await;
                let response = response.clone();
                plugin_instance.record_response(response);
            }
            let decision_components = decision_components.clone();
            response_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    let decision_result =
                        Self::execute_on_response_decision(plugin_instance.clone()).await;
                    if let Ok(mut decision_component) = decision_result {
                        {
                            // Re-weight the decision based on its weighting value from the configuration
                            let plugin_instance = plugin_instance.lock().await;
                            decision_component.decision =
                                decision_component.decision.weight(plugin_instance.weight());

                            let decision = &decision_component.decision;
                            info!(
                                message = "plugin decision",
                                name = plugin_instance.plugin_reference(),
                                accept = decision.accept,
                                restrict = decision.restrict,
                                unknown = decision.unknown,
                                score = decision.pignistic().restrict,
                            );
                        }
                        let mut decision_components = decision_components.lock().await;
                        decision_components.push(decision_component);
                    } else if let Err(err) = decision_result {
                        info!(message = "plugin error", error = err.to_string());
                        let mut decision_components = decision_components.lock().await;
                        decision_components.push(DecisionComponents {
                            decision: bulwark_wasm_sdk::UNKNOWN,
                            tags: vec![String::from("error")],
                        });
                    }
                    drop(permit);
                })
                .instrument(response_phase_child_span.or_current()),
            );
        }
        join_all(response_phase_tasks).await;

        let decision_vec: Vec<Decision>;
        let tags: HashSet<String>;
        {
            let decision_components = decision_components.lock().await;
            decision_vec = decision_components.iter().map(|dc| dc.decision).collect();
            tags = decision_components
                .iter()
                .flat_map(|dc| dc.tags.clone())
                .collect();
        }
        let decision = Decision::combine_murphy(&decision_vec);

        DecisionComponents {
            decision,
            tags: tags.into_iter().collect(),
        }
    }

    async fn execute_response_body_phase(
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        response: Arc<http::Response<BodyChunk>>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) -> DecisionComponents {
        let decision_components = Arc::new(Mutex::new(Vec::with_capacity(plugin_instances.len())));
        let mut response_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances {
            let response_phase_child_span =
                tracing::info_span!("execute on_response_body_decision",);
            let permit = plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            {
                // Make sure the plugin instance knows about the updated response
                let mut plugin_instance = plugin_instance.lock().await;
                let response = response.clone();
                plugin_instance.record_response(response);
            }
            let decision_components = decision_components.clone();
            response_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    let decision_result =
                        Self::execute_on_response_body_decision(plugin_instance.clone()).await;
                    if let Ok(mut decision_component) = decision_result {
                        {
                            // Re-weight the decision based on its weighting value from the configuration
                            let plugin_instance = plugin_instance.lock().await;
                            decision_component.decision =
                                decision_component.decision.weight(plugin_instance.weight());

                            let decision = &decision_component.decision;
                            info!(
                                message = "plugin decision",
                                name = plugin_instance.plugin_reference(),
                                accept = decision.accept,
                                restrict = decision.restrict,
                                unknown = decision.unknown,
                                score = decision.pignistic().restrict,
                            );
                        }
                        let mut decision_components = decision_components.lock().await;
                        decision_components.push(decision_component);
                    } else if let Err(err) = decision_result {
                        info!(message = "plugin error", error = err.to_string());
                        let mut decision_components = decision_components.lock().await;
                        decision_components.push(DecisionComponents {
                            decision: bulwark_wasm_sdk::UNKNOWN,
                            tags: vec![String::from("error")],
                        });
                    }
                    drop(permit);
                })
                .instrument(response_phase_child_span.or_current()),
            );
        }
        join_all(response_phase_tasks).await;

        let decision_vec: Vec<Decision>;
        let tags: HashSet<String>;
        {
            let decision_components = decision_components.lock().await;
            decision_vec = decision_components.iter().map(|dc| dc.decision).collect();
            tags = decision_components
                .iter()
                .flat_map(|dc| dc.tags.clone())
                .collect();
        }
        let decision = Decision::combine_murphy(&decision_vec);

        DecisionComponents {
            decision,
            tags: tags.into_iter().collect(),
        }
    }

    async fn execute_on_init(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<(), PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance.handle_init().await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_init",
                "ref" => plugin_instance.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_init",
                "ref" => plugin_instance.plugin_reference(), "result" => "error"
            ),
        }
        result
    }

    async fn execute_on_request(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<(), PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance.handle_request().await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_request",
                "ref" => plugin_instance.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_request",
                "ref" => plugin_instance.plugin_reference(), "result" => "error"
            ),
        }
        result
    }

    async fn execute_on_request_decision(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<DecisionComponents, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance.handle_request_decision().await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_request_decision",
                "ref" => plugin_instance.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_request_decision",
                "ref" => plugin_instance.plugin_reference(), "result" => "error"
            ),
        }
        result?;
        Ok(plugin_instance.decision())
    }

    async fn execute_on_request_body_decision(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<DecisionComponents, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance.handle_request_body_decision().await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_request_body_decision",
                "ref" => plugin_instance.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_request_body_decision",
                "ref" => plugin_instance.plugin_reference(), "result" => "error"
            ),
        }
        result?;
        Ok(plugin_instance.decision())
    }

    async fn execute_on_response_decision(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<DecisionComponents, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance.handle_response_decision().await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_response_decision",
                "ref" => plugin_instance.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_response_decision",
                "ref" => plugin_instance.plugin_reference(), "result" => "error"
            ),
        }
        result?;
        Ok(plugin_instance.decision())
    }

    async fn execute_on_response_body_decision(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<DecisionComponents, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance.handle_response_body_decision().await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_response_body_decision",
                "ref" => plugin_instance.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_response_body_decision",
                "ref" => plugin_instance.plugin_reference(), "result" => "error"
            ),
        }
        result?;
        Ok(plugin_instance.decision())
    }

    async fn execute_on_decision_feedback(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<(), PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().await;
        let result = plugin_instance.handle_decision_feedback().await;
        match result {
            Ok(_) => metrics::increment_counter!(
                "plugin_wasm_on_decision_feedback",
                "ref" => plugin_instance.plugin_reference(), "result" => "ok"
            ),
            Err(_) => metrics::increment_counter!(
                "plugin_wasm_on_decision_feedback",
                "ref" => plugin_instance.plugin_reference(), "result" => "error"
            ),
        }
        result
    }

    // TODO: refactor this (issue #112)
    #[allow(clippy::too_many_arguments)]
    async fn handle_request_phase_decision(
        sender: UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        mut stream: Streaming<ProcessingRequest>,
        http_req: Arc<bulwark_wasm_sdk::Request>,
        decision_components: DecisionComponents,
        thresholds: Thresholds,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) {
        let decision = decision_components.decision;
        let outcome = decision
            .outcome(thresholds.trust, thresholds.suspicious, thresholds.restrict)
            .unwrap();

        info!(
            message = "combine decision",
            accept = decision.accept,
            restrict = decision.restrict,
            unknown = decision.unknown,
            score = decision.pignistic().restrict,
            outcome = outcome.to_string(),
            observe_only = thresholds.observe_only,
            // array values aren't handled well unfortunately, coercing to comma-separated values seems to be the best option
            tags = decision_components
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
            "observe_only" => thresholds.observe_only.to_string(),
        );

        let mut receive_request_body = false;
        // Don't check if there's no body to process
        if !(http_req.body().received
            && http_req.body().end_of_stream
            && http_req.body().size == 0
            && http_req.body().start == 0)
        {
            // Iterator with `any` could be cleaner, but there's an async lock.
            for plugin_instance in &plugin_instances {
                let plugin_instance = plugin_instance.lock().await;
                receive_request_body =
                    receive_request_body || plugin_instance.receive_request_body();
                if receive_request_body {
                    break;
                }
            }
        }

        let mut restricted = false;
        match outcome {
            bulwark_wasm_sdk::Outcome::Trusted
            | bulwark_wasm_sdk::Outcome::Accepted
            // suspected requests are monitored but not rejected
            | bulwark_wasm_sdk::Outcome::Suspected => {
                let result = Self::allow_request(&sender, &decision_components, receive_request_body).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    debug!(message = format!("send error: {}", err));
                }
            },
            bulwark_wasm_sdk::Outcome::Restricted => {
                restricted = true;
                if !thresholds.observe_only {
                    info!(message = "process response", status = 403);
                    let result = Self::block_request(&sender, &decision_components).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        debug!(message = format!("send error: {}", err));
                    }

                    // Normally we initiate feedback after the response phase, but if we skip the response phase
                    // we need to do it here instead.
                    Self::handle_decision_feedback(
                        decision_components,
                        outcome,
                        plugin_instances.clone(),
                        timeout_duration,
                        plugin_semaphore.clone(),
                    ).await;
                    // Short-circuit if restricted, we can skip the response phase
                    return;
                } else {
                    // Don't receive a body when we would have otherwise blocked if we weren't in monitor-only mode
                    let result = Self::allow_request(&sender, &decision_components, false).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        debug!(message = format!("send error: {}", err));
                    }
                }
            },
        }

        // This will still be false if there's no body to process, causing the body phase to be skipped.
        // Skip request body processing if we've already blocked or if observe-only mode is on or we would have.
        if receive_request_body && !restricted {
            let http_req = Self::merge_request_body(http_req, &mut stream).await;
            Self::handle_request_body_phase_decision(
                sender,
                stream,
                http_req.clone(),
                Self::execute_request_body_phase(
                    plugin_instances.clone(),
                    http_req,
                    timeout_duration,
                    plugin_semaphore.clone(),
                )
                .await,
                thresholds,
                plugin_instances,
                timeout_duration,
                plugin_semaphore,
            )
            .await;
        } else if let Ok(http_resp) = Self::prepare_response(&mut stream, http_req.version()).await
        {
            let http_resp = Arc::new(http_resp);

            Self::handle_response_phase_decision(
                sender,
                stream,
                http_resp.clone(),
                Self::execute_response_phase(
                    plugin_instances.clone(),
                    http_resp,
                    timeout_duration,
                    plugin_semaphore.clone(),
                )
                .await,
                thresholds,
                plugin_instances,
                timeout_duration,
                plugin_semaphore,
            )
            .await;
        }
    }

    // TODO: refactor this (issue #112)
    #[allow(clippy::too_many_arguments)]
    async fn handle_request_body_phase_decision(
        sender: UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        mut stream: Streaming<ProcessingRequest>,
        http_req: Arc<bulwark_wasm_sdk::Request>,
        decision_components: DecisionComponents,
        thresholds: Thresholds,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) {
        let decision = decision_components.decision;
        let outcome = decision
            .outcome(thresholds.trust, thresholds.suspicious, thresholds.restrict)
            .unwrap();

        info!(
            message = "combine decision",
            accept = decision.accept,
            restrict = decision.restrict,
            unknown = decision.unknown,
            score = decision.pignistic().restrict,
            outcome = outcome.to_string(),
            observe_only = thresholds.observe_only,
            // array values aren't handled well unfortunately, coercing to comma-separated values seems to be the best option
            tags = decision_components
                .tags
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>()
                .to_vec()
                .join(","),
        );
        metrics::increment_counter!(
            "plugin_request_body_phase_decision",
            "outcome" => outcome.to_string(),
            "observe_only" => thresholds.observe_only.to_string(),
        );

        match outcome {
            bulwark_wasm_sdk::Outcome::Trusted
            | bulwark_wasm_sdk::Outcome::Accepted
            // suspected requests are monitored but not rejected
            | bulwark_wasm_sdk::Outcome::Suspected => {
                let result = Self::allow_request_body(&sender).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    debug!(message = format!("send error: {}", err));
                }
            },
            bulwark_wasm_sdk::Outcome::Restricted => {
                if !thresholds.observe_only {
                    info!(message = "process response", status = 403);
                    let result = Self::block_request_body(&sender).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        debug!(message = format!("send error: {}", err));
                    }

                    // Normally we initiate feedback after the response phase, but if we skip the response phase
                    // we need to do it here instead.
                    Self::handle_decision_feedback(
                        decision_components,
                        outcome,
                        plugin_instances.clone(),
                        timeout_duration,
                        plugin_semaphore.clone(),
                    ).await;
                    // Short-circuit if restricted, we can skip the response phase
                    return;
                } else {
                    // Don't receive a body when we would have otherwise blocked if we weren't in monitor-only mode
                    let result = Self::allow_request_body(&sender).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        debug!(message = format!("send error: {}", err));
                    }
                }
            },
        }

        if let Ok(http_resp) = Self::prepare_response(&mut stream, http_req.version()).await {
            let http_resp = Arc::new(http_resp);
            Self::handle_response_phase_decision(
                sender,
                stream,
                http_resp.clone(),
                Self::execute_response_phase(
                    plugin_instances.clone(),
                    http_resp,
                    timeout_duration,
                    plugin_semaphore.clone(),
                )
                .await,
                thresholds,
                plugin_instances,
                timeout_duration,
                plugin_semaphore,
            )
            .await;
        }
    }

    // TODO: refactor this (issue #112)
    #[allow(clippy::too_many_arguments)]
    async fn handle_response_phase_decision(
        sender: UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        mut stream: Streaming<ProcessingRequest>,
        http_resp: Arc<bulwark_wasm_sdk::Response>,
        decision_components: DecisionComponents,
        thresholds: Thresholds,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) {
        let decision = decision_components.decision;
        let outcome = decision
            .outcome(thresholds.trust, thresholds.suspicious, thresholds.restrict)
            .unwrap();

        info!(
            message = "combine decision",
            accept = decision.accept,
            restrict = decision.restrict,
            unknown = decision.unknown,
            score = decision.pignistic().restrict,
            outcome = outcome.to_string(),
            observe_only = thresholds.observe_only,
            // array values aren't handled well unfortunately, coercing to comma-separated values seems to be the best option
            tags = decision_components
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
            "observe_only" => thresholds.observe_only.to_string(),
        );

        // Iterator with `any` would be cleaner, but there's an async lock.
        let mut receive_response_body = false;
        if !(http_resp.body().received
            && http_resp.body().end_of_stream
            && http_resp.body().size == 0
            && http_resp.body().start == 0)
        {
            for plugin_instance in &plugin_instances {
                let plugin_instance = plugin_instance.lock().await;
                receive_response_body =
                    receive_response_body || plugin_instance.receive_response_body();
                if receive_response_body {
                    break;
                }
            }
        }

        let mut restricted = false;
        match outcome {
            bulwark_wasm_sdk::Outcome::Trusted
            | bulwark_wasm_sdk::Outcome::Accepted
            // suspected requests are monitored but not rejected
            | bulwark_wasm_sdk::Outcome::Suspected => {
                info!(message = "process response", status = u16::from(http_resp.status()));
                let result = Self::allow_response(&sender, &decision_components, receive_response_body).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    debug!(message = format!("send error: {}", err));
                }
            },
            bulwark_wasm_sdk::Outcome::Restricted => {
                restricted = true;
                if !thresholds.observe_only {
                    info!(message = "process response", status = 403);
                    let result = Self::block_response(&sender, &decision_components).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        debug!(message = format!("send error: {}", err));
                    }
                } else {
                    info!(message = "process response", status = u16::from(http_resp.status()));
                    // Don't receive a body when we would have otherwise blocked if we weren't in monitor-only mode
                    let result = Self::allow_response(&sender, &decision_components, false).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        debug!(message = format!("send error: {}", err));
                    }
                }
            }
        }

        // This will still be false if there's no body to process, causing the body phase to be skipped.
        // Skip response body processing if we've already blocked or if observe-only mode is on or we would have.
        if receive_response_body && !restricted {
            let http_resp = Self::merge_response_body(http_resp, &mut stream).await;
            Self::handle_response_body_phase_decision(
                sender,
                http_resp.clone(),
                Self::execute_response_body_phase(
                    plugin_instances.clone(),
                    http_resp,
                    timeout_duration,
                    plugin_semaphore.clone(),
                )
                .await,
                thresholds,
                plugin_instances,
                timeout_duration,
                plugin_semaphore,
            )
            .await;
        } else {
            Self::handle_decision_feedback(
                decision_components,
                outcome,
                plugin_instances.clone(),
                timeout_duration,
                plugin_semaphore,
            )
            .await;
        }
    }

    async fn handle_response_body_phase_decision(
        sender: UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        http_resp: Arc<bulwark_wasm_sdk::Response>,
        decision_components: DecisionComponents,
        thresholds: Thresholds,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) {
        let decision = decision_components.decision;
        let outcome = decision
            .outcome(thresholds.trust, thresholds.suspicious, thresholds.restrict)
            .unwrap();

        info!(
            message = "combine decision",
            accept = decision.accept,
            restrict = decision.restrict,
            unknown = decision.unknown,
            score = decision.pignistic().restrict,
            outcome = outcome.to_string(),
            observe_only = thresholds.observe_only,
            // array values aren't handled well unfortunately, coercing to comma-separated values seems to be the best option
            tags = decision_components
                .tags
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>()
                .to_vec()
                .join(","),
        );
        metrics::increment_counter!(
            "plugin_response_body_phase_decision",
            "outcome" => outcome.to_string(),
            "observe_only" => thresholds.observe_only.to_string(),
        );

        match outcome {
            bulwark_wasm_sdk::Outcome::Trusted
            | bulwark_wasm_sdk::Outcome::Accepted
            // suspected requests are monitored but not rejected
            | bulwark_wasm_sdk::Outcome::Suspected => {
                info!(message = "process response", status = u16::from(http_resp.status()));
                let result = Self::allow_response_body(&sender).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    debug!(message = format!("send error: {}", err));
                }
            },
            bulwark_wasm_sdk::Outcome::Restricted => {
                if !thresholds.observe_only {
                    info!(message = "process response", status = 403);
                    let result = Self::block_response_body(&sender).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        debug!(message = format!("send error: {}", err));
                    }
                } else {
                    info!(message = "process response", status = u16::from(http_resp.status()));
                    // Don't receive a body when we would have otherwise blocked if we weren't in monitor-only mode
                    let result = Self::allow_response_body(&sender).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        debug!(message = format!("send error: {}", err));
                    }
                }
            }
        }

        Self::handle_decision_feedback(
            decision_components,
            outcome,
            plugin_instances.clone(),
            timeout_duration,
            plugin_semaphore,
        )
        .await;
    }

    async fn handle_decision_feedback(
        decision_components: DecisionComponents,
        outcome: bulwark_wasm_sdk::Outcome,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
        plugin_semaphore: Arc<Semaphore>,
    ) {
        metrics::increment_counter!(
            "combined_decision",
            "outcome" => outcome.to_string(),
        );
        metrics::histogram!(
            "combined_decision_score",
            decision_components.decision.pignistic().restrict
        );

        let mut feedback_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances.iter().cloned() {
            let response_phase_child_span = tracing::info_span!("execute on_decision_feedback",);
            let permit = plugin_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            {
                // Make sure the plugin instance knows about the final combined decision
                let mut plugin_instance = plugin_instance.lock().await;
                plugin_instance.record_combined_decision(&decision_components, outcome);
                metrics::histogram!(
                    "decision_score",
                    plugin_instance.decision().decision.pignistic().restrict,
                    "ref" => plugin_instance.plugin_reference(),
                );
            }
            feedback_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    Self::execute_on_decision_feedback(plugin_instance)
                        .await
                        .ok();
                    drop(permit);
                })
                .instrument(response_phase_child_span.or_current()),
            );
        }
        join_all(feedback_phase_tasks).await;

        // Capturing stdio is always the last thing that happens and feedback should always be the second-to-last.
        Self::handle_stdio(plugin_instances).await;
    }

    #[instrument(name = "plugin output", skip(plugin_instances))]
    async fn handle_stdio(plugin_instances: Vec<Arc<Mutex<PluginInstance>>>) {
        // TODO: refactor to process one plugin at a time and try to avoid having handle_decision_feedback join_all
        for plugin_instance in plugin_instances {
            let plugin_instance = plugin_instance.lock().await;
            let (_, stdout, stderr) = plugin_instance.stdio().into_inner();
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
                    info!(
                        message = "stderr",
                        plugin = plugin_instance.plugin_reference(),
                        content = line
                    );
                }
            }
        }
    }

    async fn allow_request(
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        decision_components: &DecisionComponents,
        receive_request_body: bool,
    ) -> Result<(), ProcessingMessageError> {
        // Send back a response that changes the request header for the HTTP target.
        let mut req_headers_cr = CommonResponse::default();
        Self::add_set_header(
            &mut req_headers_cr,
            "Bulwark-Decision",
            &serialize_decision_sfv(decision_components.decision)
                .map_err(|err| SfvError::Serialization(err.to_string()))?,
        );
        if !decision_components.tags.is_empty() {
            Self::add_set_header(
                &mut req_headers_cr,
                "Bulwark-Tags",
                &serialize_tags_sfv(decision_components.tags.clone())
                    .map_err(|err| SfvError::Serialization(err.to_string()))?,
            );
        }
        let mut req_headers_resp = ProcessingResponse {
            response: Some(processing_response::Response::RequestHeaders(
                HeadersResponse {
                    response: Some(req_headers_cr),
                },
            )),
            ..Default::default()
        };
        if receive_request_body {
            req_headers_resp.mode_override = Some(ProcessingMode {
                request_body_mode: processing_mode::BodySendMode::BufferedPartial as i32,
                ..Default::default()
            });
        }
        Ok(sender.send(Ok(req_headers_resp)).await?)
    }

    async fn block_request(
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        // TODO: this will be used in the future
        _decision_components: &DecisionComponents,
    ) -> Result<(), ProcessingMessageError> {
        // Send back a response indicating the request has been blocked.
        let req_headers_resp = ProcessingResponse {
            response: Some(processing_response::Response::ImmediateResponse(
                ImmediateResponse {
                    status: Some(HttpStatus { code: 403 }),
                    // TODO: add decision debug
                    details: "blocked by bulwark".to_string(),
                    // TODO: better default response + customizability
                    body: "Access Denied\n".to_string(),
                    headers: None,
                    grpc_status: None,
                },
            )),
            ..Default::default()
        };
        Ok(sender.send(Ok(req_headers_resp)).await?)
    }

    async fn allow_request_body(
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
    ) -> Result<(), ProcessingMessageError> {
        let req_body_resp = ProcessingResponse {
            response: Some(processing_response::Response::RequestBody(
                BodyResponse::default(),
            )),
            ..Default::default()
        };
        Ok(sender.send(Ok(req_body_resp)).await?)
    }

    async fn block_request_body(
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
    ) -> Result<(), ProcessingMessageError> {
        // Send back a response indicating the request has been blocked.
        let req_body_resp = ProcessingResponse {
            response: Some(processing_response::Response::ImmediateResponse(
                ImmediateResponse {
                    status: Some(HttpStatus { code: 403 }),
                    // TODO: add decision debug
                    details: "blocked by bulwark".to_string(),
                    // TODO: better default response + customizability
                    body: "Access Denied\n".to_string(),
                    headers: None,
                    grpc_status: None,
                },
            )),
            ..Default::default()
        };
        Ok(sender.send(Ok(req_body_resp)).await?)
    }

    async fn allow_response(
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        // TODO: this will be used in the future
        _decision_components: &DecisionComponents,
        receive_response_body: bool,
    ) -> Result<(), ProcessingMessageError> {
        let mut resp_headers_resp = ProcessingResponse {
            response: Some(processing_response::Response::ResponseHeaders(
                HeadersResponse::default(),
            )),
            ..Default::default()
        };
        if receive_response_body {
            resp_headers_resp.mode_override = Some(ProcessingMode {
                response_body_mode: processing_mode::BodySendMode::BufferedPartial as i32,
                ..Default::default()
            });
        }
        Ok(sender.send(Ok(resp_headers_resp)).await?)
    }

    async fn block_response(
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
        // TODO: this will be used in the future
        _decision_components: &DecisionComponents,
    ) -> Result<(), ProcessingMessageError> {
        // Send back a response indicating the request has been blocked.
        let resp_headers_resp = ProcessingResponse {
            response: Some(processing_response::Response::ImmediateResponse(
                ImmediateResponse {
                    status: Some(HttpStatus { code: 403 }),
                    // TODO: add decision debug
                    details: "blocked by bulwark".to_string(),
                    // TODO: better default response + customizability
                    body: "Access Denied\n".to_string(),
                    headers: None,
                    grpc_status: None,
                },
            )),
            ..Default::default()
        };
        Ok(sender.send(Ok(resp_headers_resp)).await?)
    }

    async fn allow_response_body(
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
    ) -> Result<(), ProcessingMessageError> {
        let resp_body_resp = ProcessingResponse {
            response: Some(processing_response::Response::ResponseBody(
                BodyResponse::default(),
            )),
            ..Default::default()
        };
        Ok(sender.send(Ok(resp_body_resp)).await?)
    }

    async fn block_response_body(
        mut sender: &UnboundedSender<Result<ProcessingResponse, tonic::Status>>,
    ) -> Result<(), ProcessingMessageError> {
        // Send back a response indicating the request has been blocked.
        let resp_body_resp = ProcessingResponse {
            response: Some(processing_response::Response::ImmediateResponse(
                ImmediateResponse {
                    status: Some(HttpStatus { code: 403 }),
                    // TODO: add decision debug
                    details: "blocked by bulwark".to_string(),
                    // TODO: better default response + customizability
                    body: "Access Denied\n".to_string(),
                    headers: None,
                    grpc_status: None,
                },
            )),
            ..Default::default()
        };
        Ok(sender.send(Ok(resp_body_resp)).await?)
    }

    async fn get_request_headers(stream: &mut Streaming<ProcessingRequest>) -> Option<HttpHeaders> {
        // TODO: if request attributes are eventually supported, we may need to extract both instead of just headers
        if let Ok(Some(next_msg)) = stream.message().await {
            if let Some(processing_request::Request::RequestHeaders(hdrs)) = next_msg.request {
                return Some(hdrs);
            }
        }
        None
    }

    async fn get_request_body(stream: &mut Streaming<ProcessingRequest>) -> Option<HttpBody> {
        if let Ok(Some(next_msg)) = stream.message().await {
            if let Some(processing_request::Request::RequestBody(body)) = next_msg.request {
                return Some(body);
            }
        }
        None
    }

    async fn get_response_headers(
        stream: &mut Streaming<ProcessingRequest>,
    ) -> Option<HttpHeaders> {
        if let Ok(Some(next_msg)) = stream.message().await {
            if let Some(processing_request::Request::ResponseHeaders(hdrs)) = next_msg.request {
                return Some(hdrs);
            }
        }
        None
    }

    async fn get_response_body(stream: &mut Streaming<ProcessingRequest>) -> Option<HttpBody> {
        if let Ok(Some(next_msg)) = stream.message().await {
            if let Some(processing_request::Request::ResponseBody(body)) = next_msg.request {
                return Some(body);
            }
        }
        None
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

    fn add_set_header(cr: &mut CommonResponse, key: &str, value: &str) {
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

    fn parse_forwarded_ip(forwarded: &str, hops: usize) -> Option<IpAddr> {
        let value = ForwardedHeaderValue::from_forwarded(forwarded).ok();
        value.and_then(|fhv| {
            if hops > fhv.len() {
                None
            } else {
                let item = fhv.iter().nth(fhv.len() - hops);
                item.and_then(|fs| fs.forwarded_for_ip())
            }
        })
    }

    fn parse_x_forwarded_for_ip(forwarded: &str, hops: usize) -> Option<IpAddr> {
        let value = ForwardedHeaderValue::from_x_forwarded_for(forwarded).ok();
        value.and_then(|fhv| {
            if hops > fhv.len() {
                None
            } else {
                let item = fhv.iter().nth(fhv.len() - hops);
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
