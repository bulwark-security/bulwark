// TODO: the awkward distinction between the config file format here and the future config serialization for the wire may be unnecessary if From traits can convert to the Toml structs instead of directly embedding them?

use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use crate::{
    ConfigFileError, DEFAULT_RESTRICT_THRESHOLD, DEFAULT_SUSPICIOUS_THRESHOLD,
    DEFAULT_TRUST_THRESHOLD,
};

#[derive(Serialize, Deserialize)]
struct Config {
    service: Option<Service>,
    thresholds: Option<Thresholds>,
    #[serde(rename(serialize = "include", deserialize = "include"))]
    includes: Option<Vec<Include>>,
    #[serde(rename(serialize = "plugin", deserialize = "plugin"))]
    plugins: Option<Vec<Plugin>>,
    #[serde(rename(serialize = "preset", deserialize = "preset"))]
    presets: Option<Vec<Preset>>,
    #[serde(rename(serialize = "resource", deserialize = "resource"))]
    resources: Option<Vec<Resource>>,
}

#[derive(Serialize, Deserialize)]
struct Service {
    // TODO: should ports be optional or just have a detault value?
    #[serde(default = "default_port")]
    port: Option<u16>,
    #[serde(default = "default_admin_port")]
    admin_port: Option<u16>,
    #[serde(default = "default_admin")]
    admin: bool,
    remote_state: Option<String>,
    proxy_hops: Option<u8>,
}

fn default_port() -> Option<u16> {
    Some(8089)
}

fn default_admin_port() -> Option<u16> {
    Some(8090)
}

fn default_admin() -> bool {
    true
}

impl From<Service> for crate::config::Service {
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

#[derive(Serialize, Deserialize)]
struct Thresholds {
    restrict: Option<f64>,
    suspicious: Option<f64>,
    trust: Option<f64>,
}

#[derive(Serialize, Deserialize)]
struct Include {
    path: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Plugin {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    reference: String,
    path: String,
    #[serde(default = "default_weight")]
    weight: f64,
    #[serde(default)]
    config: toml::map::Map<String, toml::Value>,
    #[serde(default)]
    permissions: Permissions,
}

fn default_weight() -> f64 {
    1.0
}

impl From<&Plugin> for crate::config::Plugin {
    fn from(plugin: &Plugin) -> Self {
        Self {
            reference: plugin.reference.clone(),
            path: plugin.path.clone(),
            weight: plugin.weight,
            config: plugin.config.clone(),
            permissions: plugin.permissions.clone().into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct Permissions {
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    http: Vec<String>,
    #[serde(default)]
    state: Vec<String>,
}

impl From<Permissions> for crate::config::Permissions {
    fn from(permissions: Permissions) -> Self {
        Self {
            env: permissions.env,
            http: permissions.http,
            state: permissions.state,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Preset {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    reference: String,
    plugins: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Resource {
    route: String,
    plugins: Vec<String>,
    timeout: Option<u64>,
}

pub fn load_config<'a, P>(path: &'a P) -> Result<crate::config::Config, ConfigFileError>
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
        if let Some(includes) = &root.includes {
            for include in includes {
                let include_path = base.join(&include.path);
                let include_root = load_config_recursive(&include_path)?;

                // TODO: clean this up
                if let Some(include_plugins) = include_root.plugins {
                    let root_plugins = root.plugins.unwrap_or_default();
                    let mut combined_plugins: Vec<Plugin> =
                        Vec::with_capacity(root_plugins.len() + include_plugins.len());
                    combined_plugins.extend_from_slice(root_plugins.as_slice());
                    combined_plugins.extend_from_slice(include_plugins.as_slice());

                    root.plugins = Some(combined_plugins);
                }

                if let Some(include_presets) = include_root.presets {
                    let root_presets = root.presets.unwrap_or_default();
                    let mut combined_presets: Vec<Preset> =
                        Vec::with_capacity(root_presets.len() + include_presets.len());
                    combined_presets.extend_from_slice(root_presets.as_slice());
                    combined_presets.extend_from_slice(include_presets.as_slice());

                    root.presets = Some(combined_presets);
                }

                if let Some(include_resources) = include_root.resources {
                    let root_resources = root.resources.unwrap_or_default();
                    let mut combined_resources: Vec<Resource> =
                        Vec::with_capacity(root_resources.len() + include_resources.len());
                    combined_resources.extend_from_slice(root_resources.as_slice());
                    combined_resources.extend_from_slice(include_resources.as_slice());

                    root.resources = Some(combined_resources);
                }
            }
        }

        // Strip includes once processed
        root.includes = None;

        Ok(root)
    }

    // Load the raw serialization format and resolve includes
    let root = load_config_recursive(path)?;
    let resolve_reference = |ref_name: &String| {
        let mut reference = crate::config::Reference::Missing(ref_name.clone());
        if let Some(presets) = root.presets.as_ref() {
            for preset in presets {
                if preset.reference == *ref_name {
                    reference = crate::config::Reference::Preset(ref_name.clone());
                }
            }
        }
        if let Some(plugins) = root.plugins.as_ref() {
            for plugin in plugins {
                if plugin.reference == *ref_name {
                    reference = crate::config::Reference::Plugin(ref_name.clone());
                }
            }
        }
        reference
    };
    // Transfer to the public config type, checking reference enums
    // TODO: convert this to From traits
    Ok(crate::config::Config {
        service: root.service.map(|service| service.into()),
        thresholds: root
            .thresholds
            .as_ref()
            .map(|thresholds| crate::config::Thresholds {
                restrict: thresholds.restrict.unwrap_or(DEFAULT_RESTRICT_THRESHOLD),
                suspicious: thresholds
                    .suspicious
                    .unwrap_or(DEFAULT_SUSPICIOUS_THRESHOLD),
                trust: thresholds.trust.unwrap_or(DEFAULT_TRUST_THRESHOLD),
            }),
        plugins: root
            .plugins
            .as_ref()
            .map(|plugins| plugins.iter().map(|plugin| plugin.into()).collect()),
        presets: root.presets.as_ref().map(|presets| {
            presets
                .iter()
                .map(|preset| crate::config::Preset {
                    reference: preset.reference.clone(),
                    plugins: preset.plugins.iter().map(resolve_reference).collect(),
                })
                .collect()
        }),
        resources: root.resources.as_ref().map(|resources| {
            resources
                .iter()
                .map(|resource| crate::config::Resource {
                    route: resource.route.clone(),
                    plugins: resource.plugins.iter().map(resolve_reference).collect(),
                    timeout: resource.timeout,
                })
                .collect()
        }),
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
        ref = "evil-bit"
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

        assert_eq!(root.service.as_ref().unwrap().port, Some(10002));

        assert_eq!(root.thresholds.as_ref().unwrap().restrict, Some(0.75));
        assert_eq!(root.thresholds.as_ref().unwrap().suspicious, None);
        assert_eq!(root.thresholds.as_ref().unwrap().trust, None);

        assert_eq!(root.includes.as_ref().unwrap().len(), 1);
        assert_eq!(
            root.includes.as_ref().unwrap().get(0).unwrap().path,
            "default.toml"
        );

        assert_eq!(root.plugins.as_ref().unwrap().len(), 1);
        assert_eq!(
            root.plugins.as_ref().unwrap().get(0).unwrap().reference,
            "evil-bit"
        );
        assert_eq!(
            root.plugins.as_ref().unwrap().get(0).unwrap().path,
            "bulwark-evil-bit.wasm"
        );
        assert_eq!(
            root.plugins.as_ref().unwrap().get(0).unwrap().config,
            toml::map::Map::default()
        );

        assert_eq!(root.presets.as_ref().unwrap().len(), 1);
        assert_eq!(
            root.presets.as_ref().unwrap().get(0).unwrap().reference,
            "custom"
        );
        assert_eq!(
            root.presets.as_ref().unwrap().get(0).unwrap().plugins,
            vec!["evil-bit"]
        );

        assert_eq!(root.resources.as_ref().unwrap().len(), 1);
        assert_eq!(root.resources.as_ref().unwrap().get(0).unwrap().route, "/");
        assert_eq!(
            root.resources.as_ref().unwrap().get(0).unwrap().plugins,
            vec!["custom"]
        );
        assert_eq!(
            root.resources.as_ref().unwrap().get(0).unwrap().timeout,
            Some(25)
        );

        Ok(())
    }

    #[test]
    fn test_load_config() -> Result<(), Box<dyn std::error::Error>> {
        let root: crate::config::Config = load_config("tests/main.toml")?;

        assert_eq!(root.service.as_ref().unwrap().port, Some(10002));

        assert_eq!(root.thresholds.as_ref().unwrap().restrict, 0.75); // non-default
        assert_eq!(root.thresholds.as_ref().unwrap().suspicious, 0.6); // default
        assert_eq!(root.thresholds.as_ref().unwrap().trust, 0.2); // default

        assert_eq!(root.plugins.as_ref().unwrap().len(), 2);
        assert_eq!(
            root.plugins.as_ref().unwrap().get(0).unwrap().reference,
            "evil-bit"
        );
        assert_eq!(
            root.plugins.as_ref().unwrap().get(0).unwrap().path,
            "bulwark-evil-bit.wasm"
        );
        assert_eq!(
            root.plugins.as_ref().unwrap().get(0).unwrap().config,
            toml::map::Map::default()
        );

        assert_eq!(root.presets.as_ref().unwrap().len(), 2);
        assert_eq!(
            root.presets.as_ref().unwrap().get(0).unwrap().reference,
            "default"
        );
        assert_eq!(
            root.presets.as_ref().unwrap().get(1).unwrap().reference,
            "starter-preset"
        );
        assert_eq!(
            root.presets.as_ref().unwrap().get(0).unwrap().plugins,
            vec![
                crate::config::Reference::Plugin("evil-bit".to_string()),
                crate::config::Reference::Preset("starter-preset".to_string())
            ]
        );
        assert_eq!(
            root.presets.as_ref().unwrap().get(1).unwrap().plugins,
            vec![crate::config::Reference::Plugin("blank-slate".to_string())]
        );

        assert_eq!(root.resources.as_ref().unwrap().len(), 2);
        assert_eq!(root.resources.as_ref().unwrap().get(0).unwrap().route, "/");
        assert_eq!(
            root.resources.as_ref().unwrap().get(1).unwrap().route,
            "/*params"
        );
        assert_eq!(
            root.resources.as_ref().unwrap().get(0).unwrap().plugins,
            vec![crate::config::Reference::Preset("default".to_string())]
        );
        assert_eq!(
            root.resources.as_ref().unwrap().get(0).unwrap().timeout,
            Some(25)
        );

        Ok(())
    }
}
