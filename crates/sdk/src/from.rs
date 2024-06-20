use crate::{Decision, Outcome, Value, Verdict};

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
            count: verdict.count,
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

impl From<crate::wit::bulwark::plugin::config::PrimitiveValue> for Value {
    fn from(value: crate::wit::bulwark::plugin::config::PrimitiveValue) -> Self {
        match value {
            crate::wit::bulwark::plugin::config::PrimitiveValue::Null => Value::Null,
            crate::wit::bulwark::plugin::config::PrimitiveValue::Boolean(b) => Value::Bool(b),
            crate::wit::bulwark::plugin::config::PrimitiveValue::Num(n) => match n {
                crate::wit::bulwark::plugin::config::Number::Posint(n) => {
                    Value::Number(serde_json::Number::from(n))
                }
                crate::wit::bulwark::plugin::config::Number::Negint(n) => {
                    Value::Number(serde_json::Number::from(n))
                }
                crate::wit::bulwark::plugin::config::Number::Float(n) => {
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
            crate::wit::bulwark::plugin::config::PrimitiveValue::Str(s) => Value::String(s),
        }
    }
}

impl From<crate::wit::bulwark::plugin::config::Value> for Value {
    fn from(value: crate::wit::bulwark::plugin::config::Value) -> Self {
        match value {
            crate::wit::bulwark::plugin::config::Value::Null => Value::Null,
            crate::wit::bulwark::plugin::config::Value::Boolean(b) => Value::Bool(b),
            crate::wit::bulwark::plugin::config::Value::Num(n) => match n {
                crate::wit::bulwark::plugin::config::Number::Posint(n) => {
                    Value::Number(serde_json::Number::from(n))
                }
                crate::wit::bulwark::plugin::config::Number::Negint(n) => {
                    Value::Number(serde_json::Number::from(n))
                }
                crate::wit::bulwark::plugin::config::Number::Float(n) => {
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
            crate::wit::bulwark::plugin::config::Value::Str(s) => Value::String(s),
            crate::wit::bulwark::plugin::config::Value::Arr(a) => {
                let mut arr: Vec<Value> = Vec::with_capacity(a.len());
                for v in a {
                    arr.push(v.into());
                }
                Value::Array(arr)
            }
            crate::wit::bulwark::plugin::config::Value::Obj(o) => {
                let mut obj = serde_json::Map::with_capacity(o.len());
                for (k, v) in o {
                    obj.insert(k, v.into());
                }
                Value::Object(obj)
            }
        }
    }
}
