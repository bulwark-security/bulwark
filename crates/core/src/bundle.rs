use bulwark_wasm_host::Plugin;

// owns a collection of bulwark plugins
#[derive(Default)]
pub struct Bundle {
    modules: Vec<Plugin>,
}

impl Bundle {
    pub fn new() -> Self {
        Bundle { modules: vec![] }
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
}
