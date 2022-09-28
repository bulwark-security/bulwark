use crate::Component;

// owns a collection of bulwark components and wraps a wasm store
#[derive(Default)]
pub struct Bundle {
    components: Vec<Component>,
}

impl Bundle {
    pub fn new() -> Self {
        Bundle {
            components: Vec::new(),
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

    #[test]
    fn add_default_components() {
        let mut bundle = Bundle::new();
        bundle.push(Component::new());
        bundle.push(Component::new());
        assert_eq!(2, bundle.len());
    }
}
