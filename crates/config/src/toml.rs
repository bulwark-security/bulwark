//! The `toml` module provides TOML deserialization and parsing for Bulwark's configuration files.

// Due to the need for multiple serialization mappings, TOML deserialization is not done
// directly in the [`bulwark_config`](crate) module's structs.

use crate::ConfigFileError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, ffi::OsString, fs, path::Path};
use validator::Validate;

lazy_static! {
    static ref RE_VALID_REFERENCE: Regex = Regex::new(r"^[_a-z]+$").unwrap();
}

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
    admin_enabled: bool,
    #[serde(default = "default_remote_state_uri")]
    remote_state_uri: Option<String>,
    #[serde(default = "default_remote_state_pool_size")]
    remote_state_pool_size: u32,
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
fn default_remote_state_uri() -> Option<String> {
    None
}

/// The default for the remote state connection pool size.
fn default_remote_state_pool_size() -> u32 {
    crate::DEFAULT_REMOTE_STATE_POOL_SIZE
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
            admin_enabled: default_admin(),
            remote_state_uri: default_remote_state_uri(),
            remote_state_pool_size: default_remote_state_pool_size(),
            proxy_hops: default_proxy_hops(),
        }
    }
}

impl From<Service> for crate::Service {
    fn from(service: Service) -> Self {
        Self {
            port: service.port,
            admin_port: service.admin_port,
            admin_enabled: service.admin_enabled,
            remote_state_uri: service.remote_state_uri.clone(),
            remote_state_pool_size: service.remote_state_pool_size,
            proxy_hops: service.proxy_hops,
        }
    }
}

/// The TOML serialization for a Thresholds structure.
#[derive(Serialize, Deserialize)]
struct Thresholds {
    #[serde(default = "default_observe_only")]
    observe_only: bool,
    #[serde(default = "default_restrict_threshold")]
    restrict: f64,
    #[serde(default = "default_suspicious_threshold")]
    suspicious: f64,
    #[serde(default = "default_trust_threshold")]
    trust: f64,
}

/// The default for whether the primary service should take no action in response to restrict decisions.
fn default_observe_only() -> bool {
    crate::DEFAULT_OBSERVE_ONLY
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
            observe_only: default_observe_only(),
            restrict: default_restrict_threshold(),
            suspicious: default_suspicious_threshold(),
            trust: default_trust_threshold(),
        }
    }
}

impl From<Thresholds> for crate::Thresholds {
    fn from(thresholds: Thresholds) -> Self {
        Self {
            observe_only: thresholds.observe_only,
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
#[derive(Validate, Serialize, Deserialize, Clone)]
struct Plugin {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    #[validate(length(min = 1, max = 96), regex(path = "RE_VALID_REFERENCE"))]
    reference: String,
    #[validate(length(min = 1))]
    path: String,
    #[serde(default = "default_plugin_weight")]
    #[validate(range(min = 0.0))]
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
            let ts = chrono::DateTime::parse_from_rfc3339(v.to_string().as_str()).unwrap();
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
#[derive(Validate, Serialize, Deserialize, Clone)]
struct Preset {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    #[validate(length(min = 1, max = 96), regex(path = "RE_VALID_REFERENCE"))]
    reference: String,
    #[validate(length(min = 1))]
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
    let mut loaded_files = HashSet::new();

    fn load_config_recursive<'a, P>(
        path: &'a P,
        loaded_files: &mut HashSet<OsString>,
    ) -> Result<Config, ConfigFileError>
    where
        P: 'a + ?Sized + AsRef<Path>,
    {
        let path_string = path.as_ref().as_os_str().to_os_string();
        if loaded_files.contains(&path_string) {
            return Err(ConfigFileError::CircularInclude(
                path_string.to_string_lossy().to_string(),
            ));
        } else {
            loaded_files.insert(path_string);
        }
        let toml_data = fs::read_to_string(path)?;
        let mut root: Config = toml::from_str(&toml_data)?;
        // TODO: avoid unwrap
        let base = path.as_ref().parent().unwrap();

        // TODO: error on circular includes
        for include in &root.includes {
            let include_path = base.join(&include.path);
            let include_root = load_config_recursive(&include_path, loaded_files)?;

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
    let root = load_config_recursive(path, &mut loaded_files)?;

    // Validate presets and plugins and their references
    let mut references: HashSet<&String> = HashSet::new();
    for preset in &root.presets {
        preset.validate()?;
        if references.contains(&preset.reference) {
            return Err(ConfigFileError::Duplicate(preset.reference.to_string()));
        } else {
            references.insert(&preset.reference);
        }
    }
    for plugin in &root.plugins {
        plugin.validate()?;
        if references.contains(&plugin.reference) {
            return Err(ConfigFileError::Duplicate(plugin.reference.to_string()));
        } else {
            references.insert(&plugin.reference);
        }
    }
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
    let config = crate::Config {
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
    };
    for resource in &config.resources {
        // Resolve plugins to surface resolution errors immediately
        resource.resolve_plugins(&config)?;
    }
    Ok(config)
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

    #[test]
    fn test_load_config_overlapping_preset() -> Result<(), Box<dyn std::error::Error>> {
        let root: crate::config::Config = load_config("tests/overlapping_preset.toml")?;

        let resource = root.resources.get(0).unwrap();
        let plugins = resource.resolve_plugins(&root)?;
        assert_eq!(plugins.len(), 2);
        assert_eq!(
            plugins.get(0).unwrap().reference,
            root.plugin("blank_slate").unwrap().reference
        );
        assert_eq!(
            plugins.get(1).unwrap().reference,
            root.plugin("evil_bit").unwrap().reference
        );

        Ok(())
    }

    #[test]
    fn test_load_config_circular_include() -> Result<(), Box<dyn std::error::Error>> {
        let result = load_config("tests/circular_include.toml");
        // This needs to be an error, not a panic.
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .starts_with("invalid circular include"));
        Ok(())
    }

    #[test]
    fn test_load_config_circular_preset() -> Result<(), Box<dyn std::error::Error>> {
        let result = load_config("tests/circular_preset.toml");
        // This needs to be an error, not a panic.
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .starts_with("invalid circular preset reference"));
        Ok(())
    }

    #[test]
    fn test_load_config_duplicate_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let result = load_config("tests/duplicate_plugin.toml");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "duplicate named plugin or preset: 'blank_slate'"
        );
        Ok(())
    }

    #[test]
    fn test_load_config_duplicate_preset() -> Result<(), Box<dyn std::error::Error>> {
        let result = load_config("tests/duplicate_preset.toml");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "duplicate named plugin or preset: 'default'"
        );
        Ok(())
    }

    #[test]
    fn test_load_config_duplicate_mixed() -> Result<(), Box<dyn std::error::Error>> {
        let result = load_config("tests/duplicate_mixed.toml");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "duplicate named plugin or preset: 'blank_slate'"
        );
        Ok(())
    }

    #[test]
    fn test_load_config_resolution_missing() -> Result<(), Box<dyn std::error::Error>> {
        let result = load_config("tests/missing.toml");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "missing named plugin or preset: 'blank_slate'"
        );
        Ok(())
    }

    #[test]
    fn test_load_config_missing_include() -> Result<(), Box<dyn std::error::Error>> {
        let result = load_config("tests/missing_include.toml");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .starts_with("No such file or directory"));
        Ok(())
    }
}
