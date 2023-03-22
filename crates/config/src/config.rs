use serde::{Deserialize, Serialize};

const DEFAULT_PORT: u16 = 10000;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub service: Option<Service>,
    pub thresholds: Option<Thresholds>,
    #[serde(rename(serialize = "plugin", deserialize = "plugin"))]
    pub plugins: Option<Vec<Plugin>>,
    #[serde(rename(serialize = "preset", deserialize = "preset"))]
    pub presets: Option<Vec<Preset>>,
    #[serde(rename(serialize = "resource", deserialize = "resource"))]
    pub resources: Option<Vec<Resource>>,
}

impl Config {
    pub fn port(&self) -> u16 {
        self.service
            .as_ref()
            .map(|service| service.port.unwrap_or(DEFAULT_PORT))
            .unwrap_or(DEFAULT_PORT)
    }

    pub fn get_plugin(&self, reference: &str) -> Option<Plugin> {
        if let Some(plugins) = &self.plugins {
            for plugin in plugins {
                if plugin.reference == reference {
                    // TODO: might prefer not to clone and use lifetimes instead?
                    return Some(plugin.clone());
                }
            }
        }
        None
    }

    pub fn get_preset(&self, reference: &str) -> Option<Preset> {
        if let Some(presets) = &self.presets {
            for preset in presets {
                if preset.reference == reference {
                    // TODO: might prefer not to clone and use lifetimes instead?
                    return Some(preset.clone());
                }
            }
        }
        None
    }
}

#[derive(Serialize, Deserialize)]
pub struct Service {
    pub port: Option<u16>,
    pub remote_state: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
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

#[derive(Serialize, Deserialize, Clone)]
pub struct Plugin {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    pub reference: String,

    // TODO: plugin path should be absolute; once it's in this structure the config base path is no longer known
    pub path: String,

    // TODO: this will serialize as JSON, so there might be a better internal representation
    pub config: toml::map::Map<String, toml::Value>,
}

impl Plugin {
    pub fn config_as_json(&self) -> Vec<u8> {
        let mut json_map = serde_json::Map::with_capacity(self.config.len());
        for (key, value) in self.config.clone() {
            json_map.insert(key, toml_value_to_json(value));
        }
        let obj = serde_json::Value::Object(json_map);
        // TODO: probably should return a result instead of panicking
        serde_json::to_vec(&obj).unwrap()
    }
}

fn toml_value_to_json(value: toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(v) => serde_json::Value::String(v),
        toml::Value::Integer(v) => serde_json::Value::Number(serde_json::Number::from(v)),
        toml::Value::Float(v) => {
            // TODO: probably should return a result instead of panicking although NaN in a config would be weird
            serde_json::Value::Number(serde_json::Number::from_f64(v).unwrap())
        }
        toml::Value::Boolean(v) => serde_json::Value::Bool(v),
        toml::Value::Datetime(v) => {
            // TODO: probably should return a result instead of panicking
            // TODO: should we always convert to UTC, make this configurable, or just leave time-zone as-is and hope system time is UTC?
            let ts = chrono::DateTime::parse_from_rfc3339(v.to_string().as_str())
                .unwrap()
                .with_timezone(&chrono::Utc);
            serde_json::Value::String(ts.to_rfc3339())
        }
        toml::Value::Array(v) => {
            let svec = v
                .iter()
                .map(|inner_value| toml_value_to_json(inner_value.clone()))
                .collect();
            serde_json::Value::Array(svec)
        }
        toml::Value::Table(v) => {
            let smap = v
                .iter()
                .map(|(inner_key, inner_value)| {
                    (inner_key.clone(), toml_value_to_json(inner_value.clone()))
                })
                .collect();
            serde_json::Value::Object(smap)
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Preset {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    pub reference: String,
    pub plugins: Vec<Reference>,
}

impl Preset {
    pub fn resolve_plugins(&self, config: &Config) -> Vec<Plugin> {
        let mut plugins: Vec<Plugin> = Vec::with_capacity(self.plugins.len());
        for reference in &self.plugins {
            match reference {
                Reference::Plugin(ref_name) => {
                    if let Some(plugin) = config.get_plugin(ref_name.as_str()) {
                        plugins.push(plugin);
                    }
                }
                Reference::Preset(ref_name) => {
                    if let Some(preset) = config.get_preset(ref_name.as_str()) {
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

#[derive(Serialize, Deserialize, Clone)]
pub struct Resource {
    pub route: String,
    pub plugins: Vec<Reference>,
    pub timeout: Option<u64>,
}

impl Resource {
    pub fn resolve_plugins(&self, config: &Config) -> Vec<Plugin> {
        let mut plugins: Vec<Plugin> = Vec::with_capacity(self.plugins.len());
        for reference in &self.plugins {
            match reference {
                Reference::Plugin(ref_name) => {
                    if let Some(plugin) = config.get_plugin(ref_name.as_str()) {
                        plugins.push(plugin);
                    }
                }
                Reference::Preset(ref_name) => {
                    if let Some(preset) = config.get_preset(ref_name.as_str()) {
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub enum Reference {
    Plugin(String),
    Preset(String),
    Missing(String),
}
