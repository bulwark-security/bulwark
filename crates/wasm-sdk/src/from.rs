use crate::{Decision, Outcome, Value, Verdict};

// impl From<Request> for crate::bulwark_host::RequestInterface {
//     fn from(request: Request) -> Self {
//         crate::bulwark_host::RequestInterface {
//             method: request.method().to_string(),
//             uri: request.uri().to_string(),
//             version: format!("{:?}", request.version()),
//             headers: request
//                 .headers()
//                 .iter()
//                 .map(|(name, value)| (name.to_string(), value.as_bytes().to_vec()))
//                 .collect(),
//             body_received: request.body().received,
//             chunk: request.body().content.clone(),
//             chunk_start: request.body().start,
//             chunk_length: request.body().size,
//             end_of_stream: request.body().end_of_stream,
//         }
//     }
// }

// impl From<crate::bulwark_host::ResponseInterface> for Response {
//     fn from(response: crate::bulwark_host::ResponseInterface) -> Self {
//         let mut builder = http::response::Builder::new();
//         builder = builder.status::<u16>(response.status.try_into().unwrap());
//         for (name, value) in response.headers {
//             builder = builder.header(name, value);
//         }
//         builder
//             .body(BodyChunk {
//                 received: response.body_received,
//                 end_of_stream: response.end_of_stream,
//                 size: response.chunk.len().try_into().unwrap(),
//                 start: 0,
//                 content: response.chunk,
//             })
//             .unwrap()
//     }
// }

// TODO: can we avoid conversions, perhaps by moving bindgen into lib.rs?

impl From<crate::wit::bulwark::plugin::types::Decision> for Decision {
    fn from(decision: crate::wit::bulwark::plugin::types::Decision) -> Self {
        Decision {
            accept: decision.accepted,
            restrict: decision.restricted,
            unknown: decision.unknown,
        }
    }
}

impl From<Decision> for crate::wit::bulwark::plugin::types::Decision {
    fn from(decision: Decision) -> Self {
        crate::wit::bulwark::plugin::types::Decision {
            accepted: decision.accept,
            restricted: decision.restrict,
            unknown: decision.unknown,
        }
    }
}

impl From<Verdict> for crate::wit::bulwark::plugin::types::Verdict {
    fn from(verdict: Verdict) -> Self {
        crate::wit::bulwark::plugin::types::Verdict {
            outcome: verdict.outcome.into(),
            decision: verdict.decision.into(),
            tags: verdict.tags.clone(),
        }
    }
}

impl From<Outcome> for crate::wit::bulwark::plugin::types::Outcome {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Trusted => crate::wit::bulwark::plugin::types::Outcome::Trusted,
            Outcome::Accepted => crate::wit::bulwark::plugin::types::Outcome::Accepted,
            Outcome::Suspected => crate::wit::bulwark::plugin::types::Outcome::Suspected,
            Outcome::Restricted => crate::wit::bulwark::plugin::types::Outcome::Restricted,
        }
    }
}

impl From<crate::wit::bulwark::plugin::types::Outcome> for Outcome {
    fn from(outcome: crate::wit::bulwark::plugin::types::Outcome) -> Self {
        match outcome {
            crate::wit::bulwark::plugin::types::Outcome::Trusted => Outcome::Trusted,
            crate::wit::bulwark::plugin::types::Outcome::Accepted => Outcome::Accepted,
            crate::wit::bulwark::plugin::types::Outcome::Suspected => Outcome::Suspected,
            crate::wit::bulwark::plugin::types::Outcome::Restricted => Outcome::Restricted,
        }
    }
}

impl From<crate::wit::bulwark::plugin::environment::PrimitiveValue> for Value {
    fn from(value: crate::wit::bulwark::plugin::environment::PrimitiveValue) -> Self {
        match value {
            crate::wit::bulwark::plugin::environment::PrimitiveValue::Null => Value::Null,
            crate::wit::bulwark::plugin::environment::PrimitiveValue::Boolean(b) => Value::Bool(b),
            crate::wit::bulwark::plugin::environment::PrimitiveValue::Num(n) => match n {
                crate::wit::bulwark::plugin::environment::Number::Posint(n) => {
                    Value::Number(serde_json::Number::from(n))
                }
                crate::wit::bulwark::plugin::environment::Number::Negint(n) => {
                    Value::Number(serde_json::Number::from(n))
                }
                crate::wit::bulwark::plugin::environment::Number::Float(n) => {
                    let n = serde_json::Number::from_f64(n);
                    if let Some(n) = n {
                        Value::Number(n)
                    } else {
                        // This shouldn't happen, but if somehow it did, we can't return an error,
                        // so Null is the next best thing.
                        Value::Null
                    }
                }
            },
            crate::wit::bulwark::plugin::environment::PrimitiveValue::Str(s) => Value::String(s),
        }
    }
}

impl From<crate::wit::bulwark::plugin::environment::Value> for Value {
    fn from(value: crate::wit::bulwark::plugin::environment::Value) -> Self {
        match value {
            crate::wit::bulwark::plugin::environment::Value::Null => Value::Null,
            crate::wit::bulwark::plugin::environment::Value::Boolean(b) => Value::Bool(b),
            crate::wit::bulwark::plugin::environment::Value::Num(n) => match n {
                crate::wit::bulwark::plugin::environment::Number::Posint(n) => {
                    Value::Number(serde_json::Number::from(n))
                }
                crate::wit::bulwark::plugin::environment::Number::Negint(n) => {
                    Value::Number(serde_json::Number::from(n))
                }
                crate::wit::bulwark::plugin::environment::Number::Float(n) => {
                    let n = serde_json::Number::from_f64(n);
                    if let Some(n) = n {
                        Value::Number(n)
                    } else {
                        // This shouldn't happen, but if somehow it did, we can't return an error,
                        // so Null is the next best thing.
                        Value::Null
                    }
                }
            },
            crate::wit::bulwark::plugin::environment::Value::Str(s) => Value::String(s),
            crate::wit::bulwark::plugin::environment::Value::Arr(a) => {
                let mut arr: Vec<Value> = Vec::with_capacity(a.len());
                for v in a {
                    arr.push(v.into());
                }
                Value::Array(arr)
            }
            crate::wit::bulwark::plugin::environment::Value::Obj(o) => {
                let mut obj = serde_json::Map::with_capacity(o.len());
                for (k, v) in o {
                    obj.insert(k, v.into());
                }
                Value::Object(obj)
            }
        }
    }
}
