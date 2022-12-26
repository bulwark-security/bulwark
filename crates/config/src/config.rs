use serde::{Deserialize, Serialize};

const DEFAULT_PORT: u16 = 10000;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub service: Option<Service>,
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

#[derive(Serialize, Deserialize, Clone)]
pub struct Preset {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    pub reference: String,
    pub plugins: Vec<Reference>,
    pub timeout: Option<u64>,
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
