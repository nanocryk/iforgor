pub mod ctrlc_handler;
mod on_disk;

pub use on_disk::OnDisk;

use {
    anyhow::{anyhow, bail},
    serde::{Deserialize, Serialize},
    sha3::{Digest, Sha3_256},
    std::{
        collections::{BTreeMap, BTreeSet},
        fs::File,
        io::Write,
        path::{Path, PathBuf},
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
    purge_all: bool,

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

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        let mut app_path = home::home_dir().ok_or(anyhow!("unable to fetch home dir"))?;
        app_path.push(".iforgor");
        let registry_path = app_path.join("registry.toml");
        let history_path = app_path.join("history.toml");

        if self.registry_path {
            println!("Registry path: {}", registry_path.display());
            return Ok(());
        }

        if self.purge_all {
            OnDisk::<Registry>::new_from_default(registry_path).save()?;
            OnDisk::<History>::new_from_default(history_path).save()?;

            println!("🗑️ Purged registry and history!");
            return Ok(());
        }

        if self.purge_history {
            OnDisk::<History>::new_from_default(history_path).save()?;

            println!("🗑️ Purged history!");
            return Ok(());
        }

        let mut registry = OnDisk::<Registry>::open_or_default(registry_path)?;
        let mut history = OnDisk::<History>::open_or_default(history_path.clone())?;

        let Some(command) = self.command else {
            loop {
                let current_dir =
                    std::env::current_dir().expect("to be able to fetch current dir path");

                let commands: Vec<_> = registry
                    .commands
                    .iter()
                    .filter(|(_, command)| filter_only_in_dir(&current_dir, command))
                    .map(|(id, command)| ichoose::ListEntry {
                        key: id.clone(),
                        name: command.name.to_string(),
                    })
                    .collect();

                let history_list: Vec<_> = history
                    .history
                    .iter()
                    .filter_map(|id| registry.commands.get(id).map(|c| (id, c)))
                    .filter(|(_, command)| filter_only_in_dir(&current_dir, command))
                    .map(|(id, c)| ichoose::ListEntry {
                        key: id.clone(),
                        name: c.name.to_string(),
                    })
                    .collect();

                let history_list: Vec<_> = history_list.into_iter().rev().collect();
                let history_list = if history_list.is_empty() {
                    None
                } else {
                    Some(history_list.as_slice())
                };

                let choices: Vec<_> = ichoose::ListSearch {
                    items: &commands,
                    extra: ichoose::ListSearchExtra {
                        empty_search_list: history_list,
                        title: " iforgor ".to_string(),
                        text: "Run `iforgor help` to learn about subcommands. \
                            Search for multiple search terms by separating them with commas `,` \
                            Empty search displays history, type anything (including spaces) to \
                            display the filtered full list of commands."
                            .to_string(),
                        ..Default::default()
                    },
                }
                .run()?
                .into_iter()
                .collect();

                if choices.is_empty() {
                    break;
                }

                if choices.len() > 1 {
                    bail!("Bug: There should be only one entry selected");
                }

                let choice = &choices[0];

                match registry.run_script_by_id(choice) {
                    Err(e) => eprintln!("Encountered an error when running command: {e}"),
                    Ok(status) => {
                        match status.code() {
                            Some(code) => {
                                print!("\n🏁 Execution complete with code {code}, press Enter to proceed.")
                            }
                            None => print!(
                                "\n🏁 Execution terminated by signal, press Enter to proceed."
                            ),
                        }
                    }
                }

                std::io::stdout().flush()?;
                let mut buf = String::new();

                // User may press Ctrl+C wanting to stop the script, but the execute finishes just before the press.
                // Let's avoid killing iforgor in that situation.
                ctrlc_handler::set_mode(ctrlc_handler::Mode::Ignore);
                std::io::stdin().read_line(&mut buf)?;
                ctrlc_handler::set_mode(ctrlc_handler::Mode::Kill);

                println!("━━━━━━━━━━━━━━━");

                // Reload history from disk in case multiple `iforgor` are running.
                history = OnDisk::<History>::open(history_path.clone()).unwrap_or(history);
                history.add_entry(choice);
                history.save()?;
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
                    println!("{}", source.display());
                }
            }
            CliCommands::Source {
                inner: SourceCommands::Remove { path },
            } => {
                // try to remove raw path, this allow to delete sources that no
                // longer exist on disk
                if !registry.sources.remove(&path) {
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
        history.save()?;

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
        // Ignore scripts incompatible with current platform.
        match script.only_on {
            Some(Platform::Windows) if !cfg!(target_os = "windows") => continue,
            Some(Platform::Linux) if !cfg!(target_os = "linux") => continue,
            _ => (),
        }

        let id = script.generate_id();
        println!("- Added command: {}", script.name);
        commands.insert(id, script);
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct History {
    pub history: Vec<CommandId>,
}

impl History {
    pub fn add_entry(&mut self, id: &CommandId) {
        let mut alt = Vec::new();
        std::mem::swap(&mut alt, &mut self.history);

        self.history = alt.into_iter().filter(|hid| hid != id).collect();
        self.history.push(id.clone());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Registry {
    pub sources: BTreeSet<PathBuf>,
    pub commands: BTreeMap<CommandId, UserCommand>,
}

impl Registry {
    pub fn run_script_by_id(&mut self, id: &CommandId) -> anyhow::Result<process::ExitStatus> {
        let Some(entry) = self.commands.get(id) else {
            bail!("Unknown command ID {id}")
        };

        let UserCommand {
            name,
            script,
            args,
            shell,
            risky,
            ..
        } = dbg!(entry);

        let mut args_values = Vec::new();
        if !args.is_empty() {
            println!(
                "This script requires the following arguments (use Ctrl+C to abort execution):"
            )
        }

        for arg in args {
            let mut buf = String::new();
            print!("- {arg}: ");
            std::io::stdout().flush()?;
            std::io::stdin().read_line(&mut buf)?;
            args_values.push(buf.trim().to_string());
        }

        if *risky {
            print!("⚠️ Script is tagged as risky, are you sure you want to run it? [y/N]: ");
            let mut buf = String::new();
            std::io::stdout().flush()?;
            std::io::stdin().read_line(&mut buf)?;

            if !["y", "yes"].contains(&buf.to_lowercase().trim()) {
                bail!("Aborted execution of risk script")
            }
        }

        println!("💭 Running \"{name}\" with shell \"{shell:?}\"\n");

        ctrlc_handler::set_mode(ctrlc_handler::Mode::Ignore);
        let status = execute_script(script, &args_values, *shell)?;
        ctrlc_handler::set_mode(ctrlc_handler::Mode::Kill);

        Ok(status)
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

    pub only_on: Option<Platform>,

    #[serde(default)]
    pub shell: Shell,

    pub only_in_dir: Option<String>,
    
    #[serde(default)]
    pub risky: bool,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum Shell {
    #[serde(alias = "sh")]
    Sh,
    #[serde(alias = "cmd")]
    Cmd,
    #[serde(alias = "powershell")]
    Powershell,
}

impl Default for Shell {
    #[cfg(target_os = "linux")]
    fn default() -> Self {
        Self::Sh
    }

    #[cfg(target_os = "windows")]
    fn default() -> Self {
        Self::Cmd
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Platform {
    #[serde(alias = "linux")]
    Linux,
    #[serde(alias = "windows")]
    Windows,
}

impl UserCommand {
    pub fn generate_id(&self) -> CommandId {
        let mut hasher = Sha3_256::new();
        hasher.update(self.script.as_bytes());
        let hash = hasher.finalize();
        base16ct::lower::encode_string(&hash)
    }
}

pub fn execute_script(
    script: &str,
    args: &[String],
    shell: Shell,
) -> anyhow::Result<process::ExitStatus> {
    match shell {
        Shell::Sh => execute_script_sh(script, args),
        Shell::Cmd => execute_script_cmd(script, args),
        Shell::Powershell => execute_script_powershell(script, args),
    }
}

pub fn execute_script_sh(script: &str, args: &[String]) -> anyhow::Result<process::ExitStatus> {
    // Create a temporary folder in which the script file will be
    // created.
    let tmp_dir = tempfile::tempdir()?;
    let file_path = tmp_dir.path().join("script");

    // Create the file, write into it and change its permissions (on Linux).
    // File is closed at the end of scope, which will allow to
    // execute it after.
    {
        let mut tmp_file = File::create(&file_path)?;
        tmp_file.write_all(b"#!/bin/sh\n")?;
        tmp_file.write_all(script.as_bytes())?;
        tmp_file.flush()?;

        #[cfg(target_os = "linux")]
        {
            // Set permissions to read/execute.
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = tmp_file.metadata()?.permissions();
            permissions.set_mode(0o500);
            tmp_file.set_permissions(permissions)?;
        }
    }

    // Execute the script
    let mut child = process::Command::new(file_path)
        .args(args)
        .spawn()
        .expect("script command failed to start");

    let status = child.wait()?;

    tmp_dir.close()?;

    Ok(status)
}

pub fn execute_script_cmd(script: &str, args: &[String]) -> anyhow::Result<process::ExitStatus> {
    // Create a temporary folder in which the script file will be
    // created.
    let tmp_dir = tempfile::tempdir()?;
    let file_path = tmp_dir.path().join("script.bat");

    // Create the file and write into it.
    // File is closed at the end of scope, which will allow to
    // execute it after.
    {
        let mut tmp_file = File::create(&file_path)?;
        tmp_file.write_all(b"@echo off\n")?;
        tmp_file.write_all(script.as_bytes())?;
        tmp_file.flush()?;
    }

    // Execute the script
    let mut child = process::Command::new(file_path)
        .args(args)
        .spawn()
        .expect("script command failed to start");

    let status = child.wait()?;

    tmp_dir.close()?;

    Ok(status)
}

pub fn execute_script_powershell(
    script: &str,
    args: &[String],
) -> anyhow::Result<process::ExitStatus> {
    // Create a temporary folder in which the script file will be
    // created.
    let tmp_dir = tempfile::tempdir()?;
    let file_path = tmp_dir.path().join("script.ps1");

    // Create the file and write into it.
    // File is closed at the end of scope, which will allow to
    // execute it after.
    {
        let mut tmp_file = File::create(&file_path)?;
        tmp_file.write_all(script.as_bytes())?;
        tmp_file.flush()?;
    }

    let powershell_cmd: Vec<_> = ["&".to_string(), format!("{}", file_path.display())]
        .into_iter()
        .chain(args.iter().map(|arg| snailquote::escape(arg).into_owned()))
        .collect();

    let powershell_cmd = powershell_cmd.join(" ");

    let args = [
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &powershell_cmd,
    ];

    println!("{args:?}");

    // Execute the script
    let mut child = process::Command::new("PowerShell")
        .args(args)
        .spawn()
        .expect("script command failed to start");

    let status = child.wait()?;

    tmp_dir.close()?;

    Ok(status)
}

fn filter_only_in_dir(current_dir: &Path, command: &UserCommand) -> bool {
    let Some(only_in_dir) = &command.only_in_dir else {
        return true;
    };

    let pattern = match glob::Pattern::new(only_in_dir) {
        Ok(p) => p,
        Err(e) => panic!(
            "Entry named \"{} has an error in its `only_in_dir` field: {}\"",
            command.name, e
        ),
    };

    pattern.matches_path(&current_dir)
}
