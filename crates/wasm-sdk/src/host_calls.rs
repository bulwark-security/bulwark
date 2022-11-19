// TODO: the host/guest wit files seem to be why the latest version switched to one generate macro?

wit_bindgen_rust::import!("../../bulwark-host.wit");

use std::str::FromStr;

pub use bulwark_host::set_decision;
pub use bulwark_host::DecisionInterface as Decision;
use http::HeaderMap;
pub use http::{Extensions, Method, Uri, Version};

pub type Request = http::Request<RequestChunk>;

pub struct RequestChunk {
    end_of_stream: bool,
    size: u64,
    start: u64,
    content: Vec<u8>,
}

pub fn get_request() -> Request {
    let raw_request: bulwark_host::RequestInterface = bulwark_host::get_request();
    let chunk: Vec<u8> = raw_request.chunk;
    // TODO: error handling
    let method = Method::from_str(raw_request.method.as_str()).unwrap();
    let mut request = http::Request::builder()
        .method(method)
        .uri(raw_request.uri)
        .version(http::Version::HTTP_11);
    for header in raw_request.headers {
        request = request.header(header.name, header.value);
    }
    request
        .body(RequestChunk {
            content: chunk,
            size: raw_request.chunk_length,
            start: raw_request.chunk_start,
            end_of_stream: raw_request.end_of_stream,
        })
        .unwrap()
}
