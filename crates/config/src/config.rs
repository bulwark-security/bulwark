/// The root of a Bulwark configuration.
///
/// Wraps all child configuration structures and provides the internal representation of Bulwark's configuration.
pub struct Config {
    pub service: Service,
    pub thresholds: Thresholds,
    pub plugins: Vec<Plugin>,
    pub presets: Vec<Preset>,
    pub resources: Vec<Resource>,
}

impl Config {
    pub fn plugin<'a>(&self, reference: &str) -> Option<&Plugin>
    where
        Plugin: 'a,
    {
        self.plugins
            .iter()
            .find(|&plugin| plugin.reference == reference)
    }

    pub fn preset<'a>(&self, reference: &str) -> Option<&Preset>
    where
        Preset: 'a,
    {
        self.presets
            .iter()
            .find(|&preset| preset.reference == reference)
    }
}

pub const DEFAULT_PORT: u16 = 8089;
pub const DEFAULT_ADMIN_PORT: u16 = 8090;

pub struct Service {
    pub port: u16,
    pub admin_port: u16,
    pub admin: bool,
    pub remote_state: Option<String>,
    pub proxy_hops: u8,
}

#[derive(Clone, Copy)]
pub struct Thresholds {
    pub restrict: f64,
    pub suspicious: f64,
    pub trust: f64,
}

pub const DEFAULT_RESTRICT_THRESHOLD: f64 = 0.8;
pub const DEFAULT_SUSPICIOUS_THRESHOLD: f64 = 0.6;
pub const DEFAULT_TRUST_THRESHOLD: f64 = 0.2;

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            restrict: DEFAULT_RESTRICT_THRESHOLD,
            suspicious: DEFAULT_SUSPICIOUS_THRESHOLD,
            trust: DEFAULT_TRUST_THRESHOLD,
        }
    }
}

#[derive(Clone, Default)]
pub struct Plugin {
    pub reference: String,
    // TODO: plugin path should be absolute; once it's in this structure the config base path is no longer known
    // TODO: should this be a URI? That would allow e.g. data: URI values to embed WASM into config over the wire
    pub path: String,
    pub weight: f64,
    pub config: serde_json::map::Map<String, serde_json::Value>,
    pub permissions: Permissions,
}

pub const DEFAULT_PLUGIN_WEIGHT: f64 = 1.0;

impl Plugin {
    pub fn config_to_json(&self) -> Vec<u8> {
        let obj = serde_json::Value::Object(self.config.clone());
        // TODO: probably should return a result instead of panicking
        serde_json::to_vec(&obj).unwrap()
    }
}

#[derive(Clone, Default)]
pub struct Permissions {
    pub env: Vec<String>,
    pub http: Vec<String>,
    pub state: Vec<String>,
}

#[derive(Clone)]
pub struct Preset {
    pub reference: String,
    pub plugins: Vec<Reference>,
}

impl Preset {
    pub fn resolve_plugins<'a>(&'a self, config: &'a Config) -> Vec<&Plugin> {
        let mut plugins: Vec<&Plugin> = Vec::with_capacity(self.plugins.len());
        for reference in &self.plugins {
            match reference {
                Reference::Plugin(ref_name) => {
                    if let Some(plugin) = config.plugin(ref_name.as_str()) {
                        plugins.push(plugin);
                    }
                }
                Reference::Preset(ref_name) => {
                    if let Some(preset) = config.preset(ref_name.as_str()) {
                        let mut inner_plugins = preset.resolve_plugins(config);
                        plugins.append(&mut inner_plugins);
                    }
                }
                Reference::Missing(_) => todo!(),
            }
        }
        plugins
    }
}

#[derive(Clone)]
pub struct Resource {
    pub route: String,
    pub plugins: Vec<Reference>,
    pub timeout: Option<u64>,
}

impl Resource {
    pub fn resolve_plugins<'a>(&'a self, config: &'a Config) -> Vec<&Plugin> {
        let mut plugins: Vec<&Plugin> = Vec::with_capacity(self.plugins.len());
        for reference in &self.plugins {
            match reference {
                Reference::Plugin(ref_name) => {
                    if let Some(plugin) = config.plugin(ref_name.as_str()) {
                        plugins.push(plugin);
                    }
                }
                Reference::Preset(ref_name) => {
                    if let Some(preset) = config.preset(ref_name.as_str()) {
                        let mut inner_plugins = preset.resolve_plugins(config);
                        plugins.append(&mut inner_plugins);
                    }
                }
                Reference::Missing(_) => todo!(),
            }
        }
        plugins
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Reference {
    Plugin(String),
    Preset(String),
    Missing(String),
}
