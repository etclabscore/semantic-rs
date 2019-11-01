use std::ops::Try;
use std::path::PathBuf;

use failure::Fail;

use crate::plugin_support::command::PipedCommand;
use crate::plugin_support::flow::{FlowError, Value};
use crate::plugin_support::keys::NEXT_VERSION;
use crate::plugin_support::proto::response::{self, PluginResponse};
use crate::plugin_support::{PluginInterface, PluginStep};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Default)]
pub struct DockerPlugin {
    config: Config,
    state: Option<State>,
}

impl DockerPlugin {
    pub fn new() -> Self {
        DockerPlugin::default()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    next_version: Value<semver::Version>,
    images: Value<Vec<Image>>,
    docker_user: Value<String>,
    docker_password: Value<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            next_version: Value::required_at(NEXT_VERSION, PluginStep::Publish),
            images: Value::with_default_value("images"),
            docker_user: Value::load_from_env("DOCKER_USER"),
            docker_password: Value::load_from_env("DOCKER_PASSWORD"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Image {
    registry: Registry,
    namespace: Option<String>,
    dockerfile: PathBuf,
    name: String,
    tag: String,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Hash, Debug, Copy, Clone)]
#[serde(rename_all = "snake_case")]
enum Registry {
    Dockerhub,
}

struct State {
    credentials: Option<Credentials>,
}

struct Credentials {
    username: String,
    password: String,
}

impl PluginInterface for DockerPlugin {
    fn name(&self) -> response::Name {
        PluginResponse::from_ok("docker".into())
    }

    fn methods(&self) -> response::Methods {
        PluginResponse::from_ok(vec![PluginStep::PreFlight, PluginStep::Publish])
    }

    fn provision_capabilities(&self) -> response::ProvisionCapabilities {
        PluginResponse::from_ok(vec![])
    }

    fn get_value(&self, key: &str) -> response::GetValue {
        PluginResponse::from_error(FlowError::KeyNotSupported(key.to_owned()).into())
    }

    fn get_config(&self) -> response::Config {
        PluginResponse::from_ok(serde_json::to_value(&self.config)?)
    }

    fn reset(&mut self) -> response::Null {
        *self = Self::default();
        PluginResponse::from_ok(())
    }

    fn set_config(&mut self, config: serde_json::Value) -> response::Null {
        self.config = serde_json::from_value(config)?;
        PluginResponse::from_ok(())
    }

    fn pre_flight(&mut self) -> response::Null {
        let mut response = PluginResponse::builder();

        let credentials = {
            let username = self.config.docker_user.as_value().clone();
            let password = self.config.docker_password.as_value().clone();
            Some(Credentials { username, password })
        };

        log::info!("Checking that docker daemon is running...");
        if let Err(err) = docker_info() {
            response.error(err);
        }

        if let Some(credentials) = credentials.as_ref() {
            let registries = self
                .config
                .images
                .as_value()
                .iter()
                .map(|image| image.registry)
                .collect::<HashSet<_>>();

            for registry in registries {
                let (registry_url, registry_name) = match registry {
                    Registry::Dockerhub => (None, "DockerHub"),
                };

                if let Err(err) = login(registry_url, &credentials) {
                    response.warning(format!(
                        "login to {} failed, publishing will fail: {}",
                        registry_name, err
                    ));
                }
            }
        } else {
            response.warning("credentials are undefined, publishing will fail");
        }

        self.state.replace(State { credentials });

        response.body(())
    }

    fn publish(&mut self) -> response::Null {
        let config = &self.config;
        let state = self.state.as_ref().ok_or(Error::MissingState)?;

        let credentials = state.credentials.as_ref().ok_or(Error::CredentialsUndefined)?;

        let version = config.next_version.as_value();
        let version = format!("{}", version);

        for image in config.images.as_value() {
            let registry_url = match image.registry {
                Registry::Dockerhub => None,
            };

            login(registry_url, &credentials)?;

            build_image(image)?;

            // Tag as namespace/name/tag and namespace/name/version
            let from = format!("{}:{}", image.name, image.tag);
            tag_image(&from, &get_image_path(image, &image.tag))?;
            tag_image(&from, &get_image_path(image, &version))?;

            // Publish namespace/name/tag and namespace/name/version
            push_image(image, &image.tag)?;
            push_image(image, &version)?;
        }

        PluginResponse::from_ok(())
    }
}

fn get_image_path(image: &Image, tag: &str) -> String {
    if let Some(namespace) = image.namespace.as_ref() {
        format!("{}/{}:{}", namespace, image.name, tag)
    } else {
        format!("{}:{}", image.name, tag)
    }
}

fn docker_info() -> Result<(), failure::Error> {
    PipedCommand::new("docker", &["info"]).join(log::Level::Debug)
}

fn build_image(image: &Image) -> Result<(), failure::Error> {
    let args = &[
        "build",
        "-f",
        &image.dockerfile.display().to_string(),
        "-t",
        &format!("{}:{}", image.name, image.tag),
        ".",
    ];

    PipedCommand::new("docker", args).join(log::Level::Info)?;

    log::info!("Built image {}:{}", image.name, image.tag);

    Ok(())
}

fn tag_image(from: &str, to: &str) -> Result<(), failure::Error> {
    log::info!("tagging image {} as {}", from, to);

    PipedCommand::new("docker", &["tag", from, to]).join(log::Level::Info)
}

fn login(registry_url: Option<&str>, credentials: &Credentials) -> Result<(), failure::Error> {
    log::info!("logging in as {}", credentials.username);

    let mut args = vec!["login", "--username", &credentials.username, "--password-stdin"];

    if let Some(url) = registry_url {
        args.push(url);
    }

    PipedCommand::new("docker", &args)
        .input(&credentials.password)
        .join(log::Level::Info)
}

fn push_image(image: &Image, tag: &str) -> Result<(), failure::Error> {
    let path = get_image_path(image, tag);
    log::info!("Publishing image {}", path);
    PipedCommand::new("docker", &["push", &path]).join(log::Level::Info)
}

#[derive(Fail, Debug)]
enum Error {
    #[fail(display = "DOCKER_USER or DOCKER_PASSWORD are not set, cannot push the image.")]
    CredentialsUndefined,
    #[fail(display = "state is missing: forgot to call pre_flight?")]
    MissingState,
}
