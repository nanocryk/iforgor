# `ichoose`

[![ichoose crate](https://img.shields.io/crates/v/ichoose?label=ichoose)](https://crates.io/crates/ichoose)
[![ichoose documentation](https://img.shields.io/docsrs/ichoose/latest?label=ichoose%20docs)](https://docs.rs/ichoose)

This crate manages the interactive menu with customizable features, which allows you to use it in
your Rust applications or scripts. It supports customizing the lists being showed and enabling
multi-selection.

The binary version for scripts allow performing a selection amongst a list provided in the standard
input, formatted as one entry per line as `ID @ NAME` (if ` @ ` is not found then the line will be
used both as the id and name). Once entries are selected it will returns only the `ID`, one per
line. It can be used in piped command where `grep` would be used, but instead allows the user to
perform the selection.

Multi-selection can be enabled with flag `--multi`, while title and bottom text can be customized
using `--title <TITLE>` and `--text <TEXT>`. 