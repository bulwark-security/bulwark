wit_bindgen_wasmtime::export!("../../bulwark-host.wit");

use crate::{PluginExecutionError, PluginInstantiationError, PluginLoadError};
use std::path::Path;
use std::{convert::From, sync::Arc};
use wasmtime::{Config, Engine, Instance, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

impl From<bulwark_wasm_sdk::Request> for bulwark_host::RequestInterface {
    fn from(request: bulwark_wasm_sdk::Request) -> Self {
        bulwark_host::RequestInterface {
            method: request.method().to_string(),
            uri: request.uri().to_string(),
            // version doesn't implement display for some reason
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

pub struct RequestContext {
    wasi: WasiCtx,
    request: bulwark_host::RequestInterface,
    accept: f64,
    restrict: f64,
    unknown: f64,
    tags: Vec<String>,
}
// Owns a single detection plugin and provides the interface between WASM host and guest.
#[derive(Clone)]
pub struct Plugin {
    engine: Engine,
    module: Module,
}

impl Plugin {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PluginLoadError> {
        Self::from_module(|engine| -> Result<Module, PluginLoadError> {
            let module = Module::from_binary(engine, bytes)?;
            Ok(module)
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, PluginLoadError> {
        Self::from_module(|engine| -> Result<Module, PluginLoadError> {
            let module = Module::from_file(engine, &path)?;
            Ok(module)
        })
    }

    fn from_module<F>(mut get_module: F) -> Result<Self, PluginLoadError>
    where
        F: FnMut(&Engine) -> Result<Module, PluginLoadError>,
    {
        let mut config = Config::new();
        config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
        config.wasm_multi_memory(true);
        //config.wasm_module_linking(true);

        let engine = Engine::new(&config)?;
        let module = get_module(&engine)?;

        Ok(Plugin { engine, module })
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
        request: bulwark_wasm_sdk::Request,
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

    // TODO: traits for decision?

    pub fn start(&mut self) -> Result<bulwark_host::DecisionInterface, PluginExecutionError> {
        let start = self.instance.get_func(&mut self.store, "_start").unwrap();
        start.call(&mut self.store, &[], &mut [])?;

        let ctx = self.store.data();

        Ok(bulwark_host::DecisionInterface {
            accept: ctx.accept,
            restrict: ctx.restrict,
            unknown: ctx.unknown,
            // tags: ctx.tags.iter().map(|s| s.as_str()).collect(),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_execution() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../test/bulwark-blank-slate.wasm");
        let plugin = Plugin::from_bytes(wasm_bytes)?;
        let mut plugin_instance = PluginInstance::new(
            Arc::new(plugin),
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
        )?;
        let decision = plugin_instance.start()?;
        assert_eq!(0.0, decision.accept);
        assert_eq!(0.0, decision.restrict);
        assert_eq!(1.0, decision.unknown);

        Ok(())
    }

    #[test]
    fn test_wasm_logic() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../test/bulwark-evil-bit.wasm");
        let plugin = std::sync::Arc::new(Plugin::from_bytes(wasm_bytes)?);

        let mut typical_plugin_instance = PluginInstance::new(
            plugin.clone(),
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
        )?;
        let typical_decision = typical_plugin_instance.start()?;
        assert_eq!(0.0, typical_decision.accept);
        assert_eq!(0.0, typical_decision.restrict);
        assert_eq!(1.0, typical_decision.unknown);

        let mut evil_plugin_instance = PluginInstance::new(
            plugin,
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
        )?;
        let evil_decision = evil_plugin_instance.start()?;
        assert_eq!(0.0, evil_decision.accept);
        assert_eq!(1.0, evil_decision.restrict);
        assert_eq!(0.0, evil_decision.unknown);

        Ok(())
    }
}
