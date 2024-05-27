//! The config module provides the internal representation of Bulwark's configuration.

use crate::ResolutionError;
use bytes::Bytes;
use itertools::Itertools;
use regex::Regex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use url::Url;
use validator::Validate;

lazy_static! {
    static ref RE_VALID_REFERENCE: Regex = Regex::new(r"^[_a-z]+$").unwrap();
}

/// The root of a Bulwark configuration.
///
/// Wraps all child configuration structures and provides the internal representation of Bulwark's configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Configuration for the services being launched.
    pub service: Service,
    /// Configuration for the services being launched.
    pub runtime: Runtime,
    /// Configuration for state managed by Bulwark plugins.
    pub state: State,
    /// Configuration for the decision thresholds.
    pub thresholds: Thresholds,
    /// Configuration for metrics collection.
    pub metrics: Metrics,
    /// A list of configurations for individual secrets.
    pub secrets: Vec<Secret>,
    /// A list of configurations for individual plugins.
    pub plugins: Vec<Plugin>,
    /// A list of plugin groups that allows a plugin set to be loaded with a single reference.
    pub presets: Vec<Preset>,
    /// A list of routes that maps from resource paths to plugins or presets.
    pub resources: Vec<Resource>,
    // TODO: It might make sense to convert the vectors to maps since both routes and references should be unique.
}

impl Config {
    /// Looks up the [`Secret`] corresponding to the `reference` string.
    ///
    /// # Arguments
    ///
    /// * `reference` - A string that corresponds to a [`Secret::reference`] value.
    pub fn secret<'a>(&self, reference: &str) -> Option<&Secret>
    where
        Secret: 'a,
    {
        self.secrets
            .iter()
            .find(|&secret| secret.reference == reference)
    }

    /// Looks up the [`Plugin`] corresponding to the `reference` string.
    ///
    /// # Arguments
    ///
    /// * `reference` - A string that corresponds to a [`Plugin::reference`] value.
    pub fn plugin<'a>(&self, reference: &str) -> Option<&Plugin>
    where
        Plugin: 'a,
    {
        self.plugins
            .iter()
            .find(|&plugin| plugin.reference == reference)
    }

    /// Looks up the [`Preset`] corresponding to the `reference` string.
    ///
    /// # Arguments
    ///
    /// * `reference` - A string that corresponds to a [`Preset::reference`] value.
    pub fn preset<'a>(&self, reference: &str) -> Option<&Preset>
    where
        Preset: 'a,
    {
        self.presets
            .iter()
            .find(|&preset| preset.reference == reference)
    }
}

/// Configuration for the services being launched.
#[derive(Debug, Clone)]
pub struct Service {
    /// The port for the primary service.
    pub port: u16,
    /// The port for the admin service and health checks.
    pub admin_port: u16,
    /// True if the admin service is enabled, false otherwise.
    pub admin_enabled: bool,
    /// The number of trusted proxy hops expected to be exterior to Bulwark.
    ///
    /// This number does not include Bulwark or the proxy hosting it in the proxy hop count. Zero implies that
    /// there are no other proxies exterior to Bulwark. This is used to ensure the `Forwarded` and `X-Forwarded-For`
    /// headers are not spoofed. If this is set incorrectly, the client IP reported to plugins will be incorrect.
    pub proxy_hops: u8,
    // TODO: it may be useful to introduce an "auto" setting for `proxy_hops` since it's possible to auto-discover
}

/// The default [`Service::port`] value.
pub const DEFAULT_PORT: u16 = 8089;
/// The default [`Service::admin_port`] value.
pub const DEFAULT_ADMIN_PORT: u16 = 8090;

impl Default for Service {
    /// Default service config
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            admin_port: DEFAULT_ADMIN_PORT,
            admin_enabled: true,
            proxy_hops: 0,
        }
    }
}

/// Configuration for the runtime environment.
#[derive(Debug, Clone)]
pub struct Runtime {
    /// The maximum number of concurrent incoming requests that the runtime will process before blocking.
    pub max_concurrent_requests: usize,
    /// The maximum number of concurrent plugin tasks that the runtime will launch.
    pub max_plugin_tasks: usize,
}

/// The default [`Runtime::max_concurrent_requests`] value.
pub const DEFAULT_MAX_CONCURRENT_REQUESTS: usize = 8;

/// The default [`Runtime::max_plugin_tasks`] value.
pub const DEFAULT_MAX_PLUGIN_TASKS: usize = 16;

impl Default for Runtime {
    /// Default runtime config
    fn default() -> Self {
        Self {
            max_concurrent_requests: DEFAULT_MAX_CONCURRENT_REQUESTS,
            max_plugin_tasks: DEFAULT_MAX_PLUGIN_TASKS,
        }
    }
}

/// Configuration for state managed by Bulwark plugins.
#[derive(Debug, Clone)]
pub struct State {
    /// The URI for the external Redis state store.
    pub redis_uri: Option<String>,
    /// The size of the Redis connection pool.
    pub redis_pool_size: usize,
}

impl Default for State {
    /// Default runtime config
    fn default() -> Self {
        Self {
            redis_uri: None,
            redis_pool_size: num_cpus::get_physical() * 4,
        }
    }
}

/// Configuration for the decision thresholds.
///
/// No threshold is necessary for the default `allowed` outcome because it is defined by the range between the
/// `suspicious` threshold and the `trusted` threshold. The thresholds must have values in descending order, with
/// `restrict` > `suspicious` > `trusted`. None of the threshold values may be equal.
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// True if the primary service should take no action in response to restrict decisions.
    pub observe_only: bool,
    /// Any decision value above the `restrict` threshold will cause the corresponding request to be blocked.
    pub restrict: f64,
    /// Any decision value above the `suspicious` threshold will cause the corresponding request to be flagged as
    /// suspicious but it will still be allowed.
    pub suspicious: f64,
    /// Any decision value below the `trust` threshold will cause the corresponding request to be flagged as trusted.
    /// This primarily affects plugins which use feedback loops.
    pub trust: f64,
}

/// The default [`Thresholds::observe_only`] value.
pub const DEFAULT_OBSERVE_ONLY: bool = false;
/// The default [`Thresholds::restrict`] value.
pub const DEFAULT_RESTRICT_THRESHOLD: f64 = 0.8;
/// The default [`Thresholds::suspicious`] value.
pub const DEFAULT_SUSPICIOUS_THRESHOLD: f64 = 0.6;
/// The default [`Thresholds::trust`] value.
pub const DEFAULT_TRUST_THRESHOLD: f64 = 0.2;

impl Default for Thresholds {
    /// Default decision thresholds.
    fn default() -> Self {
        Self {
            observe_only: DEFAULT_OBSERVE_ONLY,
            restrict: DEFAULT_RESTRICT_THRESHOLD,
            suspicious: DEFAULT_SUSPICIOUS_THRESHOLD,
            trust: DEFAULT_TRUST_THRESHOLD,
        }
    }
}

/// Configuration for metrics collection.
#[derive(Debug, Clone)]
pub struct Metrics {
    pub statsd_host: Option<String>,
    pub statsd_port: Option<u16>,
    pub statsd_queue_size: usize,
    pub statsd_buffer_size: usize,
    pub statsd_prefix: String,
}

/// The default [`Metrics::statsd_port`] value.
pub const DEFAULT_STATSD_PORT: Option<u16> = Some(8125);

/// The default [`Metrics::statsd_queue_size`] value.
pub const DEFAULT_STATSD_QUEUE_SIZE: usize = 5000;

/// The default [`Metrics::statsd_buffer_size`] value.
pub const DEFAULT_STATSD_BUFFER_SIZE: usize = 1024;

impl Default for Metrics {
    /// Default metrics config
    fn default() -> Self {
        Self {
            statsd_host: None,
            statsd_port: DEFAULT_STATSD_PORT,
            statsd_queue_size: DEFAULT_STATSD_QUEUE_SIZE,
            statsd_buffer_size: DEFAULT_STATSD_BUFFER_SIZE,
            statsd_prefix: String::from(""),
        }
    }
}

/// Configuration for a secret that Bulwark will need to reference.
#[derive(Debug, Validate, Clone, Default)]
pub struct Secret {
    /// The secret reference key. Should be limited to ASCII lowercase a-z plus underscores. Maximum 96 characters.
    #[validate(length(min = 1, max = 96), regex(path = "RE_VALID_REFERENCE"))]
    pub reference: String,
    /// The location where the secret can be loaded from.
    pub location: SecretLocation,
}

/// The location where a secret is stored.
#[derive(Debug, Clone)]
pub enum SecretLocation {
    /// The secret is an environment variable.
    EnvVar(String),
    /// The secret is mounted as a file.
    File(PathBuf),
}

impl Default for SecretLocation {
    /// Defaults to an unusable empty environment variable.
    fn default() -> Self {
        Self::EnvVar(String::new())
    }
}

/// The location where the plugin WASM can be loaded from.
#[derive(Debug, Clone)]
pub enum PluginLocation {
    /// The plugin is a local file.
    Local(PathBuf),
    /// The plugin is a remote file served over HTTPS.
    Remote(Url),
    /// The plugin is an binary blob.
    Bytes(Bytes),
}

impl Default for PluginLocation {
    /// Defaults to an unusable empty byte vector.
    fn default() -> Self {
        Self::Bytes(Bytes::new())
    }
}

impl Display for PluginLocation {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            PluginLocation::Local(path) => write!(f, "[local: {}]", path.display()),
            PluginLocation::Remote(uri) => write!(f, "[remote: {}]", uri),
            PluginLocation::Bytes(bytes) => write!(f, "[{} bytes]", bytes.len()),
        }
    }
}

/// The access control applied to the plugin.
///
/// Typically used in conjunction with privately distributed remote plugins.
#[derive(Debug, Clone)]
pub enum PluginAccess {
    /// No authentication provided.
    None,
    /// The authentication is provided via an HTTPS Authorization header.
    ///
    /// The entire header value will be sent verbatim. This will generally only work for
    /// basic authentication or bearer tokens.
    ///
    /// This is a secret reference. A corresponding [`Secret`] must be provided.
    Header(String),
}

impl Default for PluginAccess {
    /// Defaults to no authentication.
    fn default() -> Self {
        Self::None
    }
}

/// Verification that plugin contents are what was expected.
#[derive(Debug, Clone)]
pub enum PluginVerification {
    /// No verification.
    None,
    /// The plugin is hashed with a SHA-256 digest.
    Sha256(Bytes),
}

impl Default for PluginVerification {
    /// Defaults to no verification.
    fn default() -> Self {
        Self::None
    }
}

/// The configuration for an individual plugin.
///
/// This structure will be wrapped by structs in the host environment.
#[derive(Debug, Validate, Clone, Default)]
pub struct Plugin {
    /// The plugin reference key. Should be limited to ASCII lowercase a-z plus underscores. Maximum 96 characters.
    #[validate(length(min = 1, max = 96), regex(path = "RE_VALID_REFERENCE"))]
    pub reference: String,
    /// The location where the plugin WASM can be loaded from.
    pub location: PluginLocation,
    /// The access requirements for the plugin. If the plugin requires authentication, this can be provided here.
    pub access: PluginAccess,
    /// Verification that plugin contents match what was expected.
    pub verification: PluginVerification,
    /// A weight to multiply this plugin's decision values by.
    ///
    /// A 1.0 value has no effect on the decision. See [`bulwark_decision::Decision::weight`].
    #[validate(range(min = 0.0))]
    pub weight: f64,
    // TODO: this might be better represented as a valuable::Mappable / valuable::Value
    /// JSON-serializable configuration passed into the plugin environment.
    ///
    /// The host environment will not do anything with this value beyond serialization.
    pub config: serde_json::map::Map<String, serde_json::Value>,
    /// The permissions granted to this plugin.
    ///
    /// Any attempt to perform an operation within the plugin sandbox that requires a permission to be set will fail.
    pub permissions: Permissions,
}

/// The default [`Plugin::weight`] value.
pub const DEFAULT_PLUGIN_WEIGHT: f64 = 1.0;

/// The permissions granted to an associated plugin.
#[derive(Debug, Clone, Default)]
pub struct Permissions {
    /// A list of environment variables a plugin may acquire values for.
    ///
    /// This permission may be used to grant a plugin fine-grained access to a specific secret.
    pub env: Vec<String>,
    /// A list of domains that a plugin may make HTTP requests to.
    ///
    /// The permission value must case-insensitively match the entire host component of the request URI.
    pub http: Vec<String>,
    /// A list of key prefixes that a plugin may get or set within the external state store.
    ///
    /// This permission also affects rate limits and circuit breakers since they also use the external state store.
    pub state: Vec<String>,
}

/// A mapping between a reference identifier and a list of plugins that form a preset plugin group.
#[derive(Debug, Validate, Clone)]
pub struct Preset {
    /// The preset reference key. Should be limited to ASCII lowercase a-z plus underscores. Maximum 96 characters.
    #[validate(length(min = 1, max = 96), regex(path = "RE_VALID_REFERENCE"))]
    pub reference: String,
    /// The list of references to plugins and other presets contained within this preset.
    #[validate(length(min = 1))]
    pub plugins: Vec<Reference>,
}

impl Preset {
    /// Resolves all references within a `Preset`, producing a flattened list of the corresponding [`Plugin`]s.
    ///
    /// # Arguments
    ///
    /// * `config` - A [`Config`] reference to perform lookups againsts.
    ///   `Preset`s do not maintain their own references to their parent [`Config`] so this must be passed in.
    ///
    /// See [`Config::plugin`] and [`Config::preset`].
    pub fn resolve_plugins<'a>(
        &'a self,
        config: &'a Config,
    ) -> Result<Vec<&Plugin>, ResolutionError> {
        let mut resolved_presets = HashSet::new();
        self.resolve_plugins_recursive(config, &mut resolved_presets)
    }

    /// Resolves all references, checking for cycles.
    fn resolve_plugins_recursive<'a>(
        &'a self,
        config: &'a Config,
        resolved_presets: &mut HashSet<String>,
    ) -> Result<Vec<&Plugin>, ResolutionError> {
        let mut plugins: HashMap<String, &Plugin> = HashMap::with_capacity(self.plugins.len());
        for reference in &self.plugins {
            match reference {
                Reference::Plugin(ref_name) => {
                    if let Some(plugin) = config.plugin(ref_name.as_str()) {
                        plugins.insert(plugin.reference.to_string(), plugin);
                    }
                }
                Reference::Preset(ref_name) => {
                    if resolved_presets.contains(ref_name) {
                        return Err(ResolutionError::CircularPreset(ref_name.to_string()));
                    } else {
                        resolved_presets.insert(ref_name.to_string());
                    }
                    if let Some(preset) = config.preset(ref_name.as_str()) {
                        let inner_plugins =
                            preset.resolve_plugins_recursive(config, resolved_presets)?;
                        for inner_plugin in inner_plugins {
                            plugins.insert(inner_plugin.reference.to_string(), inner_plugin);
                        }
                    }
                }
                Reference::Missing(ref_name) => {
                    return Err(ResolutionError::Missing(ref_name.to_string()));
                }
            }
        }
        Ok(plugins.values().cloned().collect())
    }
}

/// A mapping between a route pattern and the plugins that should be run for matching requests.
#[derive(Debug, Clone)]
pub struct Resource {
    /// The route pattern used to match requests with.
    ///
    /// Uses `matchit` router patterns.
    pub routes: Vec<String>,
    /// The plugin references for this route.
    pub plugins: Vec<Reference>,
    /// The maximum amount of time a plugin may take for each execution phase.
    pub timeout: Option<u64>,
}

impl Resource {
    /// Expands routes to make them more user-friendly.
    ///
    /// # Arguments
    ///
    /// * `routes` - The route patterns to expand.
    /// * `exact` - Whether route expansion should ignore trailing slashes or not.
    /// * `prefix` - Whether route expansion should add a catch-all '/{*suffix}' pattern to each route.
    pub(crate) fn expand_routes(routes: &[String], exact: bool, prefix: bool) -> Vec<String> {
        let mut new_routes = routes.to_vec();
        if !exact {
            for route in routes.iter() {
                let new_route = if route.ends_with('/') && route.len() > 1 {
                    route[..route.len() - 1].to_string()
                } else if !route.contains("{*") && !route.ends_with('/') {
                    route.clone() + "/"
                } else {
                    continue;
                };
                if !new_routes.contains(&new_route) {
                    new_routes.push(new_route);
                }
            }
        }
        if prefix {
            for route in new_routes.clone().iter() {
                if !route.contains("{*") {
                    let new_route = PathBuf::from(route)
                        .join("{*suffix}")
                        .to_string_lossy()
                        .to_string();
                    if !new_routes.contains(&new_route) {
                        new_routes.push(new_route);
                    }
                }
            }
        }
        new_routes.sort_by_key(|route| -(route.len() as i64));
        new_routes
    }

    /// Resolves all references within a `Resource`, producing a flattened list of the corresponding [`Plugin`]s.
    ///
    /// # Arguments
    ///
    /// * `config` - A [`Config`] reference to perform lookups againsts.
    ///   `Resource`s do not maintain their own references to their parent [`Config`] so this must be passed in.
    ///
    /// See [`Config::plugin`] and [`Config::preset`].
    pub fn resolve_plugins<'a>(
        &'a self,
        config: &'a Config,
    ) -> Result<Vec<&Plugin>, ResolutionError> {
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
                        let mut inner_plugins = preset.resolve_plugins(config)?;
                        plugins.append(&mut inner_plugins);
                    }
                }
                Reference::Missing(ref_name) => {
                    return Err(ResolutionError::Missing(ref_name.to_string()));
                }
            }
        }
        Ok(plugins
            .iter()
            .sorted_by(|a, b| Ord::cmp(&a.reference, &b.reference))
            .copied()
            .collect())
    }
}

/// Wraps reference strings and differentiates what the reference points to.
#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
pub enum Reference {
    /// A reference to a [`Plugin`].
    Plugin(String),
    /// A reference to a [`Preset`].
    Preset(String),
    /// A reference that could not be resolved.
    Missing(String),
}
