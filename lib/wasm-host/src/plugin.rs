use crate::{AllocationError, PluginLoadError};
use bulwark_wasm_sdk::types::{Decision, Status};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use wasmer::{
    imports, AsStoreMut, Function, FunctionEnv, Imports, Instance, Module, RuntimeError, Store,
    TypedFunction,
};

// plugin owns vm, has many contexts
// start with one store for each plugin, maybe change later

#[derive(Clone)]
struct Env {}

// owns a collection of bulwark components and wraps a wasm store
pub struct Plugin {
    plugin_inner: Arc<PluginInner>,
}

impl Plugin {
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, PluginLoadError> {
        let plugin_inner = PluginInner::new(|store: &Store| -> Result<Module, PluginLoadError> {
            let module = Module::new(&store, &bytes)?;
            Ok(module)
        })?;
        Ok(Plugin {
            plugin_inner: Arc::new(plugin_inner),
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, PluginLoadError> {
        let plugin_inner = PluginInner::new(|store: &Store| -> Result<Module, PluginLoadError> {
            let file_ref = path.as_ref();
            let canonical = file_ref.canonicalize()?;
            let wasm_bytes = std::fs::read(file_ref)?;
            let mut module = Module::new(&store, &wasm_bytes)?;
            // Set the module name to the absolute path of the filename.
            // This is useful for debugging the stack traces.
            let filename = canonical.as_path().to_str().unwrap();
            module.set_name(filename);
            Ok(module)
        })?;
        Ok(Plugin {
            plugin_inner: Arc::new(plugin_inner),
        })
    }
}

struct PluginInner {
    store: Store,
    module: Module,
    instance: Instance,
}

fn build_imports(store: &mut impl AsStoreMut, module: &Module) -> Imports {
    // let env = FunctionEnv::new(&mut store, Env {});
    // fn get_current_time_nanoseconds(return_time: *mut u64) -> Status {
    //     // auto *context = contextOrEffectiveContext();
    //     // uint64_t result = context->getCurrentTimeNanoseconds();
    //     // if (!context->wasm()->setDatatype(result_uint64_ptr, result)) {
    //     //   return WasmResult::InvalidMemoryAccess;
    //     // }
    //     // return WasmResult::Ok;

    //     let _ = SystemTime::now()
    //         .duration_since(UNIX_EPOCH)
    //         .unwrap()
    //         .as_nanos();
    //     return Status::Unimplemented;
    // }
    imports! {
        "env" => {
            // "bulwark_get_current_time_nanoseconds" => Function::new_typed(&mut store, get_current_time_nanoseconds),
        }
    }
}

impl PluginInner {
    fn new<F>(mut build_module: F) -> Result<Self, PluginLoadError>
    where
        F: FnMut(&Store) -> Result<Module, PluginLoadError>,
    {
        let mut store = Store::default();
        let module = build_module(&store)?;
        let imports = build_imports(&mut store, &module);
        let instance = match Instance::new(&mut store, &module, &imports) {
            Ok(instance) => instance,
            Err(e) => return Err(PluginLoadError::Instantiation(e)),
        };
        Ok(Self {
            store,
            module,
            instance,
        })
    }

    // fn malloc(&self, size: u32) -> Result<*mut u8, AllocationError> {
    //     let malloc_fn: TypedFunction<(u32), i32> = self
    //         .instance
    //         .exports
    //         .get_function("bulwark_on_memory_allocate")?
    //         .typed(&self.store)?;
    //     let allocation = malloc_fn.call(&mut self.store, size)?;

    //     Ok(m)
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmer::TypedFunction;

    // TODO:
    // include_bytes!
    // cargo build --package bulwark-wasm-sdk --lib --verbose --target wasm32-unknown-unknown --example evil_param

    #[test]
    fn test_wasm_execution() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = include_bytes!("../test/dont_know.wasm");
        let mut plugin = Plugin::from_bytes(wasm_bytes)?;
        // let request_decision: TypedFunction<(i32, i32, i32), (f64, f64, f64)> = plugin
        //     .instance
        //     .exports
        //     .get_function("on_http_request_decision")?
        //     .typed(&plugin.store)?;
        // let result = request_decision.call(&mut plugin.store, 2, 0, 1)?;
        // assert_eq!(result.0, 0.0);
        // for import in plugin.module.imports() {
        //     println!("module: {} func: {}", import.module(), import.name());
        // }

        Ok(())
    }

    // #[test]
    // fn test_wasm_from_file_execution() -> Result<(), Box<dyn std::error::Error>> {
    //     let mut bundle = Bundle::new();
    //     let module = Module::Module::from_file(&bundle.store, "./wasm_api_test.wasm")?;
    //     let instance = Instance::new(&mut bundle.store, &module, &imports! {})?;
    //     let process: TypedFunction<i32, i32> = instance
    //         .exports
    //         .get_function("process")?
    //         .typed(&mut bundle.store)?;
    //     let result = process.call(&mut bundle.store, 1)?;
    //     assert_eq!(result, 2);

    //     Ok(())
    // }
}
