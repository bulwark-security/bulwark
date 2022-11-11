use wasmtime::Linker;

use crate::interface;
use crate::interface::{Decision, Request};
use crate::PluginLoadError;
//use bulwark_wasm_sdk::types::{Decision, Status};
use std::path::Path;
// use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::{Engine, Module, Store};
use wasmtime_wasi::{add_to_linker, WasiCtx, WasiCtxBuilder};

// Owns a single detection plugin and provides the interface between WASM host and guest.
pub struct Plugin {
    engine: Engine,
    linker: Linker<WasiCtx>,
    module: Module,
    store: Store<WasiCtx>,
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
        let engine = Engine::default();
        let mut linker: Linker<WasiCtx> = Linker::new(&engine);
        add_to_linker(&mut linker, |s| s)?;
        let module = get_module(&engine)?;
        let wasi = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_args()?
            .build();
        let mut store = Store::new(&engine, wasi);
        linker.module(&mut store, "", &module)?;
        Ok(Plugin {
            engine,
            linker,
            module,
            store,
        })
    }
}

#[derive(Default)]
pub struct MyInterface {}

impl interface::Interface for MyInterface {
    fn set_decision(&mut self, _d: Decision) -> std::result::Result<(), anyhow::Error> {
        println!("set_decision called");
        todo!()
    }

    fn get_request(&mut self) -> Result<Request, anyhow::Error> {
        println!("get_request called");
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::Bindings;

    use super::*;

    // TODO:
    // include_bytes!
    // cargo build --package bulwark-wasm-sdk --lib --verbose --target wasm32-unknown-unknown --example evil_param

    #[test]
    fn test_wasm_execution() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../test/bulwark-blank-slate.wasm");
        let plugin = Plugin::from_bytes(wasm_bytes)?;
        let module = plugin.module;
        let linker = plugin.linker;
        let mut store = plugin.store;

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

        interface::add_to_linker(
            linker,
            |cx: &mut crate::Context<MyInterface>| -> &mut MyInterface { &mut cx.imports },
        );

        // let (bindings, instance) = Bindings::instantiate(store, module, linker)?;

        // linker
        //     .get_default(&mut store, "")?
        //     .typed::<(), (), _>(&store)?
        //     .call(&mut store, ())?;

        Ok(())
    }
}
