use {
  envoy_control_plane::envoy::{
    config::core::v3::{HeaderMap, HeaderValue, HeaderValueOption},
    service::ext_proc::v3::{
      external_processor_server::ExternalProcessor, processing_request,
      processing_response, CommonResponse, HeaderMutation,
      HeadersResponse, HttpHeaders, ProcessingRequest,
      ProcessingResponse,
    },
  },
  futures::{channel::mpsc::UnboundedSender, SinkExt, Stream},
  std::{pin::Pin, str},
  tonic::{Request, Response, Status, Streaming},
};

use wasmer::{Store, Module, Instance, Value, Cranelift, wat2wasm, imports};

type ExternalProcessorStream =
  Pin<Box<dyn Stream<Item = Result<ProcessingResponse, Status>> + Send>>;

pub struct ExampleProcessor {}

#[tonic::async_trait]
impl ExternalProcessor for ExampleProcessor {
  type ProcessStream = ExternalProcessorStream;

  async fn process(
    &self,
    request: Request<Streaming<ProcessingRequest>>,
  ) -> Result<Response<ExternalProcessorStream>, Status> {
    let mut stream = request.into_inner();
    if let Some(req_headers) = get_request_headers(&mut stream).await {
      let (sender, receiver) = futures::channel::mpsc::unbounded();
      let path = get_header_value(&req_headers.headers, ":path");
      match path {
        Some("/math") => {
          tokio::task::spawn(async move {
            handle_add_header(sender, stream).await;
          });
        }
        _ => sender.close_channel(),
      }
      return Ok(Response::new(Box::pin(receiver)));
    }
    // By default, just close the stream.
    Ok(Response::new(Box::pin(futures::stream::empty())))
  }
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

    let module_wat = r#"
    (module
      (type $t0 (func (param i32) (result i32)))
      (func $add_one (export "add_one") (type $t0) (param $p0 i32) (result i32)
        get_local $p0
        i32.const 1
        i32.add))
    "#;

    let wasm_result = wat2wasm(
      module_wat.as_bytes(),
    );
    match wasm_result {
      Ok(wasm_bytes) => {
        let compiler = Cranelift::default();
        let mut store = Store::new(compiler);
        let module_result = Module::new(&store, wasm_bytes);
        match module_result {
          Ok(module) => {
            let inst_result = Instance::new(&mut store, &module, &imports! {});
            match inst_result {
              Ok(instance) => {
                let func_ref_result = instance.exports.get_function("add_one");
                match func_ref_result {
                  Ok(add_one) => {
                    let addend_result = add_one.call(&mut store, &[Value::I32(42)]);
                    match addend_result {
                      Ok(value) => {
                        match value[0].i32() {
                          Some(val) => {
                            add_set_header(
                              &mut resp_headers_cr,
                              "x-external-processor-math",
                              &format!("42+1={}", val),
                            );    
                          }
                          _ => {}
                        }
                      }
                      _ => {}
                    }
                  }
                  _ => {}
                }
              }
              _ => {}
            }
          }
          _ => {}
        }
      }
      _ => {}
    }
  
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
