mod on_disk;

pub use on_disk::OnDisk;

use anyhow::{anyhow, bail};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{self};

type CommandId = String;

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(about = "The CLI tool for all those commands you forget about")]
pub struct Cli {
    /// Cleanup config file, which will remove all registered sources and commands.
    /// Use it in case of file corruption or change in format after an update.
    #[arg(long)]
    cleanup_registry: bool,

    #[arg(long)]
    registry_path: bool,

    #[command(subcommand)]
    command: CliCommands,
}

#[derive(clap::Subcommand, Debug)]
pub enum CliCommands {
    /// (DEBUG) Run command with given ID
    RunId { id: String },
    /// [short: s] Run command based on name search.
    /// Provides a list of commands matching the search, which you can then choose amongst
    /// using the provided number.
    #[command(alias = "s")]
    Search { search: Vec<String> },
    /// [short: h] List last run commands and allow to rerun them. 
    #[command(alias = "h")]
    History { 
        #[arg(long)]
        purge: bool
    },
    /// Add a source
    AddSource { path: PathBuf },
    /// [short: r] Reload commands from sources.
    #[command(alias = "r")]
    Reload,
}

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        let mut app_path = home::home_dir().ok_or(anyhow!("unable to fetch home dir"))?;
        app_path.push(".iforgor");
        let registry_path = app_path.join("registry.toml");

        if self.registry_path {
            println!("Registry path: {}", registry_path.display());
        }

        let mut registry = if self.cleanup_registry {
            println!("Cleaning config file ({})", registry_path.display());
            let registry = OnDisk::<CommandsRegistry>::new_from_default(registry_path);
            registry.save()?;
            registry
        } else {
            OnDisk::<CommandsRegistry>::open_or_default(registry_path)?
        };

        let command = self.command;

        match command {
            CliCommands::RunId { id } => {
                registry.run_script_by_id(&id)?;
            }
            CliCommands::Search { search } => {
                let search: Vec<_> = search.into_iter().map(|s| s.to_lowercase()).collect();

                let commands: Vec<(CommandId, String)> = registry
                    .commands
                    .iter()
                    .filter_map(|(id, command)| {
                        if search_filter(&command, &search) {
                            Some((id.clone(), command.name().to_string()))
                        } else {
                            None
                        }
                    })
                    .take(10)
                    .collect();

                let Some(id) = choose_amongst_list(&commands)? else {
                    return Ok(());
                };

                registry.run_script_by_id(&id)?;
            }
            CliCommands::AddSource { path } => {
                let path = std::fs::canonicalize(path)?;
                println!("Adding source \"{}\"", path.display());

                load_scripts_for_source(&mut registry.commands, path.clone())?;

                registry.sources.insert(path);
            }
            CliCommands::History { purge } => {
                if purge {
                    registry.history = Vec::new();
                    registry.save()?;
                    println!("Purged history!");
                    return Ok(())
                }

                let history: Vec<_> = registry
                    .history
                    .iter()
                    .filter_map(|id| registry.commands.get(id).map(|c| (id, c)))
                    .map(|(id, c)| (id.clone(), c.name().to_owned()))
                    .collect();

                let history: Vec<_> = history.into_iter().rev().collect();

                let Some(id) = choose_amongst_list(&history)? else {
                    return Ok(());
                };

                registry.run_script_by_id(&id)?;
            }
            CliCommands::Reload => {
                let mut commands = BTreeMap::new();

                for path in &registry.sources {
                    load_scripts_for_source(&mut commands, path.clone())?;
                }

                registry.commands = commands;
            }
        }

        registry.save()?;

        Ok(())
    }
}

fn choose_amongst_list(list: &[(CommandId, String)]) -> anyhow::Result<Option<CommandId>> {
    if list.is_empty() {
        bail!("Result list is empty");
    }

    for (i, (_id, name)) in list.iter().enumerate() {
        println!("{i}. {}", name);
    }

    print!("Which script do you want to run (0-9, a/q to abort): ");
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;

    let line = line.trim();
    if line == "a" || line == "q" {
        return Ok(None);
    }

    let choice: u8 = line.parse()?;

    let Some((id, _name)) = list.get(choice as usize) else {
        bail!("No script with this index");
    };

    Ok(Some(id.clone()))
}

fn load_scripts_for_source(
    commands: &mut BTreeMap<CommandId, Command>,
    path: PathBuf,
) -> anyhow::Result<()> {
    println!("Loading source: {}", path.display());
    let scripts = OnDisk::<CommandsSource>::open(path.clone())?.into_inner();

    for script in scripts.entries {
        let id = script.generate_id();
        println!("- Added command: {}", script.name);
        commands.insert(id, Command::UserCommand(script));
    }

    Ok(())
}

fn search_filter(command: &Command, search: &[String]) -> bool {
    let command_name_lower = command.name().to_lowercase();
    for word in search {
        if !command_name_lower.contains(word) {
            return false;
        }
    }

    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandsRegistry {
    pub history: Vec<CommandId>,
    pub sources: BTreeSet<PathBuf>,
    pub commands: BTreeMap<CommandId, Command>,
}

impl CommandsRegistry {
    pub fn run_script_by_id(&mut self, id: &CommandId) -> anyhow::Result<()> {
        let Some(entry) = self.commands.get(id) else {
            bail!("Unknown command ID {id}")
        };

        // Update history before running the script in case it fails.
        let mut history = Vec::new();
        std::mem::swap(&mut self.history, &mut history);

        self.history = history
            .into_iter()
            .filter(|hid| hid != id)
            .collect();
        self.history.push(id.clone());

        match entry {
            Command::UserCommand(UserCommand { name, script }) => {
                println!("Running command {name}\n");
                execute_script(&script)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandsSource {
    pub entries: Vec<UserCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    // SystemCommand(SystemCommand),
    UserCommand(UserCommand),
}

impl Command {
    pub fn name(&self) -> &str {
        match self {
            Self::UserCommand(command) => &command.name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemCommand {
    RefreshFromSources,
    AddSource,
    RemoveSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCommand {
    pub name: String,
    pub script: String,
}

impl UserCommand {
    pub fn generate_id(&self) -> CommandId {
        let mut hasher = Sha3_256::new();
        hasher.update(self.script.as_bytes());
        let hash = hasher.finalize();
        base16ct::lower::encode_string(&hash)
    }
}

pub fn execute_script(script: &str) -> anyhow::Result<()> {
    // Create a temporary folder in which the script file will be
    // created.
    let tmp_dir = tempfile::tempdir()?;
    let file_path = tmp_dir.path().join("script");

    // Create the file, write into it and change its permissions.
    // File is closed at the end of scope, which will allow to
    // execute it after.
    {
        let mut tmp_file = File::create(&file_path)?;
        tmp_file.write_all(b"#!/bin/sh\n")?;
        tmp_file.write_all(script.as_bytes())?;
        tmp_file.flush()?;

        // Set permissions to read/execute.
        let mut permissions = tmp_file.metadata()?.permissions();
        permissions.set_mode(0o500);
        tmp_file.set_permissions(permissions)?;
    }

    // Execute the script
    let mut child = process::Command::new(file_path)
        .spawn()
        .expect("script command failed to start");

    child.wait()?;

    tmp_dir.close()?;

    Ok(())
}
