wit_bindgen_wasmtime::export!("../../bulwark-host.wit");

use crate::{PluginExecutionError, PluginInstantiationError, PluginLoadError};
use std::path::Path;
use wasmtime::{Config, Engine, Instance, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

pub struct RequestContext {
    wasi: WasiCtx,
    request: bulwark_host::RequestInterface,
    accept: f64,
    restrict: f64,
    unknown: f64,
    tags: Vec<String>,
}
// Owns a single detection plugin and provides the interface between WASM host and guest.
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

pub struct PluginInstance<'a> {
    plugin: &'a Plugin,
    linker: Linker<RequestContext>,
    store: Store<RequestContext>,
    instance: Instance,
}

impl PluginInstance<'_> {
    fn new(
        plugin: &Plugin,
        request: bulwark_host::RequestInterface,
    ) -> Result<PluginInstance, PluginInstantiationError> {
        let mut linker: Linker<RequestContext> = Linker::new(&plugin.engine);
        wasmtime_wasi::add_to_linker(&mut linker, |s| &mut s.wasi)?;

        let wasi = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_args()?
            .build();
        let request_context = RequestContext {
            wasi,
            request,
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

    fn start(&mut self) -> Result<bulwark_host::DecisionInterface, PluginExecutionError> {
        let start = self.instance.get_func(&mut self.store, "_start").unwrap();
        start.call(&mut self.store, &[], &mut [])?;

        let ctx = self.store.data();

        Ok(bulwark_host::DecisionInterface {
            accept: ctx.accept,
            restrict: ctx.restrict,
            unknown: ctx.unknown,
            tags: ctx.tags.iter().map(|s| s.as_str()).collect(),
        })
    }

    fn has_request_decision_handler(&self) -> bool {
        false
    }
}

impl bulwark_host::BulwarkHost for RequestContext {
    fn set_decision(&mut self, decision: bulwark_host::DecisionInterface) {
        self.accept = decision.accept;
        self.restrict = decision.restrict;
        self.unknown = decision.unknown;
        self.tags = decision
            .tags
            .iter()
            .map(|s| String::from(*s))
            .collect::<Vec<String>>();

        // TODO: validate, probably via trait?
    }

    fn get_request(&mut self) -> bulwark_host::RequestInterface {
        self.request.clone()
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
            &plugin,
            bulwark_host::RequestInterface {
                method: String::from("GET"),
                uri: String::from("/"),
                version: String::from("HTTP/1.1"),
                chunk: "".as_bytes().to_vec(),
                headers: vec![],
                chunk_start: 0,
                chunk_length: 0,
                end_of_stream: true,
            },
        )?;
        let decision = plugin_instance.start()?;
        assert_eq!(0.0, decision.accept);
        assert_eq!(0.0, decision.restrict);
        assert_eq!(1.0, decision.unknown);
        assert_eq!(0, decision.tags.len());

        Ok(())
    }

    #[test]
    fn test_wasm_logic() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../test/bulwark-evil-bit.wasm");
        let plugin = std::sync::Arc::new(Plugin::from_bytes(wasm_bytes)?);

        let mut typical_plugin_instance = PluginInstance::new(
            &plugin,
            bulwark_host::RequestInterface {
                method: String::from("POST"),
                uri: String::from("/example"),
                version: String::from("HTTP/1.1"),
                chunk: "{\"number\": 42}".as_bytes().to_vec(),
                headers: vec![bulwark_host::HeaderInterface {
                    name: String::from("Content-Type"),
                    value: String::from("application/json"),
                }],
                chunk_start: 0,
                chunk_length: 14,
                end_of_stream: true,
            },
        )?;
        let typical_decision = typical_plugin_instance.start()?;
        assert_eq!(0.0, typical_decision.accept);
        assert_eq!(0.0, typical_decision.restrict);
        assert_eq!(1.0, typical_decision.unknown);
        assert_eq!(0, typical_decision.tags.len());

        let mut evil_plugin_instance = PluginInstance::new(
            &plugin,
            bulwark_host::RequestInterface {
                method: String::from("POST"),
                uri: String::from("/example"),
                version: String::from("HTTP/1.1"),
                chunk: "{\"number\": 42}".as_bytes().to_vec(),
                headers: vec![
                    bulwark_host::HeaderInterface {
                        name: String::from("Content-Type"),
                        value: String::from("application/json"),
                    },
                    bulwark_host::HeaderInterface {
                        name: String::from("Evil"),
                        value: String::from("true"),
                    },
                ],
                chunk_start: 0,
                chunk_length: 14,
                end_of_stream: true,
            },
        )?;
        let evil_decision = evil_plugin_instance.start()?;
        assert_eq!(0.0, evil_decision.accept);
        assert_eq!(1.0, evil_decision.restrict);
        assert_eq!(0.0, evil_decision.unknown);
        assert_eq!(1, evil_decision.tags.len());
        assert_eq!("evil", evil_decision.tags[0]);

        Ok(())
    }
}
