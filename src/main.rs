use {clap::Parser, iforgor::Cli};

fn main() -> anyhow::Result<()> {
    ctrlc::set_handler(|| {
        // don't do anything, we just want Ctrl+C to only kill
        // subcommand and not iforgor
    })
    .expect("Error setting Ctrl-C handler");

    let cli = Cli::parse();
    cli.run()
}
