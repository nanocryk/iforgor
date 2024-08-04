# iforgor

[![iforgor crate](https://img.shields.io/crates/v/iforgor?label=iforgor)](https://crates.io/crates/iforgor)

The CLI tool for all those commands you forget about

## Installation

Run `cargo install iforgor`.

## Configuration

Add script source files using `iforgor source add <PATH>` (see [exemple](exemple.toml)).
Each entry follow the following format:

```toml
[[entries]]
name = "WRITE NAME HERE"
script = "WRITE SCRIPT HERE"
```

Entry can also contain the following optional fields:
- `only_on = "OS"`: script will only be loaded on provided OS. Accepts `Linux` and `Windows`.
- `args = ["Arg 1", "Arg 2"]`: list arguments labels that will be printed when calling.
- `shell = "SHELL`: selects the shell to execute the script with. Supports `Sh` (default for Linux),
  `Cmd` (default for Windows) and `Powershell`.
- `only_in_dir`: entry only appears if the current directory path matches the provided UNIX glob pattern.
- `risky`: if true it marks the command as risky, and will ask confirmation (which defaults to false if an empty answer is provided). Avoids running dangerous scripts by mistake. 

After modifying a source file `iforgor reload` should be called to update its internal list.

## Usage

Run `iforgor` to start the interactive selection menu, which displays a list of commands that can be
selected using the up/down arrow keys and Enter. By default the search input is empty and the list
displays the command history (if any). Characters can be typed to search among the registered script
names.

Once selected the script is run. If the entry have an `args` list it asks you about the arguments
values. It'll then run the script and print its output. Execution can be halt using `Ctrl+C`, which
will only halt the script execution and not `iforgor`. Once the script stops, it displays the return
status code and wait for `Enter` to be pressed before showing back the selection menu.