#![feature(try_trait, external_doc)]
#![doc(include = "../README.md")]

#[macro_use]
extern crate strum_macros;
#[macro_use]
extern crate pest_derive;

pub mod builtin_plugins;
mod config;
mod logger;
mod plugin_runtime;
mod plugin_support;
mod utils;

use crate::builtin_plugins::{early_exit, EarlyExitPlugin};
use crate::config::Config;
use crate::plugin_runtime::kernel::InjectionTarget;
use crate::plugin_support::{Plugin, PluginStep};
use plugin_runtime::Kernel;
use std::env;

fn main() {
    if let Err(err) = run() {
        eprintln!("!! Error: {}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<(), failure::Error> {
    dotenv::dotenv().ok();

    let clap_args = clap::App::new("semantic-rs")
        .version(clap::crate_version!())
        .author(clap::crate_authors!())
        .about(clap::crate_description!())
        .arg(
            clap::Arg::with_name("dry")
                .long("dry")
                .help("Execute semantic-rs in dry-run more (no writes or publishes"),
        )
        .arg(
            clap::Arg::with_name("verbose")
                .short("v")
                .multiple(true)
                .help("Verbosity level (-v, -vv, -vvv, ...)"),
        )
        .arg(clap::Arg::with_name("silent").long("silent").help("Disable all logs"))
        .get_matches();

    logger::init_logger(clap_args.occurrences_of("verbose"), clap_args.is_present("silent"))?;

    log::info!("semantic.rs 🚀");

    let is_dry_run = clap_args.is_present("dry");

    let config = Config::from_toml("./releaserc.toml", is_dry_run)?;

    let kernel = Kernel::builder(config)
        .inject_plugin(
            Plugin::new(EarlyExitPlugin::new())?,
            InjectionTarget::AfterStep(PluginStep::DeriveNextVersion),
        )
        .build()?;

    if let Err(err) = kernel.run() {
        macro_rules! log_error_and_die {
            ($err:expr) => {{
                log::error!("{}", $err);
                std::process::exit(1);
            }};
        }

        match err.downcast::<early_exit::Error>() {
            Ok(ee_error) => match ee_error {
                early_exit::Error::EarlyExit(_) => (),
            },
            Err(other_error) => {
                log_error_and_die!(other_error);
            }
        }
    }

    Ok(())
}
