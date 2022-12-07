use bulwark_wasm_host::{Plugin, PluginInstance};
use envoy_control_plane::envoy::config::cluster::v3::Filter;

use crate::errors::FilterProcessingError;
use std::{str::FromStr, sync::Arc};
use {
    envoy_control_plane::envoy::{
        config::core::v3::{HeaderMap, HeaderValue, HeaderValueOption},
        service::ext_proc::v3::{
            external_processor_server::ExternalProcessor, processing_request, processing_response,
            CommonResponse, HeaderMutation, HeadersResponse, HttpHeaders, ProcessingRequest,
            ProcessingResponse,
        },
    },
    futures::{channel::mpsc::UnboundedSender, SinkExt, Stream},
    std::{pin::Pin, str},
    tonic::{Request, Response, Status, Streaming},
};

type ExternalProcessorStream =
    Pin<Box<dyn Stream<Item = Result<ProcessingResponse, Status>> + Send>>;

#[derive(Clone)]
pub struct BulwarkProcessor {
    pub plugin: Arc<Plugin>,
}

#[tonic::async_trait]
impl ExternalProcessor for BulwarkProcessor {
    type ProcessStream = ExternalProcessorStream;

    async fn process(
        &self,
        request: Request<Streaming<ProcessingRequest>>,
    ) -> Result<Response<ExternalProcessorStream>, Status> {
        let mut stream = request.into_inner();
        if let Ok(http_req) = prepare_request(&mut stream).await {
            // println!("request method: {}", http_req.method().as_str());
            // println!("request path: {}", http_req.uri());

            // TODO: figure out borrow problem here
            let plugin_instance_result = PluginInstance::new(Arc::clone(&self.plugin), http_req);
            let mut plugin_instance = plugin_instance_result.unwrap();

            let (sender, receiver) = futures::channel::mpsc::unbounded();
            tokio::task::spawn(async move {
                let decision_result = plugin_instance.start();
                let decision = decision_result.unwrap();
                // println!(
                //     "accept: {} restrict: {} unknown: {}",
                //     decision.accept, decision.restrict, decision.unknown
                // );

                handle_add_header(sender, stream).await;
            });
            return Ok(Response::new(Box::pin(receiver)));
        }
        // By default, just close the stream.
        Ok(Response::new(Box::pin(futures::stream::empty())))
    }
}

// Add a header to the response.
async fn prepare_request(
    stream: &mut Streaming<ProcessingRequest>,
) -> Result<bulwark_wasm_sdk::Request, FilterProcessingError> {
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

// Add a header to the response.
async fn handle_add_header(
    mut sender: UnboundedSender<Result<ProcessingResponse, Status>>,
    mut stream: Streaming<ProcessingRequest>,
) {
    // Send back a response that changes the request header for the HTTP target.
    let mut req_headers_cr = CommonResponse::default();
    add_set_header(&mut req_headers_cr, ":path", "/hello");
    let req_headers_resp = ProcessingResponse {
        response: Some(processing_response::Response::RequestHeaders(
            HeadersResponse {
                response: Some(req_headers_cr),
            },
        )),
        ..Default::default()
    };
    sender.send(Ok(req_headers_resp)).await.ok();

    if get_response_headers(&mut stream).await.is_some() {
        let mut resp_headers_cr = CommonResponse::default();
        add_set_header(
            &mut resp_headers_cr,
            "x-external-processor-status",
            "Powered by Rust!",
        );

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
