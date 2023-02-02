// TODO: switch to wasmtime::component::bindgen!
wit_bindgen_wasmtime::export!("../../bulwark-host.wit");

use crate::{PluginExecutionError, PluginInstantiationError, PluginLoadError};
use bulwark_wasm_sdk::{Decision, MassFunction};
use chrono::Utc;
use redis::Commands;
use std::ops::DerefMut;
use std::path::Path;
use std::{convert::From, sync::Arc};
use wasmtime::{Config, Engine, Instance, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

extern crate redis;

use self::bulwark_host::DecisionInterface;

impl From<Arc<bulwark_wasm_sdk::Request>> for bulwark_host::RequestInterface {
    fn from(request: Arc<bulwark_wasm_sdk::Request>) -> Self {
        bulwark_host::RequestInterface {
            method: request.method().to_string(),
            uri: request.uri().to_string(),
            version: format!("{:?}", request.version()),
            headers: request
                .headers()
                .iter()
                .map(|(name, value)| {
                    bulwark_host::HeaderInterface {
                        name: name.to_string(),
                        // TODO: header values might not be valid unicode
                        value: String::from(value.to_str().unwrap()),
                    }
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
}

impl Default for ScriptRegistry {
    fn default() -> ScriptRegistry {
        ScriptRegistry {
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
        }
    }
}

pub struct RequestContext {
    wasi: WasiCtx,
    request: bulwark_host::RequestInterface,
    redis_info: Option<Arc<RedisInfo>>,
    accept: f64,
    restrict: f64,
    unknown: f64,
    tags: Vec<String>,
}

// Owns a single detection plugin and provides the interface between WASM host and guest.
#[derive(Clone)]
pub struct Plugin {
    name: String,
    engine: Engine,
    module: Module,
    // TODO: plugins need to carry configuration and their name around
}

impl Plugin {
    pub fn from_wat(name: String, wat: &str) -> Result<Self, PluginLoadError> {
        Self::from_module(name, |engine| -> Result<Module, PluginLoadError> {
            let module = Module::new(engine, wat.as_bytes())?;
            Ok(module)
        })
    }

    pub fn from_bytes(name: String, bytes: &[u8]) -> Result<Self, PluginLoadError> {
        Self::from_module(name, |engine| -> Result<Module, PluginLoadError> {
            let module = Module::from_binary(engine, bytes)?;
            Ok(module)
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, PluginLoadError> {
        let name = path.as_ref().display().to_string();
        Self::from_module(name, |engine| -> Result<Module, PluginLoadError> {
            let module = Module::from_file(engine, &path)?;
            Ok(module)
        })
    }

    fn from_module<F>(name: String, mut get_module: F) -> Result<Self, PluginLoadError>
    where
        F: FnMut(&Engine) -> Result<Module, PluginLoadError>,
    {
        let mut config = Config::new();
        config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
        config.wasm_multi_memory(true);
        //config.wasm_module_linking(true);

        let engine = Engine::new(&config)?;
        let module = get_module(&engine)?;

        Ok(Plugin {
            name,
            engine,
            module,
        })
    }
}

pub struct PluginInstance {
    plugin: Arc<Plugin>,
    linker: Linker<RequestContext>,
    store: Store<RequestContext>,
    instance: Instance,
}

impl PluginInstance {
    pub fn new(
        plugin: Arc<Plugin>,
        redis_info: Option<Arc<RedisInfo>>,
        request: Arc<bulwark_wasm_sdk::Request>,
    ) -> Result<PluginInstance, PluginInstantiationError> {
        // convert from normal request struct to wasm request interface
        let mut linker: Linker<RequestContext> = Linker::new(&plugin.engine);
        wasmtime_wasi::add_to_linker(&mut linker, |s| &mut s.wasi)?;

        let wasi = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_args()?
            .build();
        let request_context = RequestContext {
            wasi,
            redis_info,
            request: bulwark_host::RequestInterface::from(request),
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
            tags: vec![],
        };
        let mut store = Store::new(&plugin.engine, request_context);
        bulwark_host::add_to_linker(&mut linker, |ctx: &mut RequestContext| ctx)?;

        let instance = linker.instantiate(&mut store, &plugin.module)?;

        Ok(PluginInstance {
            plugin,
            linker,
            store,
            instance,
        })
    }

    pub fn plugin_name(&self) -> String {
        self.plugin.name.clone()
    }

    // TODO: traits for decision?

    pub fn start(&mut self) -> Result<DecisionComponents, PluginExecutionError> {
        let start = self.instance.get_func(&mut self.store, "_start").unwrap();
        start.call(&mut self.store, &[], &mut [])?;

        let ctx = self.store.data();

        Ok(DecisionComponents {
            decision: Decision {
                accept: ctx.accept,
                restrict: ctx.restrict,
                unknown: ctx.unknown,
            },
            tags: ctx.tags.clone(),
        })
    }

    fn has_request_decision_handler(&self) -> bool {
        false
    }
}

impl bulwark_host::BulwarkHost for RequestContext {
    fn get_request(&mut self) -> bulwark_host::RequestInterface {
        self.request.clone()
    }

    fn set_decision(&mut self, decision: bulwark_host::DecisionInterface) {
        self.accept = decision.accept;
        self.restrict = decision.restrict;
        self.unknown = decision.unknown;
        // self.tags = decision
        //     .tags
        //     .iter()
        //     .map(|s| String::from(*s))
        //     .collect::<Vec<String>>();

        // TODO: validate, probably via trait?
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_execution() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../tests/bulwark-blank-slate.wasm");
        let plugin = Plugin::from_bytes("bulwark-blank-slate.wasm".to_string(), wasm_bytes)?;
        let mut plugin_instance = PluginInstance::new(
            Arc::new(plugin),
            None,
            Arc::new(
                http::Request::builder()
                    .method("GET")
                    .uri("/")
                    .version(http::Version::HTTP_11)
                    .body(bulwark_wasm_sdk::RequestChunk {
                        content: vec![],
                        start: 0,
                        size: 0,
                        end_of_stream: true,
                    })?,
            ),
        )?;
        let decision_components = plugin_instance.start()?;
        assert_eq!(decision_components.decision.accept, 0.0);
        assert_eq!(decision_components.decision.restrict, 0.0);
        assert_eq!(decision_components.decision.unknown, 1.0);
        assert_eq!(decision_components.tags, vec![""; 0]);

        Ok(())
    }

    #[test]
    fn test_wasm_logic() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../tests/bulwark-evil-bit.wasm");
        let plugin = std::sync::Arc::new(Plugin::from_bytes(
            "bulwark-evil-bit.wasm".to_string(),
            wasm_bytes,
        )?);

        let mut typical_plugin_instance = PluginInstance::new(
            plugin.clone(),
            None,
            Arc::new(
                http::Request::builder()
                    .method("POST")
                    .uri("/example")
                    .version(http::Version::HTTP_11)
                    .header("Content-Type", "application/json")
                    .body(bulwark_wasm_sdk::RequestChunk {
                        content: "{\"number\": 42}".as_bytes().to_vec(),
                        start: 0,
                        size: 14,
                        end_of_stream: true,
                    })?,
            ),
        )?;
        let typical_decision = typical_plugin_instance.start()?;
        assert_eq!(typical_decision.decision.accept, 0.0);
        assert_eq!(typical_decision.decision.restrict, 0.0);
        assert_eq!(typical_decision.decision.unknown, 1.0);
        assert_eq!(typical_decision.tags, vec![""; 0]);

        let mut evil_plugin_instance = PluginInstance::new(
            plugin,
            None,
            Arc::new(
                http::Request::builder()
                    .method("POST")
                    .uri("/example")
                    .version(http::Version::HTTP_11)
                    .header("Content-Type", "application/json")
                    .header("Evil", "true")
                    .body(bulwark_wasm_sdk::RequestChunk {
                        content: "{\"number\": 42}".as_bytes().to_vec(),
                        start: 0,
                        size: 14,
                        end_of_stream: true,
                    })?,
            ),
        )?;
        let evil_decision = evil_plugin_instance.start()?;
        assert_eq!(evil_decision.decision.accept, 0.0);
        assert_eq!(evil_decision.decision.restrict, 1.0);
        assert_eq!(evil_decision.decision.unknown, 0.0);
        assert_eq!(evil_decision.tags, vec!["evil"]);

        Ok(())
    }
}
