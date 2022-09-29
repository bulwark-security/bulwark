use crate::Component;
use wasmer::Store;
// owns a collection of bulwark components and wraps a wasm store
#[derive(Default)]
pub struct Bundle {
    components: Vec<Component>,
    store: Store,
}

impl Bundle {
    pub fn new() -> Self {
        Bundle {
            components: Vec::new(),
            store: Store::default(),
        }
    }

    pub fn push(&mut self, value: Component) {
        self.components.push(value);
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmer::{imports, wat2wasm, Instance, Module, TypedFunction};

    #[test]
    fn add_default_components() {
        let mut bundle = Bundle::new();
        bundle.push(Component::new());
        bundle.push(Component::new());
        assert_eq!(2, bundle.len());
    }

    #[test]
    fn simple_wasm_execution() -> Result<(), Box<dyn std::error::Error>> {
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
}
