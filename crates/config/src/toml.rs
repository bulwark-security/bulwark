//! The `toml` module provides TOML deserialization and parsing for Bulwark's configuration files.

// Due to the need for multiple serialization mappings, TOML deserialization is not done
// directly in the [`bulwark_config`](crate) module's structs.

use crate::ConfigFileError;
use bytes::Bytes;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;
use validator::Validate;

lazy_static! {
    static ref RE_VALID_REFERENCE: Regex = Regex::new(r"^[a-z_]([a-z0-9_])*$").unwrap();
}

/// The TOML serialization for a [Config](crate::Config) structure.
#[derive(Serialize, Deserialize, Default)]
struct Config {
    #[serde(default)]
    service: Service,
    #[serde(default)]
    runtime: Runtime,
    #[serde(default)]
    state: State,
    #[serde(default)]
    thresholds: Thresholds,
    #[serde(default)]
    metrics: Metrics,
    #[serde(default, rename(serialize = "include", deserialize = "include"))]
    includes: Vec<Include>,
    #[serde(default, rename(serialize = "secret", deserialize = "secret"))]
    secrets: Vec<Secret>,
    #[serde(default, rename(serialize = "plugin", deserialize = "plugin"))]
    plugins: Vec<Plugin>,
    #[serde(default, rename(serialize = "preset", deserialize = "preset"))]
    presets: Vec<Preset>,
    #[serde(default, rename(serialize = "resource", deserialize = "resource"))]
    resources: Vec<Resource>,
}

/// The TOML serialization for a [Service](crate::Service) config structure.
#[derive(Serialize, Deserialize)]
struct Service {
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_admin_port")]
    admin_port: u16,
    #[serde(default = "default_admin")]
    admin_enabled: bool,
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
            proxy_hops: service.proxy_hops,
        }
    }
}

/// The TOML serialization for a [Runtime](crate::Runtime) config structure.
#[derive(Serialize, Deserialize)]
struct Runtime {
    #[serde(default = "default_max_concurrent_requests")]
    max_concurrent_requests: usize,
    #[serde(default = "default_max_plugin_tasks")]
    max_plugin_tasks: usize,
}

/// The default maximum number of concurrent incoming requests that the runtime will process before blocking.
///
/// See [`DEFAULT_MAX_CONCURRENT_REQUESTS`].
fn default_max_concurrent_requests() -> usize {
    crate::DEFAULT_MAX_CONCURRENT_REQUESTS
}

/// The default maximum number of concurrent plugin tasks that the runtime will launch.
///
/// See [`DEFAULT_MAX_PLUGIN_TASKS`].
fn default_max_plugin_tasks() -> usize {
    crate::DEFAULT_MAX_PLUGIN_TASKS
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            max_concurrent_requests: default_max_concurrent_requests(),
            max_plugin_tasks: default_max_plugin_tasks(),
        }
    }
}

impl From<Runtime> for crate::Runtime {
    fn from(service: Runtime) -> Self {
        Self {
            max_concurrent_requests: service.max_concurrent_requests,
            max_plugin_tasks: service.max_plugin_tasks,
        }
    }
}

/// The TOML serialization for a [State](crate::State) config structure.
#[derive(Serialize, Deserialize)]
struct State {
    #[serde(default = "default_redis_uri")]
    redis_uri: Option<String>,
    #[serde(default = "default_redis_pool_size")]
    redis_pool_size: usize,
}

/// The default for the network address to access remote state.
fn default_redis_uri() -> Option<String> {
    None
}

/// The default for the remote state connection pool size.
fn default_redis_pool_size() -> usize {
    num_cpus::get_physical() * 4
}

impl Default for State {
    fn default() -> Self {
        Self {
            redis_uri: default_redis_uri(),
            redis_pool_size: default_redis_pool_size(),
        }
    }
}

impl From<State> for crate::State {
    fn from(state: State) -> Self {
        Self {
            redis_uri: state.redis_uri,
            redis_pool_size: state.redis_pool_size,
        }
    }
}

/// The TOML serialization for a [Thresholds](crate::Thresholds) structure.
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

/// The TOML serialization for a [Metrics](crate::Metrics) structure.
#[derive(Serialize, Deserialize)]
struct Metrics {
    #[serde(default)]
    statsd_host: Option<String>,
    #[serde(default = "default_statsd_port")]
    statsd_port: Option<u16>,
    #[serde(default = "default_statsd_queue_size")]
    statsd_queue_size: usize,
    #[serde(default = "default_statsd_buffer_size")]
    statsd_buffer_size: usize,
    #[serde(default)]
    statsd_prefix: String,
}

fn default_statsd_port() -> Option<u16> {
    crate::DEFAULT_STATSD_PORT
}

fn default_statsd_queue_size() -> usize {
    crate::DEFAULT_STATSD_QUEUE_SIZE
}

fn default_statsd_buffer_size() -> usize {
    crate::DEFAULT_STATSD_BUFFER_SIZE
}

impl Default for Metrics {
    /// Default metrics config
    fn default() -> Self {
        Self {
            statsd_host: None,
            statsd_port: default_statsd_port(),
            statsd_queue_size: default_statsd_queue_size(),
            statsd_buffer_size: default_statsd_buffer_size(),
            statsd_prefix: String::from(""),
        }
    }
}

impl From<Metrics> for crate::Metrics {
    fn from(metrics: Metrics) -> Self {
        Self {
            statsd_host: metrics.statsd_host.clone(),
            statsd_port: metrics.statsd_port,
            statsd_queue_size: metrics.statsd_queue_size,
            statsd_buffer_size: metrics.statsd_buffer_size,
            statsd_prefix: metrics.statsd_prefix,
        }
    }
}

/// The TOML serialization for an [Include](crate::Include) structure.
#[derive(Serialize, Deserialize)]
struct Include {
    path: String,
}

/// The TOML serialization for a Secret structure.
#[derive(Validate, Serialize, Deserialize, Clone)]
struct Secret {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    #[validate(length(min = 1, max = 96), regex(path = "RE_VALID_REFERENCE"))]
    reference: String,
    #[validate(length(min = 1))]
    path: Option<String>,
    #[validate(length(min = 1))]
    env_var: Option<String>,
}

impl TryFrom<&Secret> for crate::config::Secret {
    type Error = crate::SecretConversionError;

    fn try_from(secret: &Secret) -> Result<Self, Self::Error> {
        Ok(Self {
            reference: secret.reference.clone(),
            location: match (&secret.path, &secret.env_var) {
                (Some(path), None) => crate::SecretLocation::File(PathBuf::from(path)),
                (None, Some(env_var)) => crate::SecretLocation::EnvVar(env_var.clone()),
                _ => return Err(Self::Error::InvalidSecretLocation),
            },
        })
    }
}
/// The TOML serialization for a Plugin structure.
#[derive(Validate, Serialize, Deserialize, Clone)]
struct Plugin {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    #[validate(length(min = 1, max = 96), regex(path = "RE_VALID_REFERENCE"))]
    reference: String,
    #[validate(length(min = 1))]
    path: Option<String>,
    #[validate(length(min = 1))]
    uri: Option<String>,
    #[validate(length(min = 1))]
    authorization_header: Option<String>,
    #[validate(length(min = 1))]
    verification: Option<String>,
    #[validate(length(min = 1))]
    bytes: Option<Vec<u8>>,
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

impl TryFrom<&Plugin> for crate::config::Plugin {
    type Error = crate::PluginConversionError;

    fn try_from(plugin: &Plugin) -> Result<Self, Self::Error> {
        Ok(Self {
            reference: plugin.reference.clone(),
            location: match (&plugin.path, &plugin.uri, &plugin.bytes) {
                (Some(path), None, None) => crate::PluginLocation::Local(PathBuf::from(path)),
                (None, Some(uri), None) => crate::PluginLocation::Remote(uri.parse::<Url>()?),
                (None, None, Some(bytes)) => {
                    crate::PluginLocation::Bytes(Bytes::from(bytes.clone()))
                }
                _ => return Err(Self::Error::InvalidLocation),
            },
            access: match &plugin.authorization_header {
                Some(header) => crate::PluginAccess::Header(header.clone()),
                None => crate::PluginAccess::None,
            },
            verification: match &plugin.verification {
                Some(verification) => match verification.split_once(':') {
                    Some(("sha256", digest)) => {
                        crate::PluginVerification::Sha256(bytes::Bytes::from(hex::decode(digest)?))
                    }
                    Some((_, _)) => crate::PluginVerification::None,
                    None => crate::PluginVerification::None,
                },
                None => crate::PluginVerification::None,
            },
            weight: plugin.weight,
            config: toml_map_to_json(plugin.config.clone()),
            permissions: plugin.permissions.clone().into(),
        })
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

/// The TOML serialization for a [Permissions](crate::Permissions) structure.
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

/// The TOML serialization for a [Preset](crate::Preset) structure.
#[derive(Validate, Serialize, Deserialize, Clone)]
struct Preset {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    #[validate(length(min = 1, max = 96), regex(path = "RE_VALID_REFERENCE"))]
    reference: String,
    #[validate(length(min = 1))]
    plugins: Vec<String>,
}

/// The TOML serialization for a [Resource](crate::Resource) structure.
#[derive(Serialize, Deserialize, Clone)]
struct Resource {
    routes: Vec<String>,
    #[serde(default = "default_resource_prefix")]
    prefix: bool,
    #[serde(default = "default_resource_exact")]
    exact: bool,
    plugins: Vec<String>,
    // TODO: default timeout
    timeout: Option<u64>,
}

/// Resources default to being a prefix.
///
/// This allows the default resource to simply be the "/" route.
fn default_resource_prefix() -> bool {
    true
}

/// Resources default to being inexact.
///
/// This avoids surprises when matching against "/resource" and a request for "/resource/" is made.
fn default_resource_exact() -> bool {
    false
}

fn resolve_path<'a, B, P>(base: &'a B, path: &'a P) -> Result<PathBuf, ConfigFileError>
where
    B: 'a + ?Sized + AsRef<Path>,
    P: 'a + ?Sized + AsRef<Path>,
{
    let joined_path = base
        .as_ref()
        .parent()
        .ok_or(ConfigFileError::MissingParent(
            base.as_ref().to_string_lossy().to_string(),
        ))?
        .join(path);
    Ok(fs::canonicalize(joined_path)?)
}

/// Loads a TOML config file into a [`Config`](crate::Config) structure.
pub fn load_config<'a, P>(config_path: &'a P) -> Result<crate::Config, ConfigFileError>
where
    P: 'a + ?Sized + AsRef<Path>,
{
    let mut loaded_files = HashSet::new();

    fn load_config_recursive<'a, P>(
        config_path: &'a P,
        loaded_files: &mut HashSet<OsString>,
    ) -> Result<Config, ConfigFileError>
    where
        P: 'a + ?Sized + AsRef<Path>,
    {
        let path_string = config_path.as_ref().as_os_str().to_os_string();
        if loaded_files.contains(&path_string) {
            return Err(ConfigFileError::CircularInclude(
                path_string.to_string_lossy().to_string(),
            ));
        } else {
            loaded_files.insert(path_string);
        }
        let toml_data = match fs::read_to_string(config_path) {
            Ok(data) => data,
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => {
                    return Err(ConfigFileError::NotFound(
                        config_path.as_ref().to_path_buf(),
                    ));
                }
                _ => Err(ConfigFileError::IO(err)),
            }?,
        };
        let mut root: Config = toml::from_str(&toml_data)?;
        let base = config_path
            .as_ref()
            .parent()
            .ok_or(ConfigFileError::MissingParent(
                config_path.as_ref().to_string_lossy().to_string(),
            ))?;

        for include in &root.includes {
            let include_path = base.join(&include.path);
            if !include_path.exists() {
                return Err(ConfigFileError::IncludedFileNotFound(
                    include_path.to_string_lossy().to_string(),
                ));
            }
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

        // Resolve plugins relative to config path or validate that remote URIs are secure.
        root.plugins = root
            .plugins
            .iter()
            .map(|plugin| -> Result<Plugin, ConfigFileError> {
                Ok(Plugin {
                    reference: plugin.reference.clone(),
                    path: plugin
                        .path
                        .as_ref()
                        .map(|path| {
                            resolve_path(config_path, Path::new(path.as_str()))
                                .map(|path| path.to_string_lossy().to_string())
                        })
                        .transpose()?,
                    uri: plugin
                        .uri
                        .as_ref()
                        .map(|uri| {
                            uri.parse::<Url>().map_err(ConfigFileError::from).and_then(
                                |parsed_uri| {
                                    if parsed_uri.scheme() == "https" {
                                        Ok(uri.clone())
                                    } else {
                                        Err(ConfigFileError::InsecureRemoteUri(uri.clone()))
                                    }
                                },
                            )
                        })
                        .transpose()?,
                    authorization_header: plugin.authorization_header.clone(),
                    verification: plugin.verification.clone(),
                    bytes: plugin.bytes.clone(),
                    weight: plugin.weight,
                    config: plugin.config.clone(),
                    permissions: plugin.permissions.clone(),
                })
            })
            .collect::<Result<Vec<Plugin>, ConfigFileError>>()?;

        Ok(root)
    }

    // Load the raw serialization format and resolve includes
    let root = load_config_recursive(config_path, &mut loaded_files)?;

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
        runtime: root.runtime.into(),
        state: root.state.into(),
        thresholds: root.thresholds.into(),
        metrics: root.metrics.into(),
        secrets: root
            .secrets
            .iter()
            .map(|secret: &Secret| secret.try_into())
            .collect::<Result<Vec<crate::Secret>, _>>()
            .map_err(|err| ConfigFileError::InvalidSecretConfig(err.to_string()))?,
        plugins: root
            .plugins
            .iter()
            .map(|plugin| plugin.try_into())
            .collect::<Result<Vec<crate::Plugin>, _>>()
            .map_err(|err| ConfigFileError::InvalidPluginConfig(err.to_string()))?,
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
                routes: crate::Resource::expand_routes(
                    &resource.routes,
                    resource.exact,
                    resource.prefix,
                ),
                plugins: resource.plugins.iter().map(resolve_reference).collect(),
                timeout: resource.timeout,
            })
            .collect(),
    };
    for plugin in &config.plugins {
        // Read plugin configs to surface type errors immediately
        validate_plugin_config(&plugin.config)?;
    }
    for resource in &config.resources {
        // Resolve plugins to surface resolution errors immediately
        resource.resolve_plugins(&config)?;
    }
    Ok(config)
}

fn validate_plugin_config(
    config: &serde_json::map::Map<String, serde_json::Value>,
) -> Result<(), ConfigFileError> {
    for (key, value) in config {
        match value {
            serde_json::Value::Null => (),
            serde_json::Value::Bool(_) => (),
            serde_json::Value::Number(_) => (),
            serde_json::Value::String(_) => (),
            serde_json::Value::Array(array) => {
                for value in array {
                    match value {
                        serde_json::Value::Null => (),
                        serde_json::Value::Bool(_) => (),
                        serde_json::Value::Number(_) => (),
                        serde_json::Value::String(_) => (),
                        serde_json::Value::Array(_) => {
                            return Err(ConfigFileError::InvalidPluginConfig(format!(
                                "config key '{key}' contained an array with a non-primitive subtype"
                            )));
                        }
                        serde_json::Value::Object(_) => {
                            return Err(ConfigFileError::InvalidPluginConfig(format!(
                                "config key '{key}' contained an array with a non-primitive subtype"
                            )));
                        }
                    }
                }
            }
            serde_json::Value::Object(obj) => {
                for (_, value) in obj {
                    match value {
                        serde_json::Value::Null => (),
                        serde_json::Value::Bool(_) => (),
                        serde_json::Value::Number(_) => (),
                        serde_json::Value::String(_) => (),
                        serde_json::Value::Array(_) => {
                            return Err(ConfigFileError::InvalidPluginConfig(format!(
                                "config key '{key}' contained an object with a non-primitive subtype"
                            )));
                        }
                        serde_json::Value::Object(_) => {
                            return Err(ConfigFileError::InvalidPluginConfig(format!(
                                "config key '{key}' contained an object with a non-primitive subtype"
                            )));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use matchit::Router;

    fn build_plugins() -> Result<(), Box<dyn std::error::Error>> {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

        if !project_root
            .join("tests/dist/plugins/bulwark_blank_slate.wasm")
            .exists()
        {
            bulwark_build::build_plugin(
                project_root.join("crates/sdk/examples/blank-slate"),
                project_root.join("tests/dist/plugins/bulwark_blank_slate.wasm"),
                &[],
                true,
            )?;
            assert!(project_root
                .join("tests/dist/plugins/bulwark_blank_slate.wasm")
                .exists());
        }

        if !project_root
            .join("tests/dist/plugins/bulwark_evil_bit.wasm")
            .exists()
        {
            bulwark_build::build_plugin(
                project_root.join("crates/sdk/examples/evil-bit"),
                project_root.join("tests/dist/plugins/bulwark_evil_bit.wasm"),
                &[],
                true,
            )?;
            assert!(project_root
                .join("tests/dist/plugins/bulwark_evil_bit.wasm")
                .exists());
        }

        Ok(())
    }

    #[test]
    fn test_deserialize() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root: Config = toml::from_str(
            r#"
        [service]
        port = 10002

        [state]
        redis_uri = "redis://10.0.0.1:6379"

        [thresholds]
        restrict = 0.75

        [metrics]
        statsd_host = "10.0.0.2"
        statsd_prefix = "bulwark_"

        [[include]]
        path = "default.toml"

        [[plugin]]
        ref = "evil_bit"
        path = "bulwark_evil_bit.wasm"

        [[preset]]
        ref = "custom"
        plugins = ["evil-bit"]

        [[resource]]
        routes = ["/"]
        plugins = ["custom"]
        timeout = 25
    "#,
        )?;

        assert_eq!(root.service.port, 10002); // non-default
        assert_eq!(root.service.admin_port, crate::DEFAULT_ADMIN_PORT);
        assert_eq!(
            root.state.redis_uri,
            Some(String::from("redis://10.0.0.1:6379"))
        );

        assert_eq!(root.metrics.statsd_host, Some(String::from("10.0.0.2")));
        assert_eq!(root.metrics.statsd_port, Some(8125));
        assert_eq!(root.metrics.statsd_prefix, String::from("bulwark_"));

        assert_eq!(root.thresholds.restrict, 0.75); // non-default
        assert_eq!(
            root.thresholds.suspicious,
            crate::DEFAULT_SUSPICIOUS_THRESHOLD
        );
        assert_eq!(root.thresholds.trust, crate::DEFAULT_TRUST_THRESHOLD);

        assert_eq!(root.includes.len(), 1);
        assert_eq!(root.includes.first().unwrap().path, "default.toml");

        assert_eq!(root.plugins.len(), 1);
        assert_eq!(root.plugins.first().unwrap().reference, "evil_bit");
        assert!(root
            .plugins
            .first()
            .unwrap()
            .path
            .clone()
            .unwrap()
            .ends_with("bulwark_evil_bit.wasm"));
        assert_eq!(
            root.plugins.first().unwrap().config,
            toml::map::Map::default()
        );

        assert_eq!(root.presets.len(), 1);
        assert_eq!(root.presets.first().unwrap().reference, "custom");
        assert_eq!(root.presets.first().unwrap().plugins, vec!["evil-bit"]);

        assert_eq!(root.resources.len(), 1);
        assert_eq!(
            root.resources.first().unwrap().routes,
            vec!["/".to_string()]
        );
        assert_eq!(root.resources.first().unwrap().plugins, vec!["custom"]);
        assert_eq!(root.resources.first().unwrap().timeout, Some(25));

        Ok(())
    }

    #[test]
    fn test_load_config() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root: crate::config::Config = load_config("tests/main.toml")?;

        assert_eq!(root.service.port, 10002); // non-default
        assert_eq!(root.service.admin_port, crate::DEFAULT_ADMIN_PORT);

        assert_eq!(
            root.state.redis_uri,
            Some(String::from("redis://127.0.0.1:6379"))
        );
        // We don't know how many CPUs will be present but we know it's not zero,
        // so pool size has to be at least 4.
        assert!(root.state.redis_pool_size >= 4);

        assert_eq!(root.metrics.statsd_host, Some(String::from("10.0.0.2")));
        assert_eq!(root.metrics.statsd_port, Some(8125));
        assert_eq!(root.metrics.statsd_prefix, String::from("bulwark_"));

        assert_eq!(root.thresholds.restrict, 0.75); // non-default
        assert_eq!(
            root.thresholds.suspicious,
            crate::DEFAULT_SUSPICIOUS_THRESHOLD
        );
        assert_eq!(root.thresholds.trust, crate::DEFAULT_TRUST_THRESHOLD);

        assert_eq!(root.plugins.len(), 2);
        assert_eq!(root.plugins.first().unwrap().reference, "evil_bit");
        match root.plugins.first().unwrap().location.clone() {
            crate::PluginLocation::Local(path) => assert!(path.ends_with("bulwark_evil_bit.wasm")),
            crate::PluginLocation::Remote(_) => panic!("should not be https"),
            crate::PluginLocation::Bytes(_) => panic!("should not be bytes"),
        }

        assert_eq!(
            root.plugins.first().unwrap().config,
            serde_json::map::Map::default()
        );

        assert_eq!(root.presets.len(), 2);
        assert_eq!(root.presets.first().unwrap().reference, "default");
        assert_eq!(root.presets.last().unwrap().reference, "starter_preset");
        assert_eq!(
            root.presets.first().unwrap().plugins,
            vec![
                crate::config::Reference::Plugin("evil_bit".to_string()),
                crate::config::Reference::Preset("starter_preset".to_string())
            ]
        );
        assert_eq!(
            root.presets.get(1).unwrap().plugins,
            vec![crate::config::Reference::Plugin("blank_slate".to_string())]
        );

        assert_eq!(root.resources.len(), 1);
        assert_eq!(
            root.resources.first().unwrap().routes,
            vec!["/{*suffix}".to_string(), "/".to_string()]
        );
        assert_eq!(
            root.resources.first().unwrap().plugins,
            vec![crate::config::Reference::Preset("default".to_string())]
        );
        assert_eq!(root.resources.first().unwrap().timeout, Some(25));

        Ok(())
    }

    #[test]
    fn test_load_config_overlapping_preset() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root: crate::config::Config = load_config("tests/overlapping_preset.toml")?;

        let resource = root.resources.first().unwrap();
        let plugins = resource.resolve_plugins(&root)?;
        assert_eq!(plugins.len(), 2);
        assert_eq!(
            plugins.first().unwrap().reference,
            root.plugin("blank_slate").unwrap().reference
        );
        assert_eq!(
            plugins.last().unwrap().reference,
            root.plugin("evil_bit").unwrap().reference
        );

        Ok(())
    }

    #[test]
    fn test_load_config_circular_include() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

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
        build_plugins()?;

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
        build_plugins()?;

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
        build_plugins()?;

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
        build_plugins()?;

        let result = load_config("tests/duplicate_mixed.toml");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "duplicate named plugin or preset: 'blank_slate'"
        );
        Ok(())
    }

    #[test]
    fn test_load_config_invalid_config_array() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let result = load_config("tests/invalid_config_array.toml");
        assert!(result.is_err());
        assert_eq!(result
            .unwrap_err()
            .to_string(),
            "invalid plugin config: config key 'key' contained an array with a non-primitive subtype");
        Ok(())
    }

    #[test]
    fn test_load_config_invalid_config_object() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let result = load_config("tests/invalid_config_object.toml");
        assert!(result.is_err());
        assert_eq!(result
            .unwrap_err()
            .to_string(),
            "invalid plugin config: config key 'key' contained an object with a non-primitive subtype");
        Ok(())
    }

    #[test]
    fn test_load_config_invalid_plugin_reference() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let result = load_config("tests/invalid_plugin_reference.toml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("reference:"));
        assert!(err.to_string().contains("\"not/okay-reference\""));
        Ok(())
    }

    #[test]
    fn test_load_config_valid_numeric_plugin_reference() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root = load_config("tests/valid_numeric_plugin_reference.toml")?;

        let resource = root.resources.first().unwrap();
        let plugins = resource.resolve_plugins(&root)?;
        assert_eq!(plugins.len(), 1);
        assert_eq!(
            plugins.first().unwrap().reference,
            root.plugin("blank_slate_v2").unwrap().reference
        );

        Ok(())
    }

    #[test]
    fn test_load_config_resolution_missing() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

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
        build_plugins()?;

        let result = load_config("tests/missing_include.toml");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .starts_with("no such file or directory"));
        Ok(())
    }

    #[test]
    fn test_load_config_exact_resource_route() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root = load_config("tests/exact_resource_route.toml")?;

        assert_eq!(root.resources.len(), 1);
        assert_eq!(
            root.resources.first().unwrap().routes,
            vec![
                "/user/{userid}/{*suffix}".to_string(),
                "/user/{userid}".to_string(),
                "/{*suffix}".to_string(),
                "/".to_string()
            ]
        );

        Ok(())
    }

    // Integration test w/ matchit to verify that generated route syntax is valid.
    #[test]
    fn test_exact_resource_route() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root = load_config("tests/exact_resource_route.toml")?;
        let mut router = Router::new();

        for resource in root.resources.iter() {
            for route in resource.routes.iter() {
                router.insert(route, true)?;
            }
        }
        for route in [
            "/user/anonymous/account/suffix",
            "/user/anonymous",
            "/arbitrary/suffix",
            "/",
        ] {
            assert!(router.at(route)?.value);
        }
        assert_eq!(
            router
                .at("/user/anonymous/account/suffix")?
                .params
                .get("userid"),
            Some("anonymous")
        );
        assert_eq!(
            router
                .at("/user/anonymous/account/suffix")?
                .params
                .get("suffix"),
            Some("account/suffix")
        );
        assert_eq!(
            router.at("/user/anonymous")?.params.get("userid"),
            Some("anonymous")
        );
        assert_eq!(router.at("/user/anonymous")?.params.get("suffix"), None);
        assert_eq!(
            router.at("/arbitrary/suffix")?.params.get("suffix"),
            Some("arbitrary/suffix")
        );

        Ok(())
    }

    #[test]
    fn test_load_config_inexact_resource_route() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root = load_config("tests/inexact_resource_route.toml")?;

        assert_eq!(root.resources.len(), 1);
        assert_eq!(
            root.resources.first().unwrap().routes,
            vec![
                "/logout/{*suffix}".to_string(),
                "/login/{*suffix}".to_string(),
                "/api/{*suffix}".to_string(),
                "/{*suffix}".to_string(),
                "/logout/".to_string(),
                "/login/".to_string(),
                "/logout".to_string(),
                "/login".to_string(),
                "/".to_string(),
            ]
        );

        Ok(())
    }

    // Integration test w/ matchit to verify that generated route syntax is valid.
    #[test]
    fn test_inexact_resource_route() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root = load_config("tests/inexact_resource_route.toml")?;
        let mut router = Router::new();

        for resource in root.resources.iter() {
            for route in resource.routes.iter() {
                router.insert(route, true)?;
            }
        }
        for route in [
            "/logout/logout/suffix",
            "/login/login/suffix",
            "/api/api/suffix",
            "/logout/",
            "/arbitrary/suffix",
            "/login/",
            "/logout",
            "/login",
            "/",
        ] {
            assert!(router.at(route)?.value);
        }
        assert_eq!(
            router.at("/logout/logout/suffix")?.params.get("suffix"),
            Some("logout/suffix")
        );
        assert_eq!(
            router.at("/login/login/suffix")?.params.get("suffix"),
            Some("login/suffix")
        );
        assert_eq!(
            router.at("/api/api/suffix")?.params.get("suffix"),
            Some("api/suffix")
        );
        assert_eq!(
            router.at("/arbitrary/suffix")?.params.get("suffix"),
            Some("arbitrary/suffix")
        );

        Ok(())
    }

    #[test]
    fn test_load_config_prefixed_resource_route() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root = load_config("tests/prefixed_resource_route.toml")?;

        assert_eq!(root.resources.len(), 1);
        assert_eq!(
            root.resources.first().unwrap().routes,
            vec![
                "/api/{*suffix}".to_string(),
                "/{*suffix}".to_string(),
                "/".to_string(),
            ]
        );

        Ok(())
    }

    // Integration test w/ matchit to verify that generated route syntax is valid.
    #[test]
    fn test_prefixed_resource_route_match() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root = load_config("tests/prefixed_resource_route.toml")?;
        let mut router = Router::new();

        for resource in root.resources.iter() {
            for route in resource.routes.iter() {
                router.insert(route, true)?;
            }
        }
        for route in ["/api/api/suffix", "/arbitrary/suffix", "/"] {
            assert!(router.at(route)?.value);
        }
        assert_eq!(
            router.at("/api/api/suffix")?.params.get("suffix"),
            Some("api/suffix")
        );
        assert_eq!(
            router.at("/arbitrary/suffix")?.params.get("suffix"),
            Some("arbitrary/suffix")
        );

        Ok(())
    }

    #[test]
    fn test_load_config_nonprefixed_resource_route() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root = load_config("tests/nonprefixed_resource_route.toml")?;

        assert_eq!(root.resources.len(), 1);
        assert_eq!(
            root.resources.first().unwrap().routes,
            vec![
                "/logout/".to_string(),
                "/login/".to_string(),
                "/logout".to_string(),
                "/login".to_string(),
                "/".to_string(),
            ]
        );

        Ok(())
    }

    // Integration test w/ matchit to verify that generated route syntax is valid.
    #[test]
    fn test_nonprefixed_resource_route_match() -> Result<(), Box<dyn std::error::Error>> {
        build_plugins()?;

        let root = load_config("tests/nonprefixed_resource_route.toml")?;
        let mut router = Router::new();

        for resource in root.resources.iter() {
            for route in resource.routes.iter() {
                router.insert(route, true)?;
            }
        }
        assert!(router.at("/no/match").is_err());
        for route in ["/logout/", "/login/", "/logout", "/login", "/"] {
            assert!(router.at(route)?.value);
        }

        Ok(())
    }

    #[test]
    fn test_resolve_path() -> Result<(), Box<dyn std::error::Error>> {
        let base = PathBuf::new().join(".");
        let path = Path::new("./src");
        let resolved_path = resolve_path(&base, path)?;

        assert!(resolved_path.ends_with("crates/config/src"));
        Ok(())
    }
}
