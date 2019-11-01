use crate::plugin_support::{Plugin, PluginInterface, PluginStep};

pub fn discover(plugin: &Plugin) -> Result<Vec<PluginStep>, failure::Error> {
    let response = plugin.methods()?;
    Ok(response)
}
