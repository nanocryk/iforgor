mod on_disk;
mod tui;

pub use on_disk::OnDisk;

use {
    anyhow::{anyhow, bail},
    serde::{Deserialize, Serialize},
    sha3::{Digest, Sha3_256},
    std::{
        collections::{BTreeMap, BTreeSet},
        fmt::Display,
        fs::File,
        io::Write,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        process::{self},
    },
};

type CommandId = String;

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(about = "The CLI tool for all those commands you forget about")]
pub struct Cli {
    /// Cleanup config file, which will remove all registered sources and commands.
    /// Use it in case of file corruption or change in format after an update.
    #[arg(long)]
    cleanup_registry: bool,

    /// Cleanup the history of ran commands.
    #[arg(long)]
    purge_history: bool,

    /// Display the registry path.
    #[arg(long)]
    registry_path: bool,

    #[command(subcommand)]
    command: Option<CliCommands>,
}

#[derive(clap::Subcommand, Debug)]
pub enum CliCommands {
    /// Source subcommands
    Source {
        #[command(subcommand)]
        inner: SourceCommands,
    },
    /// Reload commands from sources.
    Reload,
}

#[derive(clap::Subcommand, Debug)]
pub enum SourceCommands {
    /// Add a source
    Add { path: PathBuf },
    /// List all sources
    List,
    /// Remove a source
    Remove { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct IdAndName {
    // order by name first
    pub name: String,
    pub id: CommandId,
}

impl Display for IdAndName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.name)
    }
}

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        let mut app_path = home::home_dir().ok_or(anyhow!("unable to fetch home dir"))?;
        app_path.push(".iforgor");
        let registry_path = app_path.join("registry.toml");

        if self.registry_path {
            println!("Registry path: {}", registry_path.display());
            return Ok(());
        }

        let mut registry = if self.cleanup_registry {
            println!("Cleaning config file ({})", registry_path.display());
            let registry = OnDisk::<CommandsRegistry>::new_from_default(registry_path);
            registry.save()?;
            registry
        } else {
            OnDisk::<CommandsRegistry>::open_or_default(registry_path)?
        };

        if self.purge_history {
            registry.history = Vec::new();
            registry.save()?;
            println!("🗑️ Purged history!");
            return Ok(());
        }

        let Some(command) = self.command else {
            loop {
                let commands: Vec<_> = registry
                    .commands
                    .iter()
                    .map(|(id, command)| IdAndName {
                        id: id.clone(),
                        name: command.name.to_string(),
                    })
                    .collect();

                let history: Vec<_> = registry
                    .history
                    .iter()
                    .filter_map(|id| registry.commands.get(id).map(|c| (id, c)))
                    .map(|(id, c)| IdAndName {
                        id: id.clone(),
                        name: c.name.to_string(),
                    })
                    .collect();

                let history: Vec<_> = history.into_iter().rev().collect();

                let Some(choice) = tui::tui_choose_in_list(&commands, &history)? else {
                    break;
                };

                registry.run_script_by_id(&choice.id)?;
                registry.save()?;

                print!("\n🏁 Execution complete, press Enter to proceed.");
                std::io::stdout().flush()?;
                let mut buf = String::new();
                std::io::stdin().read_line(&mut buf)?;

                println!("━━━━━━━━━━━━━━━");
            }

            return Ok(());
        };

        match command {
            CliCommands::Source {
                inner: SourceCommands::Add { path },
            } => {
                let path = std::fs::canonicalize(path)?;
                println!("Adding source \"{}\"", path.display());

                load_scripts_for_source(&mut registry.commands, path.clone())?;

                registry.sources.insert(path);
            }
            CliCommands::Source {
                inner: SourceCommands::List,
            } => {
                for source in &registry.sources {
                    println!("- {}", source.display());
                }
            }
            CliCommands::Source {
                inner: SourceCommands::Remove { path },
            } => {
                let path = std::fs::canonicalize(path)?;

                if !registry.sources.remove(&path) {
                    bail!("Path was not a registered source");
                }

                println!("Removed source \"{}\"", path.display());
                println!(
                    "Commands in that source are still registred. Run \
                `iforgor reload` to reload commands from remaining sources only"
                );
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

fn load_scripts_for_source(
    commands: &mut BTreeMap<CommandId, UserCommand>,
    path: PathBuf,
) -> anyhow::Result<()> {
    println!("Loading source: {}", path.display());
    let scripts = OnDisk::<CommandsSource>::open(path.clone())?.into_inner();

    for script in scripts.entries {
        let id = script.generate_id();
        println!("- Added command: {}", script.name);
        commands.insert(id, script);
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandsRegistry {
    pub history: Vec<CommandId>,
    pub sources: BTreeSet<PathBuf>,
    pub commands: BTreeMap<CommandId, UserCommand>,
}

impl CommandsRegistry {
    pub fn run_script_by_id(&mut self, id: &CommandId) -> anyhow::Result<()> {
        let Some(entry) = self.commands.get(id) else {
            bail!("Unknown command ID {id}")
        };

        // Update history before running the script in case it fails.
        let mut history = Vec::new();
        std::mem::swap(&mut self.history, &mut history);

        self.history = history.into_iter().filter(|hid| hid != id).collect();
        self.history.push(id.clone());

        let UserCommand { name, script, args } = entry;

        let mut args_values = Vec::new();
        if !args.is_empty() {
            println!("This script requires the following arguments:")
        }
        for arg in args {
            let mut buf = String::new();
            print!("- {arg}: ");
            std::io::stdout().flush()?;
            std::io::stdin().read_line(&mut buf)?;
            args_values.push(buf);
        }

        println!("💭 Running \"{name}\"\n");

        execute_script(script, &args_values)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandsSource {
    pub entries: Vec<UserCommand>,
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
    #[serde(default)]
    pub args: Vec<String>,
}

impl UserCommand {
    pub fn generate_id(&self) -> CommandId {
        let mut hasher = Sha3_256::new();
        hasher.update(self.script.as_bytes());
        let hash = hasher.finalize();
        base16ct::lower::encode_string(&hash)
    }
}

pub fn execute_script(script: &str, args: &[String]) -> anyhow::Result<()> {
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
        .args(args)
        .spawn()
        .expect("script command failed to start");

    child.wait()?;

    tmp_dir.close()?;

    Ok(())
}
