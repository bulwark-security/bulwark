// TODO: switch to wasmtime::component::bindgen!
wit_bindgen_wasmtime::export!("../../bulwark-host.wit");

use crate::{
    ContextInstantiationError, PluginExecutionError, PluginInstantiationError, PluginLoadError,
};
use bulwark_wasm_sdk::{Decision, MassFunction, Outcome};
use chrono::Utc;
use redis::Commands;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::DerefMut;
use std::path::Path;
use std::sync::MutexGuard;
use std::{
    convert::From,
    sync::{Arc, Mutex},
};
use wasmtime::{AsContext, AsContextMut, Config, Engine, Instance, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

extern crate redis;

use self::bulwark_host::{DecisionInterface, HeaderInterface, OutcomeInterface};

impl From<Arc<bulwark_wasm_sdk::Request>> for bulwark_host::RequestInterface {
    fn from(request: Arc<bulwark_wasm_sdk::Request>) -> Self {
        bulwark_host::RequestInterface {
            method: request.method().to_string(),
            uri: request.uri().to_string(),
            version: format!("{:?}", request.version()),
            headers: request
                .headers()
                .iter()
                .map(|(name, value)| bulwark_host::HeaderInterface {
                    name: name.to_string(),
                    value: value.as_bytes().to_vec(),
                })
                .collect(),
            chunk_start: request.body().start,
            chunk_length: request.body().size,
            end_of_stream: request.body().end_of_stream,
            // TODO: figure out how to avoid the copy
            chunk: request.body().content.clone(),
        }
    }
}

impl From<Arc<bulwark_wasm_sdk::Response>> for bulwark_host::ResponseInterface {
    fn from(response: Arc<bulwark_wasm_sdk::Response>) -> Self {
        bulwark_host::ResponseInterface {
            // this unwrap should be okay since a non-zero u16 should always be coercible to u32
            status: response.status().as_u16().try_into().unwrap(),
            headers: response
                .headers()
                .iter()
                .map(|(name, value)| bulwark_host::HeaderInterface {
                    name: name.to_string(),
                    value: value.as_bytes().to_vec(),
                })
                .collect(),
            chunk_start: response.body().start,
            chunk_length: response.body().size,
            end_of_stream: response.body().end_of_stream,
            // TODO: figure out how to avoid the copy
            chunk: response.body().content.clone(),
        }
    }
}

impl From<DecisionInterface> for Decision {
    fn from(decision: DecisionInterface) -> Self {
        Decision::new(decision.accept, decision.restrict, decision.unknown)
    }
}

impl From<Decision> for DecisionInterface {
    fn from(decision: Decision) -> Self {
        DecisionInterface {
            accept: decision.accept(),
            restrict: decision.restrict(),
            unknown: decision.unknown(),
        }
    }
}

impl From<Outcome> for OutcomeInterface {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Trusted => OutcomeInterface::Trusted,
            Outcome::Accepted => OutcomeInterface::Accepted,
            Outcome::Suspected => OutcomeInterface::Suspected,
            Outcome::Restricted => OutcomeInterface::Restricted,
        }
    }
}

pub struct DecisionComponents {
    pub decision: Decision,
    pub tags: Vec<String>,
}

pub struct RedisInfo {
    pub pool: r2d2::Pool<redis::Client>,
    pub registry: ScriptRegistry,
}

pub struct ScriptRegistry {
    increment_rate_limit: redis::Script,
    check_rate_limit: redis::Script,
    increment_breaker: redis::Script,
    check_breaker: redis::Script,
}

impl Default for ScriptRegistry {
    fn default() -> ScriptRegistry {
        ScriptRegistry {
            // TODO: handle overflow errors by expiring everything on overflow and returning nil?
            increment_rate_limit: redis::Script::new(
                r#"
                local counter_key = "rl:" .. KEYS[1]
                local increment_delta = tonumber(ARGV[1])
                local expiration_window = tonumber(ARGV[2])
                local timestamp = tonumber(ARGV[3])
                local expiration_key = counter_key .. ":ex"
                local expiration = tonumber(redis.call("get", expiration_key))
                local next_expiration = timestamp + expiration_window
                if not expiration or timestamp > expiration then
                    redis.call("set", expiration_key, next_expiration)
                    redis.call("set", counter_key, 0)
                    redis.call("expireat", expiration_key, next_expiration + 1)
                    redis.call("expireat", counter_key, next_expiration + 1)
                    expiration = next_expiration
                end
                local attempts = redis.call("incrby", counter_key, increment_delta)
                return { attempts, expiration }
                "#,
            ),
            check_rate_limit: redis::Script::new(
                r#"
                local counter_key = "rl:" .. KEYS[1]
                local expiration_key = counter_key .. ":exp"
                local timestamp = tonumber(ARGV[1])
                local attempts = tonumber(redis.call("get", counter_key))
                local expiration = nil
                if attempts then
                    expiration = tonumber(redis.call("get", expiration_key))
                    if not expiration or timestamp > expiration then
                        attempts = nil
                        expiration = nil
                    end
                end
                return { attempts, expiration }
                "#,
            ),
            increment_breaker: redis::Script::new(
                r#"
                local generation_key = "bk:g:" .. KEYS[1]
                local success_key = "bk:s:" .. KEYS[1]
                local failure_key = "bk:f:" .. KEYS[1]
                local consec_success_key = "bk:cs:" .. KEYS[1]
                local consec_failure_key = "bk:cf:" .. KEYS[1]
                local success_delta = tonumber(ARGV[1])
                local failure_delta = tonumber(ARGV[2])
                local expiration_window = tonumber(ARGV[3])
                local timestamp = tonumber(ARGV[4])
                local expiration = timestamp + expiration_window
                local generation = redis.call("incrby", generation_key, 1)
                local successes = 0
                local failures = 0
                local consec_successes = 0
                local consec_failures = 0
                if success_delta > 0 then
                    successes = redis.call("incrby", success_key, success_delta)
                    failures = tonumber(redis.call("get", failure_key))
                    consec_successes = redis.call("incrby", consec_success_key, success_delta)
                    consec_failures = redis.call("set", consec_failure_key, 0)
                else
                    successes = tonumber(redis.call("get", success_key))
                    failures = redis.call("incrby", failure_key, failure_delta)
                    consec_successes = redis.call("set", consec_success_key, 0)
                    consec_failures = redis.call("incrby", consec_failure_key, failure_delta)
                end
                redis.call("expireat", generation_key, expiration + 1)
                redis.call("expireat", success_key, expiration + 1)
                redis.call("expireat", failure_key, expiration + 1)
                redis.call("expireat", consec_success_key, expiration + 1)
                redis.call("expireat", consec_failure_key, expiration + 1)
                return { generation, successes, failures, consec_successes, consec_failures, expiration }
                "#,
            ),
            check_breaker: redis::Script::new(
                r#"
                local generation_key = "bk:g:" .. KEYS[1]
                local success_key = "bk:s:" .. KEYS[1]
                local failure_key = "bk:f:" .. KEYS[1]
                local consec_success_key = "bk:cs:" .. KEYS[1]
                local consec_failure_key = "bk:cf:" .. KEYS[1]
                local generation = tonumber(redis.call("get", generation_key))
                if not generation then
                    return { nil, nil, nil, nil, nil, nil }
                end
                local successes = tonumber(redis.call("get", success_key))
                local failures = tonumber(redis.call("get", failure_key))
                local consec_successes = tonumber(redis.call("get", consec_success_key))
                local consec_failures = tonumber(redis.call("get", consec_failure_key))
                local expiration = tonumber(redis.call("expiretime", success_key)) - 1
                return { generation, successes, failures, consec_successes, consec_failures, expiration }
                "#,
            ),
        }
    }
}

pub struct RequestContext {
    wasi: WasiCtx,
    plugin_reference: String,
    config: Arc<Vec<u8>>,
    /// params are shared between all plugin instances for a single request
    params: Arc<Mutex<bulwark_wasm_sdk::Map<String, bulwark_wasm_sdk::Value>>>, // TODO: remove Arc?
    request: bulwark_host::RequestInterface,
    redis_info: Option<Arc<RedisInfo>>,
    outbound_http: Arc<Mutex<HashMap<u64, reqwest::blocking::RequestBuilder>>>,
    http_client: reqwest::blocking::Client,
    accept: f64,
    restrict: f64,
    unknown: f64,
    tags: Vec<String>,
    host_mutable_context: HostMutableContext,
}

impl RequestContext {
    pub fn new(
        plugin: Arc<Plugin>,
        redis_info: Option<Arc<RedisInfo>>,
        params: Arc<Mutex<bulwark_wasm_sdk::Map<String, bulwark_wasm_sdk::Value>>>,
        request: Arc<bulwark_wasm_sdk::Request>,
    ) -> Result<RequestContext, ContextInstantiationError> {
        let wasi = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_args()?
            .build();

        Ok(RequestContext {
            wasi,
            redis_info,
            plugin_reference: plugin.reference.clone(),
            config: plugin.config.clone(),
            params,
            request: bulwark_host::RequestInterface::from(request),
            outbound_http: Arc::new(Mutex::new(HashMap::new())),
            http_client: reqwest::blocking::Client::new(),
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
            tags: vec![],
            host_mutable_context: HostMutableContext {
                response: Arc::new(Mutex::new(None)),
                combined_decision: Arc::new(Mutex::new(None)),
                outcome: Arc::new(Mutex::new(None)),
                combined_tags: Arc::new(Mutex::new(None)),
            },
        })
    }
}

// Owns a single detection plugin and provides the interface between WASM host and guest.
#[derive(Clone)]
pub struct Plugin {
    reference: String,
    config: Arc<Vec<u8>>,
    engine: Engine,
    module: Module,
}

impl Plugin {
    pub fn from_wat(name: String, wat: &str, config: Vec<u8>) -> Result<Self, PluginLoadError> {
        Self::from_module(name, config, |engine| -> Result<Module, PluginLoadError> {
            let module = Module::new(engine, wat.as_bytes())?;
            Ok(module)
        })
    }

    pub fn from_bytes(
        name: String,
        bytes: &[u8],
        config: Vec<u8>,
    ) -> Result<Self, PluginLoadError> {
        Self::from_module(name, config, |engine| -> Result<Module, PluginLoadError> {
            let module = Module::from_binary(engine, bytes)?;
            Ok(module)
        })
    }

    pub fn from_file(path: impl AsRef<Path>, config: Vec<u8>) -> Result<Self, PluginLoadError> {
        let name = path.as_ref().display().to_string();
        Self::from_module(name, config, |engine| -> Result<Module, PluginLoadError> {
            let module = Module::from_file(engine, &path)?;
            Ok(module)
        })
    }

    fn from_module<F>(
        reference: String,
        config: Vec<u8>,
        mut get_module: F,
    ) -> Result<Self, PluginLoadError>
    where
        F: FnMut(&Engine) -> Result<Module, PluginLoadError>,
    {
        let mut wasm_config = Config::new();
        wasm_config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
        wasm_config.wasm_multi_memory(true);
        //config.wasm_module_linking(true);

        let engine = Engine::new(&wasm_config)?;
        let module = get_module(&engine)?;

        Ok(Plugin {
            reference,
            config: Arc::new(config),
            engine,
            module,
        })
    }
}

#[derive(Clone)]
struct HostMutableContext {
    response: Arc<Mutex<Option<bulwark_host::ResponseInterface>>>,
    combined_decision: Arc<Mutex<Option<bulwark_host::DecisionInterface>>>,
    combined_tags: Arc<Mutex<Option<Vec<String>>>>,
    outcome: Arc<Mutex<Option<bulwark_host::OutcomeInterface>>>,
}

pub struct PluginInstance {
    plugin: Arc<Plugin>,
    linker: Linker<RequestContext>,
    store: Store<RequestContext>,
    instance: Instance,
    host_mutable_context: HostMutableContext,
}

impl PluginInstance {
    pub fn new(
        plugin: Arc<Plugin>,
        request_context: RequestContext,
    ) -> Result<PluginInstance, PluginInstantiationError> {
        // Clone the host mutable context so that we can make changes to the interior of our request context from the parent.
        let host_mutable_context = request_context.host_mutable_context.clone();

        // convert from normal request struct to wasm request interface
        let mut linker: Linker<RequestContext> = Linker::new(&plugin.engine);
        wasmtime_wasi::add_to_linker(&mut linker, |s| &mut s.wasi)?;

        let mut store = Store::new(&plugin.engine, request_context);
        bulwark_host::add_to_linker(&mut linker, |ctx: &mut RequestContext| ctx)?;

        let instance = linker.instantiate(&mut store, &plugin.module)?;

        Ok(PluginInstance {
            plugin,
            linker,
            store,
            instance,
            host_mutable_context,
        })
    }

    pub fn set_response(&mut self, response: Arc<http::Response<bulwark_wasm_sdk::BodyChunk>>) {
        let mut interior_response = self.host_mutable_context.response.lock().unwrap();
        *interior_response = Some(bulwark_host::ResponseInterface::from(response));
    }

    pub fn set_combined_decision(
        &mut self,
        decision_components: &DecisionComponents,
        outcome: Outcome,
    ) {
        let mut interior_decision = self.host_mutable_context.combined_decision.lock().unwrap();
        *interior_decision = Some(decision_components.decision.into());
        let mut interior_outcome = self.host_mutable_context.outcome.lock().unwrap();
        *interior_outcome = Some(outcome.into());
    }

    pub fn plugin_reference(&self) -> String {
        self.plugin.reference.clone()
    }

    pub fn start(&mut self) -> Result<(), PluginExecutionError> {
        const FN_NAME: &str = "_start";
        let fn_ref = self.instance.get_func(self.store.as_context_mut(), FN_NAME);
        fn_ref
            .ok_or(PluginExecutionError::NotImplementedError {
                expected: FN_NAME.to_string(),
            })?
            .call(self.store.as_context_mut(), &[], &mut [])?;

        Ok(())
    }

    pub fn has_request_handler(&mut self) -> bool {
        self.instance
            .get_func(self.store.as_context_mut(), "on_request")
            .is_some()
    }

    pub fn handle_request(&mut self) -> Result<(), PluginExecutionError> {
        const FN_NAME: &str = "on_request";
        let fn_ref = self.instance.get_func(self.store.as_context_mut(), FN_NAME);
        fn_ref
            .ok_or(PluginExecutionError::NotImplementedError {
                expected: FN_NAME.to_string(),
            })?
            .call(self.store.as_context_mut(), &[], &mut [])?;

        Ok(())
    }

    pub fn has_request_decision_handler(&mut self) -> bool {
        self.instance
            .get_func(self.store.as_context_mut(), "on_request_decision")
            .is_some()
    }

    pub fn handle_request_decision(&mut self) -> Result<(), PluginExecutionError> {
        const FN_NAME: &str = "on_request_decision";
        let fn_ref = self.instance.get_func(self.store.as_context_mut(), FN_NAME);
        fn_ref
            .ok_or(PluginExecutionError::NotImplementedError {
                expected: FN_NAME.to_string(),
            })?
            .call(self.store.as_context_mut(), &[], &mut [])?;

        Ok(())
    }

    pub fn has_response_decision_handler(&mut self) -> bool {
        self.instance
            .get_func(self.store.as_context_mut(), "on_response_decision")
            .is_some()
    }

    pub fn handle_response_decision(&mut self) -> Result<(), PluginExecutionError> {
        const FN_NAME: &str = "on_response_decision";
        let fn_ref = self.instance.get_func(self.store.as_context_mut(), FN_NAME);
        fn_ref
            .ok_or(PluginExecutionError::NotImplementedError {
                expected: FN_NAME.to_string(),
            })?
            .call(self.store.as_context_mut(), &[], &mut [])?;

        Ok(())
    }

    pub fn has_decision_feedback_handler(&mut self) -> bool {
        self.instance
            .get_func(self.store.as_context_mut(), "on_decision_feedback")
            .is_some()
    }

    pub fn handle_decision_feedback(&mut self) -> Result<(), PluginExecutionError> {
        const FN_NAME: &str = "on_decision_feedback";
        let fn_ref = self.instance.get_func(self.store.as_context_mut(), FN_NAME);
        fn_ref
            .ok_or(PluginExecutionError::NotImplementedError {
                expected: FN_NAME.to_string(),
            })?
            .call(self.store.as_context_mut(), &[], &mut [])?;

        Ok(())
    }

    pub fn get_decision(&mut self) -> DecisionComponents {
        let ctx = self.store.data();

        DecisionComponents {
            decision: Decision {
                accept: ctx.accept,
                restrict: ctx.restrict,
                unknown: ctx.unknown,
            },
            tags: ctx.tags.clone(),
        }
    }
}

impl bulwark_host::BulwarkHost for RequestContext {
    fn get_config(&mut self) -> Vec<u8> {
        self.config.to_vec()
    }

    fn get_param_value(&mut self, key: &str) -> Vec<u8> {
        let params = self.params.lock().unwrap();
        let value = params.get(key).unwrap_or(&bulwark_wasm_sdk::Value::Null);
        serde_json::to_vec(value).unwrap()
    }

    fn set_param_value(&mut self, key: &str, value: &[u8]) {
        let mut params = self.params.lock().unwrap();
        let value: bulwark_wasm_sdk::Value = serde_json::from_slice(value).unwrap();
        params.insert(key.to_string(), value);
    }

    fn get_request(&mut self) -> bulwark_host::RequestInterface {
        self.request.clone()
    }

    fn get_response(&mut self) -> bulwark_host::ResponseInterface {
        let response: MutexGuard<Option<bulwark_host::ResponseInterface>> =
            self.host_mutable_context.response.lock().unwrap();
        response.to_owned().unwrap()
    }

    fn set_decision(&mut self, decision: bulwark_host::DecisionInterface) {
        self.accept = decision.accept;
        self.restrict = decision.restrict;
        self.unknown = decision.unknown;
        // TODO: validate, probably via trait?
    }

    fn get_combined_decision(&mut self) -> bulwark_host::DecisionInterface {
        let combined_decision: MutexGuard<Option<bulwark_host::DecisionInterface>> =
            self.host_mutable_context.combined_decision.lock().unwrap();
        combined_decision.to_owned().unwrap()
    }

    fn get_combined_tags(&mut self) -> Vec<String> {
        let combined_tags: MutexGuard<Option<Vec<String>>> =
            self.host_mutable_context.combined_tags.lock().unwrap();
        combined_tags.to_owned().unwrap()
    }

    fn get_outcome(&mut self) -> bulwark_host::OutcomeInterface {
        let outcome: MutexGuard<Option<bulwark_host::OutcomeInterface>> =
            self.host_mutable_context.outcome.lock().unwrap();
        outcome.to_owned().unwrap()
    }

    fn set_tags(&mut self, tags: Vec<&str>) {
        self.tags = tags
            .iter()
            .map(|s| String::from(*s))
            .collect::<Vec<String>>();
    }

    fn get_remote_state(&mut self, key: &str) -> Vec<u8> {
        let pool = &self.redis_info.clone().unwrap().pool;
        let mut conn = pool.get().unwrap();
        conn.get(key).unwrap()
    }

    fn set_remote_state(&mut self, key: &str, value: &[u8]) {
        let pool = &self.redis_info.clone().unwrap().pool;
        let mut conn = pool.get().unwrap();
        conn.set(key, value.to_vec()).unwrap()
    }

    fn increment_remote_state(&mut self, key: &str) -> i64 {
        let pool = &self.redis_info.clone().unwrap().pool;
        let mut conn = pool.get().unwrap();
        conn.incr(key, 1).unwrap()
    }

    fn increment_remote_state_by(&mut self, key: &str, delta: i64) -> i64 {
        let pool = &self.redis_info.clone().unwrap().pool;
        let mut conn = pool.get().unwrap();
        conn.incr(key, delta).unwrap()
    }

    fn set_remote_ttl(&mut self, key: &str, ttl: i64) {
        let pool = &self.redis_info.clone().unwrap().pool;
        let mut conn = pool.get().unwrap();
        conn.expire(key, ttl.try_into().unwrap()).unwrap()
    }

    fn prepare_request(&mut self, method: &str, uri: &str) -> u64 {
        let mut outbound_requests = self.outbound_http.lock().unwrap();
        let builder = self.http_client.request(reqwest::Method::GET, uri);
        let index: u64 = outbound_requests.len().try_into().unwrap();
        outbound_requests.insert(index, builder);
        (outbound_requests.len() - 1).try_into().unwrap()
    }

    fn add_request_header(&mut self, request_id: u64, name: &str, value: &[u8]) {
        let mut outbound_requests = self.outbound_http.lock().unwrap();
        // remove/insert to avoid move issues
        let mut builder = outbound_requests.remove(&request_id).unwrap();
        builder = builder.header(name, value);
        outbound_requests.insert(request_id, builder);
    }

    fn set_request_body(
        &mut self,
        request_id: u64,
        body: &[u8],
    ) -> bulwark_host::ResponseInterface {
        let mut outbound_requests = self.outbound_http.lock().unwrap();
        // remove/insert to avoid move issues
        let builder = outbound_requests.remove(&request_id).unwrap();
        let builder = builder.body(body.to_vec());

        let response = builder.send().unwrap();
        let status: u32 = response.status().as_u16().try_into().unwrap();
        // need to read headers before body because retrieving body bytes will move the response
        let headers: Vec<HeaderInterface> = response
            .headers()
            .iter()
            .map(|(name, value)| HeaderInterface {
                name: name.to_string(),
                value: value.as_bytes().to_vec(),
            })
            .collect();
        let body = response.bytes().unwrap().to_vec();
        let content_length: u64 = body.len().try_into().unwrap();
        bulwark_host::ResponseInterface {
            status,
            headers,
            chunk: body,
            chunk_start: 0,
            chunk_length: content_length,
            end_of_stream: true,
        }
    }

    fn increment_rate_limit(
        &mut self,
        key: &str,
        delta: i64,
        window: i64,
    ) -> bulwark_host::RateInterface {
        let redis_info = self.redis_info.clone().unwrap();
        let mut conn = redis_info.pool.get().unwrap();
        let dt = Utc::now();
        let timestamp: i64 = dt.timestamp();
        let script = redis_info.registry.increment_rate_limit.clone();
        let (attempts, expiration) = script
            .key(key)
            .arg(delta)
            .arg(window)
            .arg(timestamp)
            .invoke::<(i64, i64)>(conn.deref_mut())
            .unwrap();
        bulwark_host::RateInterface {
            attempts,
            expiration,
        }
    }

    fn check_rate_limit(&mut self, key: &str) -> bulwark_host::RateInterface {
        let redis_info = self.redis_info.clone().unwrap();
        let mut conn = redis_info.pool.get().unwrap();
        let dt = Utc::now();
        let timestamp: i64 = dt.timestamp();
        let script = redis_info.registry.check_rate_limit.clone();
        let (attempts, expiration) = script
            .key(key)
            .arg(timestamp)
            .invoke::<(i64, i64)>(conn.deref_mut())
            .unwrap();
        bulwark_host::RateInterface {
            attempts,
            expiration,
        }
    }

    fn increment_breaker(
        &mut self,
        key: &str,
        success_delta: i64,
        failure_delta: i64,
        window: i64,
    ) -> bulwark_host::BreakerInterface {
        let redis_info = self.redis_info.clone().unwrap();
        let mut conn = redis_info.pool.get().unwrap();
        let dt = Utc::now();
        let timestamp: i64 = dt.timestamp();
        let script = redis_info.registry.increment_breaker.clone();
        let (
            generation,
            successes,
            failures,
            consecutive_successes,
            consecutive_failures,
            expiration,
        ) = script
            .key(key)
            .arg(success_delta)
            .arg(failure_delta)
            .arg(window)
            .arg(timestamp)
            .invoke::<(i64, i64, i64, i64, i64, i64)>(conn.deref_mut())
            .unwrap();
        bulwark_host::BreakerInterface {
            generation,
            successes,
            failures,
            consecutive_successes,
            consecutive_failures,
            expiration,
        }
    }

    fn check_breaker(&mut self, key: &str) -> bulwark_host::BreakerInterface {
        let redis_info = self.redis_info.clone().unwrap();
        let mut conn = redis_info.pool.get().unwrap();
        let dt = Utc::now();
        let timestamp: i64 = dt.timestamp();
        let script = redis_info.registry.check_breaker.clone();
        let (
            generation,
            successes,
            failures,
            consecutive_successes,
            consecutive_failures,
            expiration,
        ) = script
            .key(key)
            .arg(timestamp)
            .invoke::<(i64, i64, i64, i64, i64, i64)>(conn.deref_mut())
            .unwrap();
        bulwark_host::BreakerInterface {
            generation,
            successes,
            failures,
            consecutive_successes,
            consecutive_failures,
            expiration,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_execution() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../tests/bulwark-blank-slate.wasm");
        let plugin = Arc::new(Plugin::from_bytes(
            "bulwark-blank-slate.wasm".to_string(),
            wasm_bytes,
            vec![],
        )?);
        let request = Arc::new(
            http::Request::builder()
                .method("GET")
                .uri("/")
                .version(http::Version::HTTP_11)
                .body(bulwark_wasm_sdk::BodyChunk {
                    content: vec![],
                    start: 0,
                    size: 0,
                    end_of_stream: true,
                })?,
        );
        let params = Arc::new(Mutex::new(bulwark_wasm_sdk::Map::new()));
        let request_context = RequestContext::new(plugin.clone(), None, params, request.clone())?;
        let mut plugin_instance = PluginInstance::new(plugin.clone(), request_context)?;
        plugin_instance.start()?;
        let decision_components = plugin_instance.get_decision();
        assert_eq!(decision_components.decision.accept, 0.0);
        assert_eq!(decision_components.decision.restrict, 0.0);
        assert_eq!(decision_components.decision.unknown, 1.0);
        assert_eq!(decision_components.tags, vec![""; 0]);

        Ok(())
    }

    #[test]
    fn test_wasm_logic() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../tests/bulwark-evil-bit.wasm");
        let plugin = Arc::new(Plugin::from_bytes(
            "bulwark-evil-bit.wasm".to_string(),
            wasm_bytes,
            vec![],
        )?);

        let request = Arc::new(
            http::Request::builder()
                .method("POST")
                .uri("/example")
                .version(http::Version::HTTP_11)
                .header("Content-Type", "application/json")
                .body(bulwark_wasm_sdk::BodyChunk {
                    content: "{\"number\": 42}".as_bytes().to_vec(),
                    start: 0,
                    size: 14,
                    end_of_stream: true,
                })?,
        );
        let params = Arc::new(Mutex::new(bulwark_wasm_sdk::Map::new()));
        let request_context = RequestContext::new(plugin.clone(), None, params, request.clone())?;
        let mut typical_plugin_instance = PluginInstance::new(plugin.clone(), request_context)?;
        typical_plugin_instance.start()?;
        let typical_decision = typical_plugin_instance.get_decision();
        assert_eq!(typical_decision.decision.accept, 0.0);
        assert_eq!(typical_decision.decision.restrict, 0.0);
        assert_eq!(typical_decision.decision.unknown, 1.0);
        assert_eq!(typical_decision.tags, vec![""; 0]);

        let request = Arc::new(
            http::Request::builder()
                .method("POST")
                .uri("/example")
                .version(http::Version::HTTP_11)
                .header("Content-Type", "application/json")
                .header("Evil", "true")
                .body(bulwark_wasm_sdk::BodyChunk {
                    content: "{\"number\": 42}".as_bytes().to_vec(),
                    start: 0,
                    size: 14,
                    end_of_stream: true,
                })?,
        );
        let params = Arc::new(Mutex::new(bulwark_wasm_sdk::Map::new()));
        let request_context = RequestContext::new(plugin.clone(), None, params, request.clone())?;
        let mut evil_plugin_instance = PluginInstance::new(plugin, request_context)?;
        evil_plugin_instance.start()?;
        let evil_decision = evil_plugin_instance.get_decision();
        assert_eq!(evil_decision.decision.accept, 0.0);
        assert_eq!(evil_decision.decision.restrict, 1.0);
        assert_eq!(evil_decision.decision.unknown, 0.0);
        assert_eq!(evil_decision.tags, vec!["evil"]);

        Ok(())
    }
}
