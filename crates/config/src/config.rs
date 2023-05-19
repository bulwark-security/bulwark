//! The config module provides the internal representation of Bulwark's configuration.

use regex::Regex;
use serde::Serialize;
use validator::Validate;

lazy_static! {
    static ref RE_VALID_REFERENCE: Regex = Regex::new(r"^[_a-z]+$").unwrap();
}

/// The root of a Bulwark configuration.
///
/// Wraps all child configuration structures and provides the internal representation of Bulwark's configuration.
#[derive(Debug)]
pub struct Config {
    /// Configuration for the services being launched.
    pub service: Service,
    /// Configuration for the decision thresholds.
    pub thresholds: Thresholds,
    /// A list of configurations for individual plugins.
    pub plugins: Vec<Plugin>,
    /// A list of plugin groups that allows a plugin set to be loaded with a single reference.
    pub presets: Vec<Preset>,
    /// A list of routes that maps from resource paths to plugins or presets.
    pub resources: Vec<Resource>,
    // TODO: It might make sense to convert the vectors to maps since both routes and references should be unique.
}

impl Config {
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
#[derive(Debug)]
pub struct Service {
    /// The port for the primary service.
    pub port: u16,
    /// The port for the admin service and health checks.
    pub admin_port: u16,
    /// True if the admin service is enabled, false otherwise.
    pub admin: bool,
    /// The URI for the external Redis state store.
    pub remote_state: Option<String>,
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

/// The configuration for an individual plugin.
///
/// This structure will be wrapped by structs in the host environment.
#[derive(Debug, Validate, Clone, Default)]
pub struct Plugin {
    /// The plugin reference key. Should be limited to ASCII lowercase a-z plus underscores.
    #[validate(length(min = 1), regex(path = "RE_VALID_REFERENCE"))]
    pub reference: String,
    // TODO: plugin path should be absolute; once it's in this structure the config base path is no longer known
    // TODO: should this be a URI? That would allow e.g. data: URI values to embed WASM into config over the wire
    /// The path to the plugin WASM file.
    #[validate(length(min = 1))]
    pub path: String,
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

impl Plugin {
    /// Serializes the [`config`](Plugin::config) value to JSON bytes.
    pub fn config_to_json(&self) -> Vec<u8> {
        let obj = serde_json::Value::Object(self.config.clone());
        // TODO: probably should return a result instead of panicking
        serde_json::to_vec(&obj).unwrap()
    }
}

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
    /// The preset reference key. Should be limited to ASCII lowercase a-z plus underscores.
    #[validate(length(min = 1), regex(path = "RE_VALID_REFERENCE"))]
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

/// A mapping between a route pattern and the plugins that should be run for matching requests.
#[derive(Debug, Clone)]
pub struct Resource {
    /// The route pattern used to match requests with.
    ///
    /// Uses `matchit` router patterns.
    pub route: String,
    /// The plugin references for this route.
    pub plugins: Vec<Reference>,
    /// The maximum amount of time a plugin may take for each execution phase.
    pub timeout: Option<u64>,
}

impl Resource {
    /// Resolves all references within a `Resource`, producing a flattened list of the corresponding [`Plugin`]s.
    ///
    /// # Arguments
    ///
    /// * `config` - A [`Config`] reference to perform lookups againsts.
    ///   `Resource`s do not maintain their own references to their parent [`Config`] so this must be passed in.
    ///
    /// See [`Config::plugin`] and [`Config::preset`].
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
                Reference::Missing(ref_name) => {
                    panic!("missing reference '{}'", ref_name);
                }
            }
        }
        plugins
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
