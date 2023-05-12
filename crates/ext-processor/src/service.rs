//! The service module contains the main Envoy external processor service implementation.

use {
    crate::{
        serialize_decision_sfv, serialize_tags_sfv, PluginGroupInstantiationError,
        PrepareRequestError, PrepareResponseError, ProcessingMessageError, SfvError,
    },
    bulwark_config::{Config, Thresholds},
    bulwark_wasm_host::{
        DecisionComponents, ForwardedIP, Plugin, PluginExecutionError, PluginInstance,
        PluginLoadError, RedisInfo, RequestContext, ScriptRegistry,
    },
    bulwark_wasm_sdk::{BodyChunk, Decision},
    envoy_control_plane::envoy::{
        config::core::v3::{HeaderMap, HeaderValue, HeaderValueOption},
        r#type::v3::HttpStatus,
        service::ext_proc::v3::{
            external_processor_server::ExternalProcessor, processing_request, processing_response,
            CommonResponse, HeaderMutation, HeadersResponse, HttpHeaders, ImmediateResponse,
            ProcessingRequest, ProcessingResponse,
        },
    },
    forwarded_header_value::ForwardedHeaderValue,
    futures::{channel::mpsc::UnboundedSender, SinkExt, Stream},
    http::StatusCode,
    matchit::Router,
    std::{
        collections::HashSet,
        net::IpAddr,
        pin::Pin,
        str,
        str::FromStr,
        sync::{Arc, Mutex},
        time::Duration,
    },
    tokio::{sync::RwLock, task::JoinSet, time::timeout},
    tonic::{Request, Response, Status, Streaming},
    tracing::{debug, error, info, instrument, warn, Instrument},
};

extern crate redis;

type ExternalProcessorStream =
    Pin<Box<dyn Stream<Item = Result<ProcessingResponse, Status>> + Send>>;
type PluginList = Vec<Arc<Plugin>>;

/// A RouteTarget allows a router to map from a routing pattern to a plugin group and associated config values.
///
/// See [`bulwark_config::Resource`] for its configuration.
struct RouteTarget {
    plugins: PluginList,
    timeout: Option<u64>,
}

/// The `BulwarkProcessor` implements the primary envoy processing service logic via the [`ExternalProcessor`] trait.
///
/// The [`process`](BulwarkProcessor::process) function is the main request handler.
pub struct BulwarkProcessor {
    // TODO: may need to have a plugin registry at some point
    router: Arc<RwLock<Router<RouteTarget>>>,
    redis_info: Option<Arc<RedisInfo>>,
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
        tonic_request: Request<Streaming<ProcessingRequest>>,
    ) -> Result<Response<ExternalProcessorStream>, Status> {
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
                                http_req.clone(),
                                route_match.params,
                            )
                            .unwrap();
                            if let Some(millis) = route_match.value.timeout {
                                timeout_duration = Duration::from_millis(millis);
                            }

                            let combined = Self::execute_request_phase(
                                plugin_instances.clone(),
                                timeout_duration,
                            )
                            .await;

                            Self::handle_request_phase_decision(
                                sender,
                                stream,
                                combined,
                                thresholds,
                                plugin_instances,
                                timeout_duration,
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
                }
                .instrument(child_span.or_current()),
            );
            return Ok(Response::new(Box::pin(receiver)));
        }
        // By default, just close the stream.
        Ok(Response::new(Box::pin(futures::stream::empty())))
    }
}

impl BulwarkProcessor {
    /// Creates a new [`BulwarkProcessor`].
    ///
    /// # Arguments
    ///
    /// * `config` - The root of the Bulwark configuration structure to be used to initialize the service.
    pub fn new(config: Config) -> Result<Self, PluginLoadError> {
        let redis_info = if let Some(remote_state_addr) = config.service.remote_state.as_ref() {
            // TODO: better error handling instead of unwrap/panic
            let client = redis::Client::open(remote_state_addr.as_str()).unwrap();
            // TODO: make pool size configurable
            Some(Arc::new(RedisInfo {
                // TODO: better error handling instead of unwrap/panic
                pool: r2d2::Pool::builder().max_size(16).build(client).unwrap(),
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
            let plugin_configs = resource.resolve_plugins(&config);
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
            thresholds: config.thresholds,
            hops: usize::from(config.service.proxy_hops),
        })
    }

    async fn prepare_request(
        stream: &mut Streaming<ProcessingRequest>,
        proxy_hops: usize,
    ) -> Result<bulwark_wasm_sdk::Request, PrepareRequestError> {
        if let Some(header_msg) = Self::get_request_headers(stream).await {
            // TODO: currently this information isn't used and isn't accessible to the plugin environment yet
            // TODO: does this go into a request extension?
            let _authority = Self::get_header_value(&header_msg.headers, ":authority")
                .ok_or(PrepareRequestError::MissingAuthority)?;
            let _scheme = Self::get_header_value(&header_msg.headers, ":scheme")
                .ok_or(PrepareRequestError::MissingScheme)?;

            let method = http::Method::from_str(
                Self::get_header_value(&header_msg.headers, ":method")
                    .ok_or(PrepareRequestError::MissingMethod)?,
            )?;
            let request_uri = Self::get_header_value(&header_msg.headers, ":path")
                .ok_or(PrepareRequestError::MissingPath)?;
            let mut request = http::Request::builder();
            // TODO: read the request body
            let request_chunk = bulwark_wasm_sdk::BodyChunk {
                end_of_stream: header_msg.end_of_stream,
                size: 0,
                start: 0,
                content: vec![],
            };
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
        Err(PrepareRequestError::MissingHeaders)
    }

    async fn prepare_response(
        stream: &mut Streaming<ProcessingRequest>,
    ) -> Result<bulwark_wasm_sdk::Response, PrepareResponseError> {
        if let Some(header_msg) = Self::get_response_headers(stream).await {
            let status = Self::get_header_value(&header_msg.headers, ":status")
                .ok_or(PrepareResponseError::MissingStatus)?;

            let mut response = http::Response::builder();
            // TODO: read the response body
            let response_chunk = bulwark_wasm_sdk::BodyChunk {
                end_of_stream: header_msg.end_of_stream,
                size: 0,
                start: 0,
                content: vec![],
            };
            response = response.status(status);
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
        Err(PrepareResponseError::MissingHeaders)
    }

    fn instantiate_plugins(
        plugins: &PluginList,
        redis_info: Option<Arc<RedisInfo>>,
        http_req: Arc<bulwark_wasm_sdk::Request>,
        params: matchit::Params,
    ) -> Result<Vec<Arc<Mutex<PluginInstance>>>, PluginGroupInstantiationError> {
        let mut plugin_instances = Vec::with_capacity(plugins.len());
        let mut shared_params = bulwark_wasm_sdk::Map::new();
        for (key, value) in params.iter() {
            let wrapped_value = bulwark_wasm_sdk::Value::String(value.to_string());
            shared_params.insert(format!("param.{}", key), wrapped_value);
        }
        let shared_params = Arc::new(Mutex::new(shared_params));
        for plugin in plugins {
            let request_context = RequestContext::new(
                plugin.clone(),
                redis_info.clone(),
                shared_params.clone(),
                http_req.clone(),
            )?;

            plugin_instances.push(Arc::new(Mutex::new(PluginInstance::new(
                plugin.clone(),
                request_context,
            )?)));
        }
        Ok(plugin_instances)
    }

    async fn execute_request_phase(
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
    ) -> DecisionComponents {
        Self::execute_request_phase_one(plugin_instances.clone(), timeout_duration).await;
        Self::execute_request_phase_two(plugin_instances.clone(), timeout_duration).await
    }

    async fn execute_request_phase_one(
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
    ) {
        let mut phase_one_tasks = JoinSet::new();
        for plugin_instance in plugin_instances.clone() {
            let phase_one_child_span = tracing::info_span!("execute on_request",);
            phase_one_tasks.spawn(
                timeout(timeout_duration, async move {
                    // TODO: avoid unwraps
                    Self::execute_plugin_initialization(plugin_instance.clone()).unwrap();
                    Self::execute_on_request(plugin_instance.clone()).unwrap();
                })
                .instrument(phase_one_child_span.or_current()),
            );
        }
        // efficiently hand execution off to the plugins
        tokio::task::yield_now().await;

        while let Some(r) = phase_one_tasks.join_next().await {
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

    async fn execute_request_phase_two(
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
    ) -> DecisionComponents {
        let decision_components = Arc::new(Mutex::new(Vec::with_capacity(plugin_instances.len())));
        let mut phase_two_tasks = JoinSet::new();
        for plugin_instance in plugin_instances.clone() {
            let phase_two_child_span = tracing::info_span!("execute on_request_decision",);
            let decision_components = decision_components.clone();
            phase_two_tasks.spawn(
                timeout(timeout_duration, async move {
                    let decision_result =
                        Self::execute_on_request_decision(plugin_instance.clone());
                    let mut decision_component = decision_result.unwrap();
                    {
                        // Re-weight the decision based on its weighting value from the configuration
                        let plugin_instance = plugin_instance.lock().unwrap();
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
                    let mut decision_components = decision_components.lock().unwrap();
                    decision_components.push(decision_component);
                })
                .instrument(phase_two_child_span.or_current()),
            );
        }
        // efficiently hand execution off to the plugins
        tokio::task::yield_now().await;

        while let Some(r) = phase_two_tasks.join_next().await {
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
        let decision_vec: Vec<Decision>;
        let tags: HashSet<String>;
        {
            let decision_components = decision_components.lock().unwrap();
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
    ) -> DecisionComponents {
        let decision_components = Arc::new(Mutex::new(Vec::with_capacity(plugin_instances.len())));
        let mut response_phase_tasks = JoinSet::new();
        for plugin_instance in plugin_instances.clone() {
            let response_phase_child_span = tracing::info_span!("execute on_response_decision",);
            {
                // Make sure the plugin instance knows about the response
                let mut plugin_instance = plugin_instance.lock().unwrap();
                let response = response.clone();
                plugin_instance.record_response(response);
            }
            let decision_components = decision_components.clone();
            response_phase_tasks.spawn(
                timeout(timeout_duration, async move {
                    let decision_result =
                        Self::execute_on_response_decision(plugin_instance.clone());
                    let mut decision_component = decision_result.unwrap();
                    {
                        // Re-weight the decision based on its weighting value from the configuration
                        let plugin_instance = plugin_instance.lock().unwrap();
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
                    let mut decision_components = decision_components.lock().unwrap();
                    decision_components.push(decision_component);
                })
                .instrument(response_phase_child_span.or_current()),
            );
        }
        // efficiently hand execution off to the plugins
        tokio::task::yield_now().await;

        while let Some(r) = response_phase_tasks.join_next().await {
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
        let decision_vec: Vec<Decision>;
        let tags: HashSet<String>;
        {
            let decision_components = decision_components.lock().unwrap();
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

    fn execute_plugin_initialization(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<(), PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().unwrap();
        // unlike on_request, the _start/main function is mandatory
        plugin_instance.start()
    }

    fn execute_on_request(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<(), PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().unwrap();
        let result = plugin_instance.handle_request();
        match result {
            Ok(_) => result,
            Err(e) => match e {
                // we can silence not implemented errors because they are expected and normal here
                PluginExecutionError::NotImplementedError { expected: _ } => Ok(()),
                // everything else will get passed along
                _ => Err(e),
            },
        }
    }

    fn execute_on_request_decision(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<DecisionComponents, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().unwrap();
        let result = plugin_instance.handle_request_decision();
        if let Err(e) = result {
            match e {
                // we can silence not implemented errors because they are expected and normal here
                PluginExecutionError::NotImplementedError { expected: _ } => (),
                // everything else will get passed along
                _ => Err(e)?,
            }
        }
        Ok(plugin_instance.decision())
    }

    fn execute_on_response_decision(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<DecisionComponents, PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().unwrap();
        let result = plugin_instance.handle_response_decision();
        if let Err(e) = result {
            match e {
                // we can silence not implemented errors because they are expected and normal here
                PluginExecutionError::NotImplementedError { expected: _ } => (),
                // everything else will get passed along
                _ => Err(e)?,
            }
        }
        Ok(plugin_instance.decision())
    }

    fn execute_on_decision_feedback(
        plugin_instance: Arc<Mutex<PluginInstance>>,
    ) -> Result<(), PluginExecutionError> {
        let mut plugin_instance = plugin_instance.lock().unwrap();
        let result = plugin_instance.handle_decision_feedback();
        if let Err(e) = result {
            match e {
                // we can silence not implemented errors because they are expected and normal here
                PluginExecutionError::NotImplementedError { expected: _ } => (),
                // everything else will get passed along
                _ => Err(e)?,
            }
        }
        Ok(())
    }

    async fn handle_request_phase_decision(
        sender: UnboundedSender<Result<ProcessingResponse, Status>>,
        mut stream: Streaming<ProcessingRequest>,
        decision_components: DecisionComponents,
        thresholds: Thresholds,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
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
            outcome = match outcome {
                bulwark_wasm_sdk::Outcome::Trusted => "trusted",
                bulwark_wasm_sdk::Outcome::Accepted => "accepted",
                bulwark_wasm_sdk::Outcome::Suspected => "suspected",
                bulwark_wasm_sdk::Outcome::Restricted => "restricted",
            },
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

        match outcome {
            bulwark_wasm_sdk::Outcome::Trusted
            | bulwark_wasm_sdk::Outcome::Accepted
            // suspected requests are monitored but not rejected
            | bulwark_wasm_sdk::Outcome::Suspected => {
                let result = Self::allow_request(&sender, &decision_components).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    debug!(message = format!("send error: {}", err));
                }
            },
            bulwark_wasm_sdk::Outcome::Restricted => {
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
                        plugin_instances,
                        timeout_duration,
                    );
                    // Short-circuit if restricted, we can skip the response phase
                    return;
                } else {
                    let result = Self::allow_request(&sender, &decision_components).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        debug!(message = format!("send error: {}", err));
                    }
                }
            },
        }

        if let Ok(http_resp) = Self::prepare_response(&mut stream).await {
            let http_resp = Arc::new(http_resp);
            let status = http_resp.status();

            Self::handle_response_phase_decision(
                sender,
                Self::execute_response_phase(plugin_instances.clone(), http_resp, timeout_duration)
                    .await,
                status,
                thresholds,
                plugin_instances.clone(),
                timeout_duration,
            )
            .await;
        }
    }

    async fn handle_response_phase_decision(
        sender: UnboundedSender<Result<ProcessingResponse, Status>>,
        decision_components: DecisionComponents,
        response_status: StatusCode,
        thresholds: Thresholds,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
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
            outcome = match outcome {
                bulwark_wasm_sdk::Outcome::Trusted => "trusted",
                bulwark_wasm_sdk::Outcome::Accepted => "accepted",
                bulwark_wasm_sdk::Outcome::Suspected => "suspected",
                bulwark_wasm_sdk::Outcome::Restricted => "restricted",
            },
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

        match outcome {
            bulwark_wasm_sdk::Outcome::Trusted
            | bulwark_wasm_sdk::Outcome::Accepted
            // suspected requests are monitored but not rejected
            | bulwark_wasm_sdk::Outcome::Suspected => {
                info!(message = "process response", status = u16::from(response_status));
                let result = Self::allow_response(&sender, &decision_components).await;
                // TODO: must perform proper error handling on sender results, sending can fail
                if let Err(err) = result {
                    debug!(message = format!("send error: {}", err));
                }
            },
            bulwark_wasm_sdk::Outcome::Restricted => {
                if !thresholds.observe_only {
                    info!(message = "process response", status = 403);
                    let result = Self::block_response(&sender, &decision_components).await;
                    // TODO: must perform proper error handling on sender results, sending can fail
                    if let Err(err) = result {
                        debug!(message = format!("send error: {}", err));
                    }
                } else {
                    info!(message = "process response", status = u16::from(response_status));
                    let result = Self::allow_response(&sender, &decision_components).await;
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
            plugin_instances,
            timeout_duration,
        );
    }

    fn handle_decision_feedback(
        decision_components: DecisionComponents,
        outcome: bulwark_wasm_sdk::Outcome,
        plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
        timeout_duration: std::time::Duration,
    ) {
        for plugin_instance in plugin_instances {
            let response_phase_child_span = tracing::info_span!("execute on_decision_feedback",);
            {
                // Make sure the plugin instance knows about the final combined decision
                let mut plugin_instance = plugin_instance.lock().unwrap();
                plugin_instance.record_combined_decision(&decision_components, outcome);
            }
            tokio::spawn(
                timeout(timeout_duration, async move {
                    Self::execute_on_decision_feedback(plugin_instance.clone()).ok();
                })
                .instrument(response_phase_child_span.or_current()),
            );
        }
    }

    async fn allow_request(
        mut sender: &UnboundedSender<Result<ProcessingResponse, Status>>,
        decision_components: &DecisionComponents,
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
        let req_headers_resp = ProcessingResponse {
            response: Some(processing_response::Response::RequestHeaders(
                HeadersResponse {
                    response: Some(req_headers_cr),
                },
            )),
            ..Default::default()
        };
        Ok(sender.send(Ok(req_headers_resp)).await?)
    }

    async fn block_request(
        mut sender: &UnboundedSender<Result<ProcessingResponse, Status>>,
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
                    body: "Access Denied".to_string(),
                    headers: None,
                    grpc_status: None,
                },
            )),
            ..Default::default()
        };
        Ok(sender.send(Ok(req_headers_resp)).await?)
    }

    async fn allow_response(
        mut sender: &UnboundedSender<Result<ProcessingResponse, Status>>,
        // TODO: this will be used in the future
        _decision_components: &DecisionComponents,
    ) -> Result<(), ProcessingMessageError> {
        let resp_headers_resp = ProcessingResponse {
            response: Some(processing_response::Response::RequestHeaders(
                HeadersResponse { response: None },
            )),
            ..Default::default()
        };
        Ok(sender.send(Ok(resp_headers_resp)).await?)
    }

    async fn block_response(
        mut sender: &UnboundedSender<Result<ProcessingResponse, Status>>,
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
                    body: "Access Denied".to_string(),
                    headers: None,
                    grpc_status: None,
                },
            )),
            ..Default::default()
        };
        Ok(sender.send(Ok(resp_headers_resp)).await?)
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
