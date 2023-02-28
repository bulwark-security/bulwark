// TODO: the host/guest wit files seem to be why the latest version switched to one generate macro?

// TODO: switch to wasmtime::component::bindgen!
wit_bindgen_rust::import!("../../bulwark-host.wit");

use std::str::FromStr;

pub use crate::Decision;
pub use bulwark_host::check_rate_limit;
pub use bulwark_host::get_remote_state;
pub use bulwark_host::increment_rate_limit;
pub use bulwark_host::increment_remote_state;
pub use bulwark_host::increment_remote_state_by;
pub use bulwark_host::set_remote_state;
pub use bulwark_host::set_remote_ttl;
pub use bulwark_host::set_tags; // TODO: use BTreeSet for merging sorted tag lists
pub use http::{Extensions, Method, Uri, Version};
use validator::{Validate, ValidationErrors};

use self::bulwark_host::DecisionInterface;

pub type Request = http::Request<BodyChunk>;
pub type Response = http::Response<BodyChunk>;

pub struct BodyChunk {
    pub end_of_stream: bool,
    pub size: u64,
    pub start: u64,
    pub content: Vec<u8>,
}

// TODO: might need either get_remote_addr or an extension on the request for non-forwarded IP address

pub use serde_json::{Map, Value};

pub fn get_config() -> serde_json::Value {
    let raw_config = bulwark_host::get_config();
    serde_json::from_slice(&raw_config).unwrap()
}

pub fn get_config_value(key: &str) -> Option<serde_json::Value> {
    // TODO: this should return a result
    let raw_config = bulwark_host::get_config();
    let object: serde_json::Value = serde_json::from_slice(&raw_config).unwrap();
    match object {
        Value::Object(v) => v.get(&key.to_string()).cloned(),
        _ => panic!("unexpected config value"),
    }
}

pub fn get_param_value(key: &str) -> serde_json::Value {
    // TODO: this should return a result
    let raw_value = bulwark_host::get_param_value(key);
    let value: serde_json::Value = serde_json::from_slice(&raw_value).unwrap();
    value
}

pub fn set_param_value(key: &str, value: serde_json::Value) {
    // TODO: this should return a result
    let json = serde_json::to_vec(&value).unwrap();
    bulwark_host::set_param_value(key, &json);
}

pub fn get_request() -> Request {
    let raw_request: bulwark_host::RequestInterface = bulwark_host::get_request();
    let chunk: Vec<u8> = raw_request.chunk;
    // TODO: error handling
    let method = Method::from_str(raw_request.method.as_str()).unwrap();
    let mut request = http::Request::builder()
        .method(method)
        .uri(raw_request.uri)
        .version(http::Version::HTTP_11); // TODO: don't hard-code version
    for header in raw_request.headers {
        request = request.header(header.name, header.value);
    }
    request
        .body(BodyChunk {
            content: chunk,
            size: raw_request.chunk_length,
            start: raw_request.chunk_start,
            end_of_stream: raw_request.end_of_stream,
        })
        .unwrap()
}

pub fn get_response() -> Response {
    let raw_response: bulwark_host::ResponseInterface = bulwark_host::get_response();
    let chunk: Vec<u8> = raw_response.chunk;
    // TODO: error handling
    let status: u16 = raw_response.status.try_into().unwrap();
    let mut response = http::Response::builder().status(status);
    for header in raw_response.headers {
        response = response.header(header.name, header.value);
    }
    response
        .body(BodyChunk {
            content: chunk,
            size: raw_response.chunk_length,
            start: raw_response.chunk_start,
            end_of_stream: raw_response.end_of_stream,
        })
        .unwrap()
}

pub fn set_decision(decision: Decision) -> Result<(), ValidationErrors> {
    decision.validate()?;
    bulwark_host::set_decision(DecisionInterface {
        accept: decision.accept,
        restrict: decision.restrict,
        unknown: decision.unknown,
    });
    Ok(())
}
