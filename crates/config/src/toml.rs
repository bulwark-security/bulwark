//! The `toml` module provides TOML deserialization and parsing for Bulwark's configuration files.

// Due to the need for multiple serialization mappings, TOML deserialization is not done
// directly in the [`bulwark_config`](crate) module's structs.

use {
    crate::ConfigFileError,
    serde::{Deserialize, Serialize},
    std::{fs, path::Path},
};

/// The TOML serialization for a Config structure.
#[derive(Serialize, Deserialize, Default)]
struct Config {
    #[serde(default)]
    service: Service,
    #[serde(default)]
    thresholds: Thresholds,
    #[serde(default, rename(serialize = "include", deserialize = "include"))]
    includes: Vec<Include>,
    #[serde(default, rename(serialize = "plugin", deserialize = "plugin"))]
    plugins: Vec<Plugin>,
    #[serde(default, rename(serialize = "preset", deserialize = "preset"))]
    presets: Vec<Preset>,
    #[serde(default, rename(serialize = "resource", deserialize = "resource"))]
    resources: Vec<Resource>,
}

/// The TOML serialization for a Service config structure.
#[derive(Serialize, Deserialize)]
struct Service {
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_admin_port")]
    admin_port: u16,
    #[serde(default = "default_admin")]
    admin: bool,
    #[serde(default = "default_remote_state")]
    remote_state: Option<String>,
    #[serde(default = "default_proxy_hops")]
    proxy_hops: u8,
}

/// The default port for the primary service.
///
/// See [`DEFAULT_PORT`].
fn default_port() -> u16 {
    crate::DEFAULT_PORT
}

/// The default port for the internal admin service.
///
/// See [`DEFAULT_ADMIN_PORT`].
fn default_admin_port() -> u16 {
    crate::DEFAULT_ADMIN_PORT
}

/// The default for whether the admin service should be enabled or not.
fn default_admin() -> bool {
    true
}

/// The default for the network address to access remote state.
fn default_remote_state() -> Option<String> {
    None
}

/// The default number of internal proxy hops expected in front of Bulwark.
fn default_proxy_hops() -> u8 {
    0
}

impl Default for Service {
    fn default() -> Self {
        Self {
            port: default_port(),
            admin_port: default_admin_port(),
            admin: default_admin(),
            remote_state: default_remote_state(),
            proxy_hops: default_proxy_hops(),
        }
    }
}

impl From<Service> for crate::Service {
    fn from(service: Service) -> Self {
        Self {
            port: service.port,
            admin_port: service.admin_port,
            admin: service.admin,
            remote_state: service.remote_state.clone(),
            proxy_hops: service.proxy_hops,
        }
    }
}

/// The TOML serialization for a Thresholds structure.
#[derive(Serialize, Deserialize)]
struct Thresholds {
    #[serde(default = "default_restrict_threshold")]
    restrict: f64,
    #[serde(default = "default_suspicious_threshold")]
    suspicious: f64,
    #[serde(default = "default_trust_threshold")]
    trust: f64,
}

/// The default threshold for restricting a request.
fn default_restrict_threshold() -> f64 {
    crate::DEFAULT_RESTRICT_THRESHOLD
}

/// The default threshold for treating a request as suspicious.
fn default_suspicious_threshold() -> f64 {
    crate::DEFAULT_SUSPICIOUS_THRESHOLD
}

/// The default threshold for trusting a request.
fn default_trust_threshold() -> f64 {
    crate::DEFAULT_TRUST_THRESHOLD
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            restrict: default_restrict_threshold(),
            suspicious: default_suspicious_threshold(),
            trust: default_trust_threshold(),
        }
    }
}

impl From<Thresholds> for crate::Thresholds {
    fn from(thresholds: Thresholds) -> Self {
        Self {
            restrict: thresholds.restrict,
            suspicious: thresholds.suspicious,
            trust: thresholds.trust,
        }
    }
}

/// The TOML serialization for an Include structure.
#[derive(Serialize, Deserialize)]
struct Include {
    path: String,
}

/// The TOML serialization for a Plugin structure.
#[derive(Serialize, Deserialize, Clone)]
struct Plugin {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    reference: String,
    path: String,
    #[serde(default = "default_plugin_weight")]
    weight: f64,
    #[serde(default)]
    config: toml::map::Map<String, toml::Value>,
    #[serde(default)]
    permissions: TomlPermissions,
}

/// The default weight for a plugin.
///
/// See DEFAULT_PLUGIN_WEIGHT.
fn default_plugin_weight() -> f64 {
    crate::DEFAULT_PLUGIN_WEIGHT
}

impl From<&Plugin> for crate::config::Plugin {
    fn from(plugin: &Plugin) -> Self {
        Self {
            reference: plugin.reference.clone(),
            path: plugin.path.clone(),
            weight: plugin.weight,
            config: toml_map_to_json(plugin.config.clone()),
            permissions: plugin.permissions.clone().into(),
        }
    }
}

fn toml_map_to_json(
    map: toml::map::Map<String, toml::Value>,
) -> serde_json::map::Map<String, serde_json::Value> {
    let mut json_map = serde_json::map::Map::with_capacity(map.len());
    for (key, value) in map {
        json_map.insert(key, toml_value_to_json(value));
    }
    json_map
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

/// The TOML serialization for a Permissions structure.
#[derive(Serialize, Deserialize, Clone, Default)]
struct TomlPermissions {
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    http: Vec<String>,
    #[serde(default)]
    state: Vec<String>,
}

impl From<TomlPermissions> for crate::config::Permissions {
    fn from(permissions: TomlPermissions) -> Self {
        Self {
            env: permissions.env,
            http: permissions.http,
            state: permissions.state,
        }
    }
}

/// The TOML serialization for a Preset structure.
#[derive(Serialize, Deserialize, Clone)]
struct Preset {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    reference: String,
    plugins: Vec<String>,
}

/// The TOML serialization for a Resource structure.
#[derive(Serialize, Deserialize, Clone)]
struct Resource {
    route: String,
    plugins: Vec<String>,
    // TODO: default timeout
    timeout: Option<u64>,
}

/// Loads a TOML config file into a [`Config`](crate::Config) structure.
pub fn load_config<'a, P>(path: &'a P) -> Result<crate::Config, ConfigFileError>
where
    P: 'a + ?Sized + AsRef<Path>,
{
    fn load_config_recursive<'a, P>(path: &'a P) -> Result<Config, ConfigFileError>
    where
        P: 'a + ?Sized + AsRef<Path>,
    {
        let toml_data = fs::read_to_string(path)?;
        let mut root: Config = toml::from_str(&toml_data)?;
        // TODO: avoid unwrap
        let base = path.as_ref().parent().unwrap();

        // TODO: error on circular includes
        for include in &root.includes {
            let include_path = base.join(&include.path);
            let include_root = load_config_recursive(&include_path)?;

            // TODO: clean this up
            let root_plugins = root.plugins;
            let mut combined_plugins: Vec<Plugin> =
                Vec::with_capacity(root_plugins.len() + include_root.plugins.len());
            combined_plugins.extend_from_slice(root_plugins.as_slice());
            combined_plugins.extend_from_slice(include_root.plugins.as_slice());
            root.plugins = combined_plugins;

            let root_presets = root.presets;
            let mut combined_presets: Vec<Preset> =
                Vec::with_capacity(root_presets.len() + include_root.presets.len());
            combined_presets.extend_from_slice(root_presets.as_slice());
            combined_presets.extend_from_slice(include_root.presets.as_slice());
            root.presets = combined_presets;

            let root_resources = root.resources;
            let mut combined_resources: Vec<Resource> =
                Vec::with_capacity(root_resources.len() + include_root.resources.len());
            combined_resources.extend_from_slice(root_resources.as_slice());
            combined_resources.extend_from_slice(include_root.resources.as_slice());
            root.resources = combined_resources;
        }

        // Strip includes once processed
        root.includes = vec![];

        Ok(root)
    }

    // Load the raw serialization format and resolve includes
    let root = load_config_recursive(path)?;
    let resolve_reference = |ref_name: &String| {
        let mut reference = crate::config::Reference::Missing(ref_name.clone());
        for preset in &root.presets {
            if preset.reference == *ref_name {
                reference = crate::config::Reference::Preset(ref_name.clone());
            }
        }
        for plugin in &root.plugins {
            if plugin.reference == *ref_name {
                reference = crate::config::Reference::Plugin(ref_name.clone());
            }
        }
        reference
    };
    // Transfer to the public config type, checking reference enums
    Ok(crate::Config {
        service: root.service.into(),
        thresholds: root.thresholds.into(),
        plugins: root.plugins.iter().map(|plugin| plugin.into()).collect(),
        presets: root
            .presets
            .iter()
            .map(|preset| crate::config::Preset {
                reference: preset.reference.clone(),
                plugins: preset.plugins.iter().map(resolve_reference).collect(),
            })
            .collect(),
        resources: root
            .resources
            .iter()
            .map(|resource| crate::config::Resource {
                route: resource.route.clone(),
                plugins: resource.plugins.iter().map(resolve_reference).collect(),
                timeout: resource.timeout,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize() -> Result<(), Box<dyn std::error::Error>> {
        let root: Config = toml::from_str(
            r#"
        [service]
        port = 10002

        [thresholds]
        restrict = 0.75
    
        [[include]]
        path = "default.toml"

        [[plugin]]
        ref = "evil_bit"
        path = "bulwark-evil-bit.wasm"

        [[preset]]
        ref = "custom"
        plugins = ["evil-bit"]

        [[resource]]
        route = "/"
        plugins = ["custom"]
        timeout = 25
    "#,
        )?;

        assert_eq!(root.service.port, 10002); // non-default
        assert_eq!(root.service.admin_port, crate::DEFAULT_ADMIN_PORT);

        assert_eq!(root.thresholds.restrict, 0.75); // non-default
        assert_eq!(
            root.thresholds.suspicious,
            crate::DEFAULT_SUSPICIOUS_THRESHOLD
        );
        assert_eq!(root.thresholds.trust, crate::DEFAULT_TRUST_THRESHOLD);

        assert_eq!(root.includes.len(), 1);
        assert_eq!(root.includes.get(0).unwrap().path, "default.toml");

        assert_eq!(root.plugins.len(), 1);
        assert_eq!(root.plugins.get(0).unwrap().reference, "evil_bit");
        assert_eq!(root.plugins.get(0).unwrap().path, "bulwark-evil-bit.wasm");
        assert_eq!(
            root.plugins.get(0).unwrap().config,
            toml::map::Map::default()
        );

        assert_eq!(root.presets.len(), 1);
        assert_eq!(root.presets.get(0).unwrap().reference, "custom");
        assert_eq!(root.presets.get(0).unwrap().plugins, vec!["evil-bit"]);

        assert_eq!(root.resources.len(), 1);
        assert_eq!(root.resources.get(0).unwrap().route, "/");
        assert_eq!(root.resources.get(0).unwrap().plugins, vec!["custom"]);
        assert_eq!(root.resources.get(0).unwrap().timeout, Some(25));

        Ok(())
    }

    #[test]
    fn test_load_config() -> Result<(), Box<dyn std::error::Error>> {
        let root: crate::config::Config = load_config("tests/main.toml")?;

        assert_eq!(root.service.port, 10002); // non-default
        assert_eq!(root.service.admin_port, crate::DEFAULT_ADMIN_PORT);

        assert_eq!(root.thresholds.restrict, 0.75); // non-default
        assert_eq!(
            root.thresholds.suspicious,
            crate::DEFAULT_SUSPICIOUS_THRESHOLD
        );
        assert_eq!(root.thresholds.trust, crate::DEFAULT_TRUST_THRESHOLD);

        assert_eq!(root.plugins.len(), 2);
        assert_eq!(root.plugins.get(0).unwrap().reference, "evil_bit");
        assert_eq!(root.plugins.get(0).unwrap().path, "bulwark-evil-bit.wasm");
        assert_eq!(
            root.plugins.get(0).unwrap().config,
            serde_json::map::Map::default()
        );

        assert_eq!(root.presets.len(), 2);
        assert_eq!(root.presets.get(0).unwrap().reference, "default");
        assert_eq!(root.presets.get(1).unwrap().reference, "starter_preset");
        assert_eq!(
            root.presets.get(0).unwrap().plugins,
            vec![
                crate::config::Reference::Plugin("evil_bit".to_string()),
                crate::config::Reference::Preset("starter_preset".to_string())
            ]
        );
        assert_eq!(
            root.presets.get(1).unwrap().plugins,
            vec![crate::config::Reference::Plugin("blank_slate".to_string())]
        );

        assert_eq!(root.resources.len(), 2);
        assert_eq!(root.resources.get(0).unwrap().route, "/");
        assert_eq!(root.resources.get(1).unwrap().route, "/*params");
        assert_eq!(
            root.resources.get(0).unwrap().plugins,
            vec![crate::config::Reference::Preset("default".to_string())]
        );
        assert_eq!(root.resources.get(0).unwrap().timeout, Some(25));

        Ok(())
    }
}
