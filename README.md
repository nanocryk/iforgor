# iforgor
The CLI tool for all those commands you forget about

## Installation

Run `cargo install iforgor`.

## Configuration

Add script source files using `iforgor source add <PATH>` (see [exemple](iforgor/exemple.toml)).
Each entry follow the following format:

```toml
[[entries]]
name = "WRITE NAME HERE"
script = "WRITE SCRIPT HERE"
```

Entry can also contain the following optional fields:
- `only_on = "OS"`: script will only be loaded on provided OS. Accepts `Linux` and `Windows`.
- `args = ["Arg 1", "Arg 2"]`: list arguments labels that will be printed when calling.

After modifying a source file `iforgor reload` should be called to update its internal list.

## Usage

Run `iforgor` to start the interactive selection menu, which displays a list of commands that
can be selected using the up/down arrow keys and Enter. By default the search input is empty and
the list displays the command history (if any). Characters can be typed to search among the
registered script names.

Once selected the script is run. If the entry have an `args` list it asks you about the
arguments values. It'll then run the script and print its output. Execution can be halt using
`Ctrl+C`, which will only halt the script execution and not `iforgor`. Once the script stops, it
displays the return status code and wait for `Enter` to be pressed before showing back the selection
menu.

## `ichoose`

This crate manages the interactive menu with customizable features, which allows you to use it
in your Rust applications or scripts. It supports customizing the lists being showed and enabling
multi-selection.

The binary version for scripts allow performing a selection amongst a list provided in the
standard input, formatted as one entry per line as `ID @ NAME`. Once entries are selected it will
returns only the `ID`, one per line. It can be used in piped command where `grep` would be used,
but instead allows the user to perform the selection.

Multi-selection can be enabled with flag `--multi`, while title and bottom text can be customized
using `--title <TITLE>` and `--text <TEXT>`. 