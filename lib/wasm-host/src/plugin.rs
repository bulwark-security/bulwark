use crate::PluginLoadError;
use bulwark_wasm_sdk::types::Decision;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use wasmer::{imports, Function, FunctionEnv, Imports, Instance, Module, Store};

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
        let mut plugin_inner = PluginInner::new();
        let module = Module::new(&plugin_inner.store, bytes)?;
        plugin_inner.module(module)?;
        Ok(Plugin {
            plugin_inner: Arc::new(plugin_inner),
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, PluginLoadError> {
        let mut plugin_inner = PluginInner::new();
        let file_ref = path.as_ref();
        let canonical = file_ref.canonicalize()?;
        let wasm_bytes = std::fs::read(file_ref)?;
        let mut module = Module::new(&plugin_inner.store, &wasm_bytes)?;
        // Set the module name to the absolute path of the filename.
        // This is useful for debugging the stack traces.
        let filename = canonical.as_path().to_str().unwrap();
        module.set_name(filename);
        plugin_inner.module(module)?;
        Ok(Plugin {
            plugin_inner: Arc::new(plugin_inner),
        })
    }
}

struct PluginInner {
    store: Store,
    module: Option<Module>,
    instance: Option<Instance>,
}

impl PluginInner {
    fn new() -> Self {
        Self {
            store: Store::default(),
            module: None,
            instance: None,
        }
    }

    fn module(&mut self, module: Module) -> Result<(), PluginLoadError> {
        self.module = Some(module);
        let imports = self.imports();
        self.instance(imports)?;
        Ok(())
    }

    fn imports(&mut self) -> Imports {
        let env = FunctionEnv::new(&mut self.store, Env {});
        fn get_current_time_nanoseconds() -> i32 {
            let _ = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            // TODO: return (uint64_t*) as i32 pointer
            return 0;
        }
        imports! {
            "env" => {
                "bulwark_get_current_time_nanoseconds" => Function::new_typed(&mut self.store, get_current_time_nanoseconds),
            }
        }
    }

    fn instance(&mut self, imports: Imports) -> Result<(), PluginLoadError> {
        let module = match &self.module {
            Some(module) => module,
            None => return Err(PluginLoadError::Build()),
        };
        let instance = match Instance::new(&mut self.store, &module, &imports) {
            Ok(instance) => instance,
            Err(e) => return Err(PluginLoadError::Instantiation(e)),
        };
        self.instance = Some(instance);
        Ok(())
    }

    // fn get_exported_function(&self, name: &str) -> Result<Function, ExportError> {
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
