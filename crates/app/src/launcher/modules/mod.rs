//! Built-in launcher modules. Adding one means a directory here and a line in
//! [`registry`].

pub mod apps;
pub mod calculator;
pub mod clipboard;
pub mod config;
pub mod help;
pub mod mpris;
pub mod services;
pub mod shell;
pub mod sysinfo;
pub mod theme;
pub mod wallpaper;
pub mod web;
pub mod workspaces;

use super::config::ModulesConfig;
use super::page::ModuleDescriptor;

/// Every module, in help and footer order. Help comes last so it can list the
/// rest.
pub fn registry(config: &ModulesConfig) -> Vec<ModuleDescriptor> {
    let mut modules = vec![
        apps::descriptor(&config.apps),
        shell::descriptor(&config.shell),
        web::descriptor(&config.web),
        workspaces::descriptor(&config.workspaces),
        wallpaper::descriptor(&config.wallpaper),
        theme::descriptor(&config.themes),
        services::descriptor(&config.services),
        sysinfo::descriptor(&config.sysinfo),
        mpris::descriptor(&config.mpris),
        calculator::descriptor(&config.calculator),
        clipboard::descriptor(&config.clipboard),
        config::descriptor(&config.config),
    ];
    let listed = modules.iter().map(|module| module.meta.clone()).collect();
    modules.push(help::descriptor(&config.help, listed));
    modules
}
