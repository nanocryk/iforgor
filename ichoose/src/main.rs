use {
    clap::Parser,
    std::io::{self, Read},
};

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(about = "Let users choose among items with a nice TUI
Choices are read from stdin in format `ID @ NAME`.
")]
pub struct Cli {
    /// Customize the title of the TUI
    #[arg(long)]
    title: Option<String>,

    /// Customize the text displayed at the bottom of the TUI.
    #[arg(long)]
    text: Option<String>,

    /// Can the user pick multiple choices.
    #[arg(long)]
    multi: bool,
}

impl Cli {
    fn run(self) -> io::Result<()> {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;

        let lines: Vec<_> = buf
            .lines()
            .map(|line| {
                let mut line_iter = line.splitn(2, " @ ");
                let key = line_iter.next().expect("missing id").trim();
                let name = line_iter.next().unwrap_or(key).to_string();

                ichoose::ListEntry { key, name }
            })
            .collect();

        let choices = ichoose::ListSearch {
            items: &lines,
            extra: ichoose::ListSearchExtra {
                title: format!(" {} ", self.title.unwrap_or_else(|| "ichoose".to_string())),
                text: self.text.unwrap_or_default(),
                ..Default::default()
            },
        }
        .run()?;

        if choices.is_empty() {
            std::process::exit(1);
        }

        for c in choices {
            println!("{c}");
        }
        Ok(())
    }
}

fn main() -> io::Result<()> {
    Cli::parse().run()
}
