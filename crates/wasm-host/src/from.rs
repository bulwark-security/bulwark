use crate::HandlerOutput;
use bulwark_wasm_sdk::{Decision, Outcome, Verdict};
use std::collections::{HashMap, HashSet};

impl TryFrom<serde_json::Value> for crate::bindings::bulwark::plugin::config::Value {
    type Error = &'static str;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        Ok(match value {
            serde_json::Value::Null => crate::bindings::bulwark::plugin::config::Value::Null,
            serde_json::Value::Bool(b) => {
                crate::bindings::bulwark::plugin::config::Value::Boolean(b)
            }
            serde_json::Value::Number(n) => {
                if let Some(n) = n.as_u64() {
                    crate::bindings::bulwark::plugin::config::Value::Num(
                        crate::bindings::bulwark::plugin::config::Number::Posint(n),
                    )
                } else if let Some(n) = n.as_i64() {
                    // This should only be Some if the number is negative
                    crate::bindings::bulwark::plugin::config::Value::Num(
                        crate::bindings::bulwark::plugin::config::Number::Negint(n),
                    )
                } else {
                    // This should only fall through if the number is not an integer
                    crate::bindings::bulwark::plugin::config::Value::Num(
                        crate::bindings::bulwark::plugin::config::Number::Float(
                            // Error scenario is presumably more likely to be not finite since
                            // all 3 underlying representations can successfully coerce to f64.
                            n.as_f64().ok_or("not finite or not a float")?,
                        ),
                    )
                }
            }
            serde_json::Value::String(s) => crate::bindings::bulwark::plugin::config::Value::Str(s),
            serde_json::Value::Array(a) => {
                let mut values = Vec::with_capacity(a.len());
                for value in a {
                    values.push(
                        crate::bindings::bulwark::plugin::config::PrimitiveValue::try_from(value)?,
                    );
                }
                crate::bindings::bulwark::plugin::config::Value::Arr(values)
            }
            serde_json::Value::Object(o) => {
                let mut obj = HashMap::with_capacity(o.len());
                for (key, value) in o {
                    obj.insert(
                        key,
                        crate::bindings::bulwark::plugin::config::PrimitiveValue::try_from(value)?,
                    );
                }
                crate::bindings::bulwark::plugin::config::Value::Obj(obj.into_iter().collect())
            }
        })
    }
}

impl TryFrom<serde_json::Value> for crate::bindings::bulwark::plugin::config::PrimitiveValue {
    type Error = &'static str;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        Ok(match value {
            serde_json::Value::Null => {
                crate::bindings::bulwark::plugin::config::PrimitiveValue::Null
            }
            serde_json::Value::Bool(b) => {
                crate::bindings::bulwark::plugin::config::PrimitiveValue::Boolean(b)
            }
            serde_json::Value::Number(n) => {
                if let Some(n) = n.as_u64() {
                    crate::bindings::bulwark::plugin::config::PrimitiveValue::Num(
                        crate::bindings::bulwark::plugin::config::Number::Posint(n),
                    )
                } else if let Some(n) = n.as_i64() {
                    // This should only be Some if the number is negative
                    crate::bindings::bulwark::plugin::config::PrimitiveValue::Num(
                        crate::bindings::bulwark::plugin::config::Number::Negint(n),
                    )
                } else {
                    // This should only fall through if the number is not an integer
                    crate::bindings::bulwark::plugin::config::PrimitiveValue::Num(
                        crate::bindings::bulwark::plugin::config::Number::Float(
                            // Error scenario is presumably more likely to be not finite since
                            // all 3 underlying representations can successfully coerce to f64.
                            n.as_f64().ok_or("not finite or not a float")?,
                        ),
                    )
                }
            }
            serde_json::Value::String(s) => {
                crate::bindings::bulwark::plugin::config::PrimitiveValue::Str(s)
            }
            _ => Err("not a primitive value")?,
        })
    }
}

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
