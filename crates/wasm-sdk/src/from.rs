use {
    crate::{BodyChunk, Decision, Outcome, Request, Response},
    std::net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

impl From<Request> for crate::bulwark_host::RequestInterface {
    fn from(request: Request) -> Self {
        crate::bulwark_host::RequestInterface {
            method: request.method().to_string(),
            uri: request.uri().to_string(),
            version: format!("{:?}", request.version()),
            headers: request
                .headers()
                .iter()
                .map(|(name, value)| (name.to_string(), value.as_bytes().to_vec()))
                .collect(),
            chunk: request.body().content.clone(),
            chunk_start: request.body().start,
            chunk_length: request.body().size,
            end_of_stream: request.body().end_of_stream,
        }
    }
}

impl From<crate::bulwark_host::ResponseInterface> for Response {
    fn from(response: crate::bulwark_host::ResponseInterface) -> Self {
        let mut builder = http::response::Builder::new();
        builder = builder.status::<u16>(response.status.try_into().unwrap());
        for (name, value) in response.headers {
            builder = builder.header(name, value);
        }
        builder
            .body(BodyChunk {
                end_of_stream: true,
                size: response.chunk.len().try_into().unwrap(),
                start: 0,
                content: response.chunk,
            })
            .unwrap()
    }
}

impl From<crate::bulwark_host::IpInterface> for IpAddr {
    fn from(ip: crate::bulwark_host::IpInterface) -> Self {
        match ip {
            crate::bulwark_host::IpInterface::V4(v4) => {
                Self::V4(Ipv4Addr::new(v4.0, v4.1, v4.2, v4.3))
            }
            crate::bulwark_host::IpInterface::V6(v6) => Self::V6(Ipv6Addr::new(
                v6.0, v6.1, v6.2, v6.3, v6.4, v6.5, v6.6, v6.7,
            )),
        }
    }
}

// TODO: can we avoid conversions, perhaps by moving bindgen into lib.rs?

impl From<crate::bulwark_host::DecisionInterface> for Decision {
    fn from(decision: crate::bulwark_host::DecisionInterface) -> Self {
        Decision {
            accept: decision.accept,
            restrict: decision.restrict,
            unknown: decision.unknown,
        }
    }
}

impl From<Decision> for crate::bulwark_host::DecisionInterface {
    fn from(decision: Decision) -> Self {
        crate::bulwark_host::DecisionInterface {
            accept: decision.accept,
            restrict: decision.restrict,
            unknown: decision.unknown,
        }
    }
}

impl From<crate::bulwark_host::OutcomeInterface> for Outcome {
    fn from(outcome: crate::bulwark_host::OutcomeInterface) -> Self {
        match outcome {
            crate::bulwark_host::OutcomeInterface::Trusted => Outcome::Trusted,
            crate::bulwark_host::OutcomeInterface::Accepted => Outcome::Accepted,
            crate::bulwark_host::OutcomeInterface::Suspected => Outcome::Suspected,
            crate::bulwark_host::OutcomeInterface::Restricted => Outcome::Restricted,
        }
    }
}

impl From<&str> for BodyChunk {
    fn from(content: &str) -> Self {
        BodyChunk {
            end_of_stream: true,
            size: content.len() as u64,
            start: 0,
            content: content.as_bytes().to_owned(),
        }
    }
}

impl From<String> for BodyChunk {
    fn from(content: String) -> Self {
        BodyChunk {
            end_of_stream: true,
            size: content.len() as u64,
            start: 0,
            content: content.into_bytes(),
        }
    }
}

impl From<Vec<u8>> for BodyChunk {
    fn from(content: Vec<u8>) -> Self {
        BodyChunk {
            end_of_stream: true,
            size: content.len() as u64,
            start: 0,
            content,
        }
    }
}
