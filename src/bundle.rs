use crate::ComponentLoadError;
use wasmer::{imports, wat2wasm, Instance, Module, Store};
// owns a collection of bulwark components and wraps a wasm store
#[derive(Default)]
pub struct Bundle {
    modules: Vec<Module>,
    instances: Vec<Instance>,
    store: Store,
}

impl Bundle {
    pub fn new() -> Self {
        Bundle {
            modules: Vec::new(),
            instances: Vec::new(),
            store: Store::default(),
        }
    }

    pub fn add_from_wat(&mut self, bytes: &[u8]) -> Result<(), ComponentLoadError> {
        // TODO: hide behind wat feature
        let module = match Module::new(&self.store, bytes) {
            Ok(module) => module,
            Err(e) => return Err(ComponentLoadError::Compile(e)),
        };
        let instance = match Instance::new(&mut self.store, &module, &imports! {}) {
            Ok(instance) => instance,
            Err(e) => return Err(ComponentLoadError::Instantiation(e)),
        };
        // TODO: verify the module contains required functions
        self.modules.push(module);
        self.instances.push(instance);
        Ok(())
    }

    pub fn len(&self) -> usize {
        // this will always be the same length as instances
        self.modules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmer::TypedFunction;

    // #[test]
    // fn add_default_components() {
    //     let mut bundle = Bundle::new();
    //     bundle.push(Component::new());
    //     bundle.push(Component::new());
    //     assert_eq!(2, bundle.len());
    // }

    #[test]
    fn test_simple_wasm_execution() -> Result<(), Box<dyn std::error::Error>> {
        let mut bundle = Bundle::new();
        let wasm_bytes = wat2wasm(
            br#"
        (module
        (type $add_one_t (func (param i32) (result i32)))
        (func $add_one_f (type $add_one_t) (param $value i32) (result i32)
            local.get $value
            i32.const 1
            i32.add)
        (export "add_one" (func $add_one_f)))
        "#,
        )?;
        let module = Module::new(&bundle.store, wasm_bytes)?;
        let instance = Instance::new(&mut bundle.store, &module, &imports! {})?;
        let add_one: TypedFunction<i32, i32> = instance
            .exports
            .get_function("add_one")?
            .typed(&mut bundle.store)?;
        let result = add_one.call(&mut bundle.store, 1)?;
        assert_eq!(result, 2);

        Ok(())
    }

    #[test]
    fn test_add_two_components() -> Result<(), Box<dyn std::error::Error>> {
        let mut bundle = Bundle::new();
        bundle.add_from_wat(
            br#"
        (module
          (type $add_one_t (func (param i32) (result i32)))
          (func $add_one_f (type $add_one_t) (param $value i32) (result i32)
            local.get $value
            i32.const 1
            i32.add)
          (export "add_one" (func $add_one_f)))
        "#,
        )?;
        bundle.add_from_wat(
            br#"
        (module
          (type $add_one_t (func (param i32) (result i32)))
          (func $add_one_f (type $add_one_t) (param $value i32) (result i32)
            local.get $value
            i32.const 1
            i32.add)
          (export "add_one" (func $add_one_f)))
        "#,
        )?;
        assert_eq!(bundle.len(), 2);
        assert!(bundle.is_empty() == false);

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
