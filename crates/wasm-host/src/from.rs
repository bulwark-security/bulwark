use crate::HandlerOutput;
use bulwark_wasm_sdk::{Decision, Outcome, Verdict};
use std::collections::{HashMap, HashSet};

// impl From<Arc<bulwark_wasm_sdk::Request>> for bulwark_host::RequestInterface {
//     fn from(request: Arc<bulwark_wasm_sdk::Request>) -> Self {
//         bulwark_host::RequestInterface {
//             method: request.method().to_string(),
//             uri: request.uri().to_string(),
//             version: format!("{:?}", request.version()),
//             headers: request
//                 .headers()
//                 .iter()
//                 .map(|(name, value)| (name.to_string(), value.as_bytes().to_vec()))
//                 .collect(),
//             body_received: request.body().received,
//             chunk_start: request.body().start,
//             chunk_length: request.body().size,
//             end_of_stream: request.body().end_of_stream,
//             // TODO: figure out how to avoid the copy
//             chunk: request.body().content.clone(),
//         }
//     }
// }

// impl From<Arc<bulwark_wasm_sdk::Response>> for bulwark_host::ResponseInterface {
//     fn from(response: Arc<bulwark_wasm_sdk::Response>) -> Self {
//         bulwark_host::ResponseInterface {
//             // this unwrap should be okay since a non-zero u16 should always be coercible to u32
//             status: response.status().as_u16().try_into().unwrap(),
//             headers: response
//                 .headers()
//                 .iter()
//                 .map(|(name, value)| (name.to_string(), value.as_bytes().to_vec()))
//                 .collect(),
//             body_received: response.body().received,
//             chunk_start: response.body().start,
//             chunk_length: response.body().size,
//             end_of_stream: response.body().end_of_stream,
//             // TODO: figure out how to avoid the copy
//             chunk: response.body().content.clone(),
//         }
//     }
// }

impl From<crate::bindings::bulwark::plugin::types::Decision> for Decision {
    fn from(decision: crate::bindings::bulwark::plugin::types::Decision) -> Self {
        Decision {
            accept: decision.accepted,
            restrict: decision.restricted,
            unknown: decision.unknown,
        }
    }
}

impl From<Decision> for crate::bindings::bulwark::plugin::types::Decision {
    fn from(decision: Decision) -> Self {
        crate::bindings::bulwark::plugin::types::Decision {
            accepted: decision.accept,
            restricted: decision.restrict,
            unknown: decision.unknown,
        }
    }
}

impl From<Verdict> for crate::bindings::bulwark::plugin::types::Verdict {
    fn from(verdict: Verdict) -> Self {
        crate::bindings::bulwark::plugin::types::Verdict {
            outcome: verdict.outcome.into(),
            decision: verdict.decision.into(),
            tags: verdict.tags.clone(),
        }
    }
}

impl From<Outcome> for crate::bindings::bulwark::plugin::types::Outcome {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Trusted => crate::bindings::bulwark::plugin::types::Outcome::Trusted,
            Outcome::Accepted => crate::bindings::bulwark::plugin::types::Outcome::Accepted,
            Outcome::Suspected => crate::bindings::bulwark::plugin::types::Outcome::Suspected,
            Outcome::Restricted => crate::bindings::bulwark::plugin::types::Outcome::Restricted,
        }
    }
}

impl From<crate::bindings::bulwark::plugin::types::HandlerOutput> for HandlerOutput {
    fn from(output: crate::bindings::bulwark::plugin::types::HandlerOutput) -> Self {
        Self {
            decision: output.decision.into(),
            tags: HashSet::from_iter(output.tags),
            params: HashMap::from_iter(output.params),
        }
    }
}
