use {clap::Parser, iforgor::Cli};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    cli.run()
}
