wit_bindgen_wasmtime::export!("../../bulwark-host.wit");

use crate::{
    bulwark_host::{Decision, Request},
    PluginExecutionError, PluginInstantiationError, PluginLoadError,
};
use std::path::Path;
use wasmtime::{Config, Engine, Instance, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

pub struct RequestContext {
    wasi: WasiCtx,
    request: Request,
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

pub struct PluginInstance {
    plugin: Plugin,
    linker: Linker<RequestContext>,
    store: Store<RequestContext>,
    instance: Instance,
}

impl PluginInstance {
    fn new(plugin: Plugin, request: Request) -> Result<PluginInstance, PluginInstantiationError> {
        let mut linker: Linker<RequestContext> = Linker::new(&plugin.engine);
        wasmtime_wasi::add_to_linker(&mut linker, |s| {
            println!("getting wasi context");
            &mut s.wasi
        })?;

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
        bulwark_host::add_to_linker(&mut linker, |ctx: &mut RequestContext| {
            println!("getting request context");
            ctx
        })?;

        // TODO: don't think this is needed?
        //linker.module(&mut store, "", &plugin.module)?;

        let instance = linker.instantiate(&mut store, &plugin.module)?;

        let start = instance.get_func(&mut store, "_start").unwrap();
        start.call(&mut store, &[], &mut [])?;

        Ok(PluginInstance {
            plugin,
            linker,
            store,
            instance,
        })
    }

    fn on_request_decision(&self) -> Result<Decision, PluginExecutionError> {
        Ok(Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        })
    }
}

impl bulwark_host::BulwarkHost for RequestContext {
    fn set_decision(&mut self, decision: Decision) {
        println!("set_decision called");
        self.accept = decision.accept;
        self.restrict = decision.restrict;
        self.unknown = decision.unknown;
        // TODO: validate, probably via trait?
    }

    fn get_request(&mut self) -> Request {
        println!("get_request called");
        Request {
            method: self.request.method.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasi_cap_std_sync::{ambient_authority, Dir, WasiCtxBuilder};

    // TODO:
    // include_bytes!
    // cargo build --package bulwark-wasm-sdk --lib --verbose --target wasm32-unknown-unknown --example evil_param

    // fn default_wasi() -> WasiCtx {
    //     let mut ctx = WasiCtxBuilder::new().inherit_stdio();
    //     ctx = ctx
    //         .preopened_dir(
    //             Dir::open_ambient_dir("./target", ambient_authority()).unwrap(),
    //             "cache",
    //         )
    //         .unwrap();

    //     ctx.build()
    // }

    #[test]
    fn test_wasm_execution() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../test/bulwark-blank-slate.wasm");
        let plugin = Plugin::from_bytes(wasm_bytes)?;
        let plugin_instance = PluginInstance::new(
            plugin,
            Request {
                method: String::from("PUT"),
            },
        )?;

        // let create = || {
        //     crate::instantiate(
        //         wasm,
        //         |linker| {
        //             imports::add_to_linker(
        //                 linker,
        //                 |cx: &mut crate::Context<MyImports>| -> &mut MyImports { &mut cx.imports },
        //             )
        //         },
        //         |store, module, linker| Exports::instantiate(store, module, linker),
        //     )
        // };

        // let (exports, mut store) = create()?;

        // interface::add_to_linker(
        //     linker,
        //     |cx: &mut crate::Context<MyInterface>| -> &mut MyInterface { &mut cx.imports },
        // );

        // let (bindings, instance) = Bindings::instantiate(store, module, linker)?;

        // linker
        //     .get_default(&mut store, "")?
        //     .typed::<(), (), _>(&store)?
        //     .call(&mut store, ())?;

        Ok(())
    }
}
