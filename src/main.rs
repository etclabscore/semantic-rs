#![feature(try_trait)]

mod builtin_plugins;
mod config;
mod kernel;
mod plugin;
mod utils;

use crate::config::Config;
use crate::kernel::Kernel;
use std::env;

fn main() -> Result<(), failure::Error> {
    init_logger();
    dotenv::dotenv()?;

    log::info!("semantic.rs 🚀");

    let config = Config::from_toml("./releaserc.toml")?;
    let kernel = Kernel::builder(config).build()?;

    if let Err(err) = kernel.run() {
        log::error!("{}", err);
        std::process::exit(1);
    }

    Ok(())
}

fn init_logger() {
    use std::io::Write;

    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }

    env_logger::Builder::from_default_env()
        .format(|fmt, record| match record.level() {
            log::Level::Info => writeln!(fmt, "{}", record.args()),
            log::Level::Warn => writeln!(fmt, ">> {}", record.args()),
            log::Level::Error => writeln!(fmt, "!! {}", record.args()),
            log::Level::Debug => writeln!(fmt, "DD {}", record.args()),
            log::Level::Trace => writeln!(fmt, "TT {}", record.args()),
        })
        .init();
}
