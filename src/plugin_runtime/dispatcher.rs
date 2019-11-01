//use crate::config::Config;
//use crate::plugin_runtime::kernel::InjectionTarget;
//use crate::plugin_runtime::Kernel;
//use crate::plugin_support::flow::{Availability, ProvisionCapability};
//use crate::plugin_support::keys::PROJECTS_PATHS;
//use crate::plugin_support::proto::ProjectsPaths;
//use crate::plugin_support::{Plugin, PluginInterface};
//use std::collections::HashSet;
//use std::path::{Path, PathBuf};
//use std::cmp;
//use walkdir::{DirEntry, WalkDir};
//
//pub struct Dispatcher {
//    absolute_root_path: PathBuf,
//    root_handlers: Vec<RootHandler>,
//}
//
//impl Dispatcher {
//    pub fn builder(path: impl AsRef<Path>) -> DispatcherBuilder {
//        DispatcherBuilder::new(path)
//    }
//
//    pub fn run(self) -> Result<(), failure::Error> {
//        for root in self.root_handlers {
//            for project in &root.projects {
//
//            }
//        }
//
//        unimplemented!();
//    }
//}
//
//pub struct DispatcherBuilder {
//    absolute_root_path: PathBuf,
//    is_dry_run: bool,
//    injections: Vec<(Plugin, InjectionTarget)>,
//}
//
//impl DispatcherBuilder {
//    pub fn new(path: impl AsRef<Path>) -> Self {
//        DispatcherBuilder {
//            absolute_root_path: path.as_ref().to_owned(),
//            is_dry_run: false,
//            injections: Vec::new(),
//        }
//    }
//
//    pub fn dry_run(mut self, is_dry_run: bool) -> Self {
//        self.is_dry_run = is_dry_run;
//        self
//    }
//
//    pub fn inject_plugin(mut self, plugin: Plugin, target: InjectionTarget) -> Self {
//        self.injections.push((plugin, target));
//        self
//    }
//
//    pub fn build(self) -> Result<Dispatcher, failure::Error> {
//        let mut roots = find_releaserc_roots(&self.absolute_root_path)?;
//        roots.sort_by(path_length_descending);
//
//        let mut root_handlers = Vec::new();
//        let mut used_roots = Vec::new();
//        for root in roots {
//            let handler = RootHandler::try_new(root.clone(), self.is_dry_run, &self.injections, |path: &Path| {
//                used_roots.iter().all(|root| !path.starts_with(root))
//            })?;
//
//            used_roots.push(root);
//            root_handlers.push(handler);
//        }
//
//        Ok(Dispatcher {
//            absolute_root_path: self.absolute_root_path,
//            root_handlers
//        })
//    }
//}
//
//struct RootHandler {
//    path: PathBuf,
//    projects: Vec<SubProject>,
//    kernel: Kernel,
//}
//
//impl RootHandler {
//    fn try_new(
//        path: PathBuf,
//        is_dry_run: bool,
//        injections: &[(Plugin, InjectionTarget)],
//        mut path_filter: impl FnMut(&Path) -> bool,
//    ) -> Result<Self, failure::Error> {
//        let config = Config::from_toml(&path.join("releases.toml"), is_dry_run)?;
//
//        let kernel = {
//            let mut builder = Kernel::builder(config);
//            for (plugin, target) in injections {
//                builder.inject_plugin(plugin.clone(), *target);
//            }
//            builder.build()?
//        };
//
//        // Collect a list of plugins capable of provisioning the project structure
//        let capable_plugins = {
//            let plugins = kernel.get_plugins();
//            let mut filtered = Vec::new();
//            for plugin in plugins {
//                // Get keys that plugin can provision
//                let caps = plugin.provision_capabilities()?;
//
//                // Iterate through capabilities to find the PROJECTS_PATHS key
//                let mut can_provision_project_structure = false;
//                for cap in caps {
//                    if cap.key == PROJECTS_PATHS {
//                        // Key must be available always
//                        if cap.when == Availability::Always {
//                            can_provision_project_structure = true;
//                        } else {
//                            log::warn!("invalid configuration of plugin {}", plugin.name);
//                            log::warn!("key {:?} must have {:?}", PROJECTS_PATHS, Availability::Always);
//                        }
//                    }
//                }
//
//                if can_provision_project_structure {
//                    filtered.push(plugin)
//                }
//            }
//            filtered
//        };
//
//        let mut project_paths = Vec::new();
//        for plugin in capable_plugins {
//            // Request the project structure from the plugin
//            let provided_project_paths = plugin.get_value(PROJECTS_PATHS)?;
//            let provided_project_paths: ProjectsPaths = serde_json::from_value(provided_project_paths)
//                .map_err(|e| failure::format_err!("plugin {} returned an invalid json: {}", plugin.name, e))?;
//
//            // Add the discovered paths
//            for path in provided_project_paths {
//                let path = PathBuf::from(path);
//                if !path.exists() {
//                    log::warn!(
//                        "plugin {} returned an invalid path '{}': not found",
//                        plugin.name,
//                        path.display()
//                    );
//                } else {
//                    project_paths.push(path);
//                }
//            }
//        }
//
//        let projects = project_paths
//            .into_iter()
//            .filter(|path| path_filter(&path))
//            .map(|path| SubProject { path })
//            .collect();
//
//        Ok(RootHandler { path, projects, kernel })
//    }
//}
//
//struct SubProject {
//    path: PathBuf,
//}
//
//fn find_releaserc_roots(path: impl AsRef<Path>) -> Result<Vec<PathBuf>, walkdir::Error> {
//    let filter_fn = |entry: DirEntry| {
//        let file_type = entry.file_type();
//        if file_type.is_dir() {
//            None
//        } else {
//            if entry.file_name() == "releaserc.toml" {
//                entry.path().parent().map(ToOwned::to_owned)
//            } else {
//                None
//            }
//        }
//    };
//
//    WalkDir::new(path)
//        .into_iter()
//        .filter_map(|entry| entry.map(filter_fn).transpose())
//        .collect()
//}
//
//fn path_length_descending(a: &PathBuf, b: &PathBuf) -> cmp::Ordering {
//    b.ancestors().count().cmp(&a.ancestors().count())
//}
//
//#[cfg(test)]
//mod tests {
//    use super::*;
//    use std::fs::{self, File};
//
//    #[test]
//    fn find_roots_simple() -> Result<(), failure::Error> {
//        let dir = tempfile::tempdir()?;
//        File::create(dir.path().join("releaserc.toml"))?;
//        let roots = find_releaserc_roots(dir.path())?;
//        assert_eq!(&roots, &[dir.path()]);
//        Ok(())
//    }
//
//    #[test]
//    fn find_roots_wrong_file_type() -> Result<(), failure::Error> {
//        let dir = tempfile::tempdir()?;
//        fs::create_dir(dir.path().join("releaserc.toml"))?;
//        let roots = find_releaserc_roots(dir.path())?;
//        assert!(roots.is_empty());
//        Ok(())
//    }
//
//    #[test]
//    fn find_roots_nested() -> Result<(), failure::Error> {
//        let dir = tempfile::tempdir()?;
//
//        let expected = &[dir.path().to_owned(), dir.path().join("one"), dir.path().join("two")];
//
//        for d in expected {
//            if !d.exists() {
//                fs::create_dir(d)?;
//            }
//            File::create(d.join("releaserc.toml"))?;
//        }
//
//        let roots = find_releaserc_roots(dir.path())?;
//        assert_eq!(&roots, &expected);
//
//        Ok(())
//    }
//
//    #[test]
//    fn find_roots_only_nested() -> Result<(), failure::Error> {
//        let dir = tempfile::tempdir()?;
//
//        let expected = &[dir.path().join("one"), dir.path().join("two")];
//
//        for d in expected {
//            fs::create_dir(d)?;
//            File::create(d.join("releaserc.toml"))?;
//        }
//
//        let roots = find_releaserc_roots(dir.path())?;
//        assert_eq!(&roots, &expected);
//
//        Ok(())
//    }
//
//    #[test]
//    fn find_roots_symlink() -> Result<(), failure::Error> {
//        let dir = tempfile::tempdir()?;
//        let orig_file_path = dir.path().join("releaserc.toml");
//        File::create(&orig_file_path)?;
//
//        let expected = &[dir.path().to_owned(), dir.path().join("one"), dir.path().join("two")];
//
//        for d in expected {
//            if !d.exists() {
//                fs::create_dir(d)?;
//            }
//            let file_path = d.join("releaserc.toml");
//            if !file_path.exists() {
//                symlink::symlink_file(&orig_file_path, &file_path)?;
//            }
//        }
//
//        let roots = find_releaserc_roots(dir.path())?;
//        assert_eq!(&roots, &expected);
//
//        Ok(())
//    }
//
//    #[test]
//    fn check_pathbuf_sorting() {
//        let mut paths = vec![
//            PathBuf::from("/tmp/dcba/xyz"),
//            PathBuf::from("/tmp/dcba"),
//            PathBuf::from("/tmp"),
//            PathBuf::from("/tmp/abcd/zyx"),
//            PathBuf::from("/tmp/abcd"),
//        ];
//
//        paths.sort_by(path_length_descending);
//
//        assert_eq!(paths, vec![
//            PathBuf::from("/tmp/dcba/xyz"),
//            PathBuf::from("/tmp/abcd/zyx"),
//            PathBuf::from("/tmp/dcba"),
//            PathBuf::from("/tmp/abcd"),
//            PathBuf::from("/tmp"),
//        ])
//    }
//}
