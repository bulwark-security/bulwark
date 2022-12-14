use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use crate::ConfigFileError;

#[derive(Serialize, Deserialize)]
struct Root {
    config: Option<Config>,
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
struct Config {
    port: Option<u16>,
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
    config: toml::map::Map<String, toml::Value>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Preset {
    #[serde(rename(serialize = "ref", deserialize = "ref"))]
    reference: String,
    plugins: Vec<String>,
    timeout: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Resource {
    prefix: String,
    plugins: Vec<String>,
    timeout: Option<u64>,
}

enum Reference {
    Plugin(String),
    Preset(String),
}

fn load_config<'a, P>(path: &'a P) -> Result<Root, ConfigFileError>
where
    P: 'a + ?Sized + AsRef<Path>,
{
    let toml_data = fs::read_to_string(path)?;
    let mut root: Root = toml::from_str(&toml_data)?;
    // TODO: avoid unwrap
    let base = path.as_ref().parent().unwrap();

    // TODO: error on circular includes
    if let Some(includes) = &root.includes {
        for include in includes {
            let include_path = base.join(&include.path);
            let mut include_root = load_config(&include_path)?;

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
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize() -> Result<(), Box<dyn std::error::Error>> {
        let root: Root = toml::from_str(
            r#"
        [config]
        port = 10002
        
        [[include]]
        path = "default.toml"
        
        [[plugin]]
        ref = "evil-bit"
        path = "bulwark-evil-bit.wasm"
        config = {}
        
        [[preset]]
        ref = "custom"
        plugins = ["evil-bit"]
        timeout = 25
        
        [[resource]]
        prefix = "/"
        plugins = ["custom"]
        timeout = 25
    "#,
        )?;

        assert_eq!(root.config.as_ref().unwrap().port, Some(10002));

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
            toml::map::Map::new()
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
        assert_eq!(
            root.presets.as_ref().unwrap().get(0).unwrap().timeout,
            Some(25)
        );

        assert_eq!(root.resources.as_ref().unwrap().len(), 1);
        assert_eq!(root.resources.as_ref().unwrap().get(0).unwrap().prefix, "/");
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
        let root: Root = load_config("test/main.toml")?;

        assert_eq!(root.config.as_ref().unwrap().port, Some(10002));

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
            toml::map::Map::new()
        );

        assert_eq!(root.presets.as_ref().unwrap().len(), 1);
        assert_eq!(
            root.presets.as_ref().unwrap().get(0).unwrap().reference,
            "starter-preset"
        );
        assert_eq!(
            root.presets.as_ref().unwrap().get(0).unwrap().plugins,
            vec!["blank-slate"]
        );
        assert_eq!(
            root.presets.as_ref().unwrap().get(0).unwrap().timeout,
            Some(100)
        );

        assert_eq!(root.resources.as_ref().unwrap().len(), 1);
        assert_eq!(root.resources.as_ref().unwrap().get(0).unwrap().prefix, "/");
        assert_eq!(
            root.resources.as_ref().unwrap().get(0).unwrap().plugins,
            vec!["evil-bit", "starter-preset"]
        );
        assert_eq!(
            root.resources.as_ref().unwrap().get(0).unwrap().timeout,
            Some(25)
        );

        Ok(())
    }
}
