use failure::Fail;
use strum::IntoEnumIterator;

use crate::config::{Config, Map, PluginDefinitionMap};
use crate::logger;
use crate::plugin_runtime::data_mgr::DataManager;
use crate::plugin_runtime::graph::{ActionKind, PluginSequence};
use crate::plugin_runtime::resolver::PluginResolver;
use crate::plugin_runtime::starter::PluginStarter;
use crate::plugin_support::flow::Value;
use crate::plugin_support::{Plugin, PluginInterface, PluginStep, RawPlugin, RawPluginState};
use std::collections::HashMap;

pub type PluginId = usize;

pub struct Kernel {
    plugins: Vec<Plugin>,
    data_mgr: DataManager,
    sequence: PluginSequence,
    env: HashMap<String, String>,
    is_dry_run: bool,
}

impl Kernel {
    pub fn builder(config: Config) -> KernelBuilder {
        KernelBuilder::new(config)
    }

    pub fn run(mut self) -> Result<(), failure::Error> {
        for action in self.sequence.into_iter() {
            log::trace!("running action {:?}", action);
            let id = action.id();
            match action.into_kind() {
                ActionKind::Call(step) => {
                    let plugin = &self.plugins[id];
                    log::debug!("call {}::{}", plugin.name, step.as_str());
                    let _span = logger::span(&plugin.name);
                    let mut callable = plugin.as_interface();
                    match step {
                        PluginStep::PreFlight => callable.pre_flight()?,
                        PluginStep::GetLastRelease => callable.get_last_release()?,
                        PluginStep::DeriveNextVersion => callable.derive_next_version()?,
                        PluginStep::GenerateNotes => callable.generate_notes()?,
                        PluginStep::Prepare => callable.prepare()?,
                        PluginStep::VerifyRelease => callable.verify_release()?,
                        PluginStep::Commit => callable.commit()?,
                        PluginStep::Publish => callable.publish()?,
                        PluginStep::Notify => callable.notify()?,
                    }
                }
                ActionKind::Get(src_key) => {
                    let plugin = &self.plugins[id];
                    let span = logger::span(&plugin.name);
                    let value = plugin.as_interface().get_value(&src_key)?;
                    drop(span);
                    log::debug!("get {}::{} ==> {:?}", self.plugins[id].name, src_key, value);
                    let value = Value::builder(&src_key).value(value).build();
                    self.data_mgr.insert_global(src_key, value);
                }
                ActionKind::Set(dst_key, src_key) => {
                    let value = self.data_mgr.prepare_value(id, &dst_key, &src_key)?;
                    log::debug!("set {}::{} <== {:?}", self.plugins[id].name, dst_key, value);
                    let plugin = &self.plugins[id];
                    let _span = logger::span(&plugin.name);
                    plugin.as_interface().set_value(&dst_key, value)?;
                }
                ActionKind::SetValue(dst_key, value) => {
                    let value = Value::builder(&dst_key).value(value).build();
                    log::debug!("set {}::{} <== {:?}", self.plugins[id].name, dst_key, value);
                    let plugin = &self.plugins[id];
                    let _span = logger::span(&plugin.name);
                    self.plugins[id].as_interface().set_value(&dst_key, value)?;
                }
                ActionKind::RequireConfigEntry(dst_key) => {
                    let value = self.data_mgr.prepare_value_same_key(id, &dst_key)?;
                    log::debug!("set {}::{} <== {:?}", self.plugins[id].name, dst_key, value);
                    let plugin = &self.plugins[id];
                    let _span = logger::span(&plugin.name);
                    self.plugins[id].as_interface().set_value(&dst_key, value)?;
                }
                ActionKind::RequireEnvValue(dst_key, src_key) => {
                    let value = self
                        .env
                        .get(&src_key)
                        .ok_or_else(|| Error::EnvValueUndefined(src_key.clone()))?;
                    let value = Value::builder(&src_key).value(serde_json::to_value(value)?).build();
                    log::debug!("set {}::{} <== {:?}", self.plugins[id].name, dst_key, value);
                    let plugin = &self.plugins[id];
                    let _span = logger::span(&plugin.name);
                    self.plugins[id].as_interface().set_value(&dst_key, value)?;
                }
            }
        }

        if self.is_dry_run {
            log::info!(
                "DRY RUN: skipping steps {:?}",
                PluginStep::iter().filter(|s| !s.is_dry()).collect::<Vec<_>>()
            );
        }

        Ok(())
    }
}

#[allow(dead_code)]
pub enum InjectionTarget {
    BeforeStep(PluginStep),
    AfterStep(PluginStep),
}

pub struct KernelBuilder {
    config: Config,
    injections: Vec<(Box<dyn PluginInterface>, InjectionTarget)>,
}

impl KernelBuilder {
    pub fn new(config: Config) -> Self {
        KernelBuilder {
            config,
            injections: Vec::new(),
        }
    }

    pub fn inject_plugin<P: PluginInterface + 'static>(&mut self, plugin: P, target: InjectionTarget) -> &mut Self {
        let plugin = Box::new(plugin);
        self.injections.push((plugin, target));
        self
    }

    pub fn build(&mut self) -> Result<Kernel, failure::Error> {
        // Convert KeyValueDefinitionMap into KeyValue<JsonValue> map
        let cfg = self.config.cfg.clone();
        let cfg: Map<String, Value<serde_json::Value>> = cfg.into();
        let is_dry_run = cfg
            .get("dry_run")
            .and_then(|kv| kv.as_value().as_bool())
            .unwrap_or(true);

        // Move PluginDefinitions out of config and convert them to Plugins
        let plugins = self.config.plugins.clone();
        let plugins = Self::plugin_def_map_to_vec(plugins);

        // Resolve stage
        let plugins = Self::resolve_plugins(plugins)?;
        Self::check_all_resolved(&plugins)?;
        log::debug!("all plugins resolved");

        // Starting stage
        let plugins = Self::start_plugins(plugins)?;
        log::debug!("all plugins started");

        // Injection stage
        let injections = std::mem::replace(&mut self.injections, Vec::new());
        let mut injection_defs = Vec::new();
        let mut injected_plugins = Vec::new();
        for (id, (plugin, target)) in injections.into_iter().enumerate() {
            let plugin = Plugin::new(plugin)?;
            injected_plugins.push(plugin);
            injection_defs.push((id, target));
        }

        // Prepend injected plugins to plugin list
        injected_plugins.extend(plugins.into_iter());
        let plugins = injected_plugins;

        // Calculate the plugin run sequence
        let sequence = PluginSequence::new(&plugins, &self.config, injection_defs, is_dry_run)?;
        log::debug!("plugin Sequence Graph built successfully");
        log::trace!("graph: {:#?}", sequence);

        // Create data manager
        let data_mgr = DataManager::new(&self.config);

        Ok(Kernel {
            env: std::env::vars().collect(),
            plugins,
            data_mgr,
            sequence,
            is_dry_run,
        })
    }

    fn plugin_def_map_to_vec(plugins: PluginDefinitionMap) -> Vec<RawPlugin> {
        plugins
            .into_iter()
            .map(|(name, def)| RawPlugin::new(name, RawPluginState::Unresolved(def.into_full())))
            .collect()
    }

    fn resolve_plugins(plugins: Vec<RawPlugin>) -> Result<Vec<RawPlugin>, failure::Error> {
        log::debug!("resolving plugins...");
        let resolver = PluginResolver::new();
        let plugins = plugins
            .into_iter()
            .map(|p| resolver.resolve(p))
            .collect::<Result<_, _>>()?;
        Ok(plugins)
    }

    fn start_plugins(plugins: Vec<RawPlugin>) -> Result<Vec<Plugin>, failure::Error> {
        log::debug!("starting plugins...");
        let starter = PluginStarter::new();
        let plugins = plugins
            .into_iter()
            .map(|p| starter.start(p))
            .collect::<Result<_, _>>()?;
        Ok(plugins)
    }

    fn check_all_resolved(plugins: &[RawPlugin]) -> Result<(), failure::Error> {
        let unresolved = Self::list_not_resolved_plugins(plugins);
        if unresolved.is_empty() {
            Ok(())
        } else {
            Err(Error::FailedToResolvePlugins(unresolved).into())
        }
    }

    fn list_not_resolved_plugins(plugins: &[RawPlugin]) -> Vec<String> {
        Self::list_all_plugins_that(plugins, |plugin| match plugin.state() {
            RawPluginState::Unresolved(_) => true,
            RawPluginState::Resolved(_) => false,
        })
    }

    fn list_all_plugins_that(plugins: &[RawPlugin], filter: impl Fn(&RawPlugin) -> bool) -> Vec<String> {
        plugins
            .iter()
            .filter_map(|plugin| {
                if filter(plugin) {
                    Some(plugin.name().clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "failed to resolve some modules: \n{:#?}", _0)]
    FailedToResolvePlugins(Vec<String>),
    #[fail(display = "environment value must be set: {}", _0)]
    EnvValueUndefined(String),
}
