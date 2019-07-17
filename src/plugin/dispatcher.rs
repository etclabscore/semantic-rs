use std::collections::HashMap;

use failure::Fail;

use super::{
    proto::{
        request::{self, PluginRequest},
        response::{self, PluginResponse},
        MethodName, Version,
    },
    Plugin, PluginName, PluginState, PluginStep, ResolvedPlugin,
};

use crate::config::StepDefinition::Shared;
use crate::config::{CfgMap, Config, Map};
use std::borrow::Borrow;
use std::fmt::Debug;
use std::rc::Rc;

pub struct PluginDispatcher {
    config: CfgMap,
    map: Map<PluginStep, Vec<Rc<Plugin>>>,
}

impl PluginDispatcher {
    pub fn new(config: CfgMap, map: Map<PluginStep, Vec<Rc<Plugin>>>) -> Self {
        PluginDispatcher { config, map }
    }

    fn dispatch<RFR: Debug>(
        &self,
        step: PluginStep,
        call_fn: impl Fn(&Plugin) -> PluginResponse<RFR>,
    ) -> DispatchedMultiResult<PluginResponse<RFR>> {
        let mut response_map = Map::new();

        if let Some(plugins) = self.mapped_plugins(step) {
            for plugin in plugins {
                let response = call_fn(&plugin);
                log::debug!("{}: {:?}", plugin.name(), response);
                response_map.insert(plugin.name().clone(), response);
            }
        }

        Ok(response_map)
    }

    fn dispatch_singleton<RFR: Debug>(
        &self,
        step: PluginStep,
        call_fn: impl Fn(&Plugin) -> PluginResponse<RFR>,
    ) -> DispatchedSingletonResult<PluginResponse<RFR>> {
        let plugin = self.mapped_singleton(step);
        let response = call_fn(&plugin);
        log::debug!("{}: {:?}", plugin.name(), response);
        Ok((plugin.name().to_owned(), response))
    }

    fn mapped_plugins<'a>(
        &'a self,
        step: PluginStep,
    ) -> Option<impl Iterator<Item = Rc<Plugin>> + 'a> {
        self.map.get(&step).map(|plugins| {
            plugins.iter().map(|plugin| match plugin.state() {
                PluginState::Started(_) => Rc::clone(plugin),
                _other_state => panic!(
                    "all plugins must be started before calling PluginDispatcher::mapped_plugins"
                ),
            })
        })
    }

    fn mapped_singleton(&self, step: PluginStep) -> Rc<Plugin> {
        let no_plugins_found_panic = || {
            panic!(
                "no plugins matching the singleton step {:?}: this is a bug, aborting.",
                step
            )
        };
        let too_many_plugins_panic = || {
            panic!(
                "more then one plugin matches the singleton step {:?}: this is a bug, aborting.",
                step
            )
        };

        let plugins = self.map.get(&step).unwrap_or_else(no_plugins_found_panic);

        if plugins.is_empty() {
            no_plugins_found_panic();
        }

        if plugins.len() != 1 {
            too_many_plugins_panic();
        }

        plugins[0].clone()
    }
}

pub type DispatchedMultiResult<T> = Result<Map<PluginName, T>, failure::Error>;
pub type DispatchedSingletonResult<T> = Result<(PluginName, T), failure::Error>;

impl PluginDispatcher {
    pub fn pre_flight(&self) -> DispatchedMultiResult<response::PreFlight> {
        self.dispatch(PluginStep::PreFlight, |p| {
            p.as_interface()
                .pre_flight(PluginRequest::with_default_data(self.config.clone()))
        })
    }

    pub fn get_last_release(&self) -> DispatchedSingletonResult<response::GetLastRelease> {
        self.dispatch_singleton(PluginStep::GetLastRelease, |p| {
            p.as_interface()
                .get_last_release(PluginRequest::with_default_data(self.config.clone()))
        })
    }

    pub fn derive_next_version(
        &self,
        current_version: Version,
    ) -> DispatchedMultiResult<response::DeriveNextVersion> {
        self.dispatch(PluginStep::DeriveNextVersion, |p| {
            p.as_interface().derive_next_version(PluginRequest::new(
                self.config.clone(),
                current_version.clone(),
            ))
        })
    }

    pub fn generate_notes(
        &self,
        params: request::GenerateNotesData,
    ) -> DispatchedMultiResult<response::GenerateNotes> {
        self.dispatch(PluginStep::GenerateNotes, |p| {
            p.as_interface()
                .generate_notes(PluginRequest::new(self.config.clone(), params.clone()))
        })
    }

    pub fn prepare(
        &self,
        params: request::PrepareData,
    ) -> DispatchedMultiResult<response::Prepare> {
        unimplemented!()
    }

    pub fn verify_release(
        &self,
        params: request::VerifyReleaseData,
    ) -> DispatchedMultiResult<response::VerifyRelease> {
        unimplemented!()
    }

    pub fn commit(&self, params: request::CommitData) -> DispatchedMultiResult<response::Commit> {
        unimplemented!()
    }

    pub fn publish(
        &self,
        params: request::PublishData,
    ) -> DispatchedMultiResult<response::Publish> {
        unimplemented!()
    }

    pub fn notify(&self, params: request::NotifyData) -> PluginResponse<response::Notify> {
        unimplemented!()
    }
}