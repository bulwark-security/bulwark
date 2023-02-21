use tracing::Span;

use {
    crate::{
        serialize_decision_sfv, serialize_tags_sfv, FilterProcessingError,
        MultiPluginInstantiationError,
    },
    bulwark_config::Config,
    bulwark_wasm_host::{
        DecisionComponents, Plugin, PluginInstance, PluginLoadError, RedisInfo, RequestContext,
        ScriptRegistry,
    },
    bulwark_wasm_sdk::{Decision, MassFunction},
    envoy_control_plane::envoy::{
        config::core::v3::{HeaderMap, HeaderValue, HeaderValueOption},
        r#type::v3::HttpStatus,
        service::ext_proc::v3::{
            external_processor_server::ExternalProcessor, processing_request, processing_response,
            CommonResponse, HeaderMutation, HeadersResponse, HttpHeaders, ImmediateResponse,
            ProcessingRequest, ProcessingResponse,
        },
    },
    futures::{
        channel::mpsc::{SendError, UnboundedSender},
        SinkExt, Stream,
    },
    matchit::Router,
    std::{
        collections::HashSet,
        pin::Pin,
        str,
        str::FromStr,
        sync::{Arc, Mutex},
        time::Duration,
    },
    tokio::{sync::RwLock, task::JoinSet, time::timeout},
    tonic::{Request, Response, Status, Streaming},
    tracing::{debug, error, info, instrument, trace, warn, Instrument},
};

extern crate redis;

type ExternalProcessorStream =
    Pin<Box<dyn Stream<Item = Result<ProcessingResponse, Status>> + Send>>;
type PluginList = Vec<Arc<Plugin>>;

struct RouteTarget {
    value: String,
    plugins: PluginList,
    timeout: Option<u64>,
}

// TODO: BulwarkProcessor::new should take a config root as a param, compile all the plugins and build a radix tree router that maps to them

#[derive(Clone)]
pub struct BulwarkProcessor {
    // TODO: may need to have a plugin registry at some point
    router: Arc<RwLock<Router<RouteTarget>>>,
    redis_info: Option<Arc<RedisInfo>>,
}

impl BulwarkProcessor {
    pub fn new(config: Config) -> Result<Self, PluginLoadError> {
        let redis_info = if let Some(service) = config.service.as_ref() {
            if let Some(remote_state_addr) = service.remote_state.as_ref() {
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
            }
        } else {
            None
        };

        // TODO: return an init error not a plugin load error
        let mut router: Router<RouteTarget> = Router::new();
        if let Some(resources) = config.resources.as_ref() {
            for resource in resources {
                let plugin_configs = resource.resolve_plugins(&config);
                let mut plugins: PluginList = Vec::with_capacity(plugin_configs.len());
                for plugin_config in &plugin_configs {
                    // TODO: pass in the plugin config
                    debug!(
                        plugin_path = plugin_config.path,
                        message = "loading plugin",
                        resource = resource.route
                    );
                    let plugin = Plugin::from_file(
                        plugin_config.path.clone(),
                        plugin_config.config_as_json(),
                    )?;
                    plugins.push(Arc::new(plugin));
                }
                router.insert(
                    resource.route.clone(),
                    RouteTarget {
                        value: resource.route.clone(),
                        timeout: resource.timeout,
                        plugins,
                    },
                );
            }
        } else {
            // TODO: error handling
            panic!("no resources found");
        }
        Ok(Self {
            router: Arc::new(RwLock::new(router)),
            redis_info,
        })
    }
}

#[tonic::async_trait]
impl ExternalProcessor for BulwarkProcessor {
    type ProcessStream = ExternalProcessorStream;

    #[instrument(skip(self, request))]
    async fn process(
        &self,
        request: Request<Streaming<ProcessingRequest>>,
    ) -> Result<Response<ExternalProcessorStream>, Status> {
        let mut stream = request.into_inner();
        if let Ok(http_req) = prepare_request(&mut stream).await {
            let redis_info = self.redis_info.clone();
            let http_req = Arc::new(http_req);
            let router = self.router.clone();

            info!(
                message = "request processed",
                method = http_req.method().to_string(),
                uri = http_req.uri().to_string(),
                user_agent = http_req
                    .headers()
                    .get("User-Agent")
                    .map(|ua: &http::HeaderValue| ua.to_str().unwrap_or_default())
            );

            let child_span = tracing::debug_span!("routing request");
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
                            let plugin_instances = instantiate_plugins(
                                &route_target.plugins,
                                redis_info.clone(),
                                http_req.clone(),
                                route_match.params,
                            )
                            .unwrap();
                            if let Some(millis) = route_match.value.timeout {
                                timeout_duration = Duration::from_millis(millis);
                            }

                            let combined =
                                execute_request_phase(plugin_instances, timeout_duration).await;

                            // TODO: need equivalent of prepare_request but for response

                            handle_request_phase_decision(
                                sender,
                                stream,
                                combined.decision,
                                combined.tags,
                            )
                            .await;
                        }
                        Err(err) => {
                            // TODO: figure out how to handle trailing slash errors, silent failure is probably undesirable
                            error!(uri = http_req.uri().to_string(), message = "match error");
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

fn instantiate_plugins(
    plugins: &PluginList,
    redis_info: Option<Arc<RedisInfo>>,
    http_req: Arc<bulwark_wasm_sdk::Request>,
    params: matchit::Params,
) -> Result<Vec<Arc<Mutex<PluginInstance>>>, MultiPluginInstantiationError> {
    let mut plugin_instances = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let mut request_context =
            RequestContext::new(plugin.clone(), redis_info.clone(), http_req.clone())?;
        for (key, value) in params.iter() {
            let wrapped_value = bulwark_wasm_sdk::Value::String(value.to_string());
            request_context.context_insert(key.to_string(), wrapped_value);
        }

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
    let mut tasks = JoinSet::new();
    let decision_components = Arc::new(Mutex::new(Vec::with_capacity(plugin_instances.len())));
    for plugin_instance in plugin_instances {
        let decision_components = decision_components.clone();

        let child_span: Span;
        {
            let plugin_instance = plugin_instance.lock().unwrap();
            child_span = tracing::debug_span!(
                "executing plugin",
                plugin = plugin_instance.plugin_reference()
            );
        }
        tasks.spawn(
            timeout(timeout_duration, async move {
                let mut plugin_instance = plugin_instance.lock().unwrap();
                let decision_result = plugin_instance.start();
                // TODO: avoid unwrap
                let decision_component = decision_result.unwrap();
                {
                    let decision = &decision_component.decision;
                    debug!(
                        message = "plugin decision result",
                        accept = decision.accept,
                        restrict = decision.restrict,
                        unknown = decision.unknown
                    );
                }
                let mut decision_components = decision_components.lock().unwrap();
                decision_components.push(decision_component);
            })
            .instrument(child_span.or_current()),
        );
    }
    // hand execution off to the plugins
    tokio::task::yield_now().await;
    while let Some(r) = tasks.join_next().await {
        match r {
            Ok(Ok(_)) => {}
            Ok(Err(_)) => {
                warn!(message = "timeout waiting on plugin execution");
                // TODO: confirm that we haven't leaked a task on timeout; plugins may not halt
            }
            Err(e) => {
                warn!(
                    message = "join error on plugin execution",
                    error_message = e.to_string(),
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
    let decision = Decision::combine(&decision_vec);

    info!(
        message = "decision combined",
        accept = decision.accept,
        restrict = decision.restrict,
        unknown = decision.unknown,
        // TODO: is it possible to pass a meaningful tracing::Value here instead of formating to string?
        tags = format!("{:?}", tags),
        count = decision_vec.len(),
    );
    DecisionComponents {
        decision,
        tags: tags.into_iter().collect(),
    }
}

async fn execute_on_request(
    plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
    timeout_duration: std::time::Duration,
) {
    todo!()
}

async fn execute_on_request_decision(
    plugin_instances: Vec<Arc<Mutex<PluginInstance>>>,
    timeout_duration: std::time::Duration,
) -> DecisionComponents {
    todo!()
}

// Add a header to the response.
async fn prepare_request(
    stream: &mut Streaming<ProcessingRequest>,
) -> Result<bulwark_wasm_sdk::Request, FilterProcessingError> {
    // TODO: determine client IP address, pass it through as an extension
    if let Some(header_msg) = get_request_headers(stream).await {
        let authority = get_header_value(&header_msg.headers, ":authority").ok_or_else(|| {
            FilterProcessingError::Error(anyhow::anyhow!("Missing HTTP authority"))
        })?;
        // println!("request authority (unused): {}", authority);
        let scheme = get_header_value(&header_msg.headers, ":scheme")
            .ok_or_else(|| FilterProcessingError::Error(anyhow::anyhow!("Missing HTTP scheme")))?;
        // println!("request scheme (unused): {}", scheme);

        let method =
            http::Method::from_str(get_header_value(&header_msg.headers, ":method").ok_or_else(
                || FilterProcessingError::Error(anyhow::anyhow!("Missing HTTP method")),
            )?)?;
        let request_uri = get_header_value(&header_msg.headers, ":path").ok_or_else(|| {
            FilterProcessingError::Error(anyhow::anyhow!("Missing HTTP request URI"))
        })?;
        let mut request = http::Request::builder();
        let request_chunk = bulwark_wasm_sdk::RequestChunk {
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
        return Ok(request.body(request_chunk).unwrap());
    }
    Err(FilterProcessingError::Error(anyhow::anyhow!(
        "Nothing useful happened"
    )))
}

async fn handle_request_phase_decision(
    mut sender: UnboundedSender<Result<ProcessingResponse, Status>>,
    mut stream: Streaming<ProcessingRequest>,
    decision: Decision,
    tags: Vec<String>,
) {
    if decision.accepted(0.5) {
        let result = allow_request(&sender, decision, tags).await;
        // TODO: must perform error handling on sender results, sending can definitely fail
        debug!(message = "send result", result = result.is_ok());
    } else {
        let result = block_request(&sender, decision, tags).await;
        // TODO: must perform error handling on sender results, sending can definitely fail
        debug!(message = "send result", result = result.is_ok());
        return;
    }

    if get_response_headers(&mut stream).await.is_some() {
        let mut resp_headers_cr = CommonResponse::default();
        add_set_header(&mut resp_headers_cr, "x-external-processor", "Bulwark");

        let resp_headers_resp = ProcessingResponse {
            response: Some(processing_response::Response::ResponseHeaders(
                HeadersResponse {
                    response: Some(resp_headers_cr),
                },
            )),
            ..Default::default()
        };
        sender.send(Ok(resp_headers_resp)).await.ok();
    }
    // Fall through if we get the wrong message.
}

async fn allow_request(
    mut sender: &UnboundedSender<Result<ProcessingResponse, Status>>,
    decision: Decision,
    tags: Vec<String>,
) -> Result<(), SendError> {
    // Send back a response that changes the request header for the HTTP target.
    let mut req_headers_cr = CommonResponse::default();
    add_set_header(
        &mut req_headers_cr,
        "Bulwark-Decision",
        &serialize_decision_sfv(decision),
    );
    if !tags.is_empty() {
        add_set_header(
            &mut req_headers_cr,
            "Bulwark-Tags",
            &serialize_tags_sfv(tags),
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
    sender.send(Ok(req_headers_resp)).await
}

async fn block_request(
    mut sender: &UnboundedSender<Result<ProcessingResponse, Status>>,
    decision: Decision,
    tags: Vec<String>,
) -> Result<(), SendError> {
    // Send back a response indicating the request has been blocked.
    let req_headers_resp = ProcessingResponse {
        response: Some(processing_response::Response::ImmediateResponse(
            ImmediateResponse {
                status: Some(HttpStatus { code: 403 }),
                details: "blocked by bulwark".to_string(), // TODO: add decision debug
                body: "Bulwark says no.".to_string(),
                headers: None,
                grpc_status: None,
            },
        )),
        ..Default::default()
    };
    sender.send(Ok(req_headers_resp)).await
}

// async fn allow_response() {}
// async fn block_response() {
//     add_set_header(&mut resp_headers_cr, ":status", "403");
// }

async fn get_request_headers(stream: &mut Streaming<ProcessingRequest>) -> Option<HttpHeaders> {
    if let Ok(Some(next_msg)) = stream.message().await {
        if let Some(processing_request::Request::RequestHeaders(hdrs)) = next_msg.request {
            return Some(hdrs);
        }
    }
    None
}

async fn get_response_headers(stream: &mut Streaming<ProcessingRequest>) -> Option<HttpHeaders> {
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
