// TODO: the host/guest wit files seem to be why the latest version switched to one generate macro?

wit_bindgen_rust::import!("../../bulwark-host.wit");

use std::str::FromStr;

pub use crate::Decision;
pub use bulwark_host::set_tags; // TODO: use BTreeSet for merging sorted tag lists
pub use http::{Extensions, Method, Uri, Version};
use validator::{Validate, ValidationErrors};

use self::bulwark_host::DecisionInterface;

pub type Request = http::Request<RequestChunk>;

pub struct RequestChunk {
    pub end_of_stream: bool,
    pub size: u64,
    pub start: u64,
    pub content: Vec<u8>,
}

// TODO: might need either get_remote_addr or an extension on the request for non-forwarded IP address

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
        .body(RequestChunk {
            content: chunk,
            size: raw_request.chunk_length,
            start: raw_request.chunk_start,
            end_of_stream: raw_request.end_of_stream,
        })
        .unwrap()
}

// TODO: get_response

pub fn set_decision(decision: Decision) -> Result<(), ValidationErrors> {
    decision.validate()?;
    bulwark_host::set_decision(DecisionInterface {
        accept: decision.accept,
        restrict: decision.restrict,
        unknown: decision.unknown,
    });
    Ok(())
}
