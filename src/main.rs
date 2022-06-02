use std::io::{self, Read, Write};
use std::process::{self, Command, Stdio};
use std::{env, fs, panic, thread};

use anyhow::Context;
use clap::{command, crate_description, Arg, ArgMatches};
use is_terminal::IsTerminal;
use owo_colors::OwoColorize;

use config::Menu;
use tag::{Decimal, Tag, Ternary};

mod config;
mod tag;

static SHORT_EXAMPLE: &str = r#"    # short example config; see `--help` for more info
    [menu]
    # name = "command"
    say-hi = "echo 'Hello, world!'"

    # name = { run = "command", group = <number> }
    first = { run = "echo 'first!'", group = 1 }

    [config]
    dmenu.prompt = "example:"
"#;

static HELP_FOOTER: &str = "Use `-h` for short descriptions, or `--help` for more detail.";

fn main() {
    if let Err(err) = run() {
        report_errors(&err);

        process::exit(1);
    }
}

fn report_errors(err: &anyhow::Error) {
    let mut chain = err.chain();
    let err = chain.next().unwrap_or_else(|| unreachable!());

    eprintln!("{} {}", "error:".red().bold(), err);

    for err in chain {
        eprintln!("  {} {}", "-".yellow().bold(), err);
    }
}

fn run() -> anyhow::Result<()> {
    let args = parse_args();
    let config = if let Some(path) = args.value_of("CONFIG") {
        read_file(path)?
    } else {
        read_stdin()?
    };
    let menu = Menu::try_new(&config)?;
    let commands = if menu.config.numbered {
        get_command_choice::<Decimal>(&menu)
    } else {
        get_command_choice::<Ternary>(&menu)
    }
    .context("failed to get menu selection")?;
    run_command(&commands, &menu.config.shell)?;
    Ok(())
}

fn parse_args() -> ArgMatches {
    command!()
        .long_about(concat!(
            crate_description!(),
            "\n",
            "The toml config may be piped in instead of specifying a file path.",
        ))
        .after_help(
            format!(
                "{}\n{}\n\n{}",
                "CONFIG:".yellow(),
                SHORT_EXAMPLE,
                HELP_FOOTER
            )
            .as_str(),
        )
        .after_long_help(
            format!(
                "{}\n{}\n\n{}",
                "CONFIG:".yellow(),
                include_str!("../EXAMPLE.toml"),
                HELP_FOOTER
            )
            .as_str(),
        )
        .arg({
            let arg = Arg::new("CONFIG")
                .help("Path to the target toml config file")
                .long_help(
                    "Path to the target toml config file.\n\
                    Required unless piping config through stdin.\n\
                    If set, anything sent through stdin is ignored.",
                )
                .index(1);
            if io::stdin().is_terminal() {
                arg.required(true)
            } else {
                arg
            }
        })
        .get_matches()
}

fn read_file(path: &str) -> anyhow::Result<String> {
    fs::read_to_string(path).context(format!("can't read config file `{}`", path.bold()))
}

fn read_stdin() -> anyhow::Result<String> {
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .context("failed to read piped input")?;
    Ok(buf)
}

fn get_command_choice<T: Tag>(menu: &Menu) -> anyhow::Result<Vec<String>> {
    let entries = construct_entries::<T>(menu);
    let dmenu_args = menu.config.dmenu.args();
    let raw_choice = run_dmenu(entries, &dmenu_args)?;
    let choices = raw_choice.trim().split('\n');
    let commands = choices
        .map(str::trim)
        .filter(|choice| !choice.is_empty())
        .map(|choice| {
            let tag = T::find(choice);

            if let Some(tag) = tag {
                let id = tag.value();
                Ok(menu.entries[id].run.clone())
            } else if menu.config.ad_hoc {
                Ok(String::from(choice))
            } else {
                anyhow::bail!(
                    "ad-hoc commands are disabled \
                    (choose a menu option or set `config.ad-hoc = true`)"
                );
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(commands)
}

fn construct_entries<T: Tag>(menu: &Menu) -> String {
    let separator = T::separator().and_then(|def| menu.config.separator.custom_or(def));
    let mut entries = String::new();

    for (i, entry) in menu.entries.iter().enumerate() {
        entries.push_str(T::new(i).as_str());
        if let Some(separator) = separator {
            entries.push_str(separator);
        }
        entries.push_str(&entry.name);
        entries.push('\n');
    }

    entries
}

fn run_dmenu(entries: String, dmenu_args: &[String]) -> anyhow::Result<String> {
    let mut dmenu = Command::new("dmenu")
        .args(dmenu_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run `dmenu` (is it installed?)")?;
    let mut stdin = dmenu
        .stdin
        .take()
        .context("failed to establish pipe to dmenu")?;
    let thread = thread::spawn(move || {
        stdin
            .write_all(entries.as_bytes())
            .context("failed to write to dmenu stdin")
    });
    let output = dmenu
        .wait_with_output()
        .context("failed to read dmenu stdout")?;
    match thread.join() {
        Ok(result) => result?,
        Err(err) => panic::resume_unwind(err),
    }

    Ok(String::from_utf8(output.stdout)?)
}

fn run_command(commands: &[String], shell: &str) -> anyhow::Result<()> {
    for command in commands {
        Command::new(shell)
            .arg("-c")
            .arg(command)
            .spawn()
            .context(format!(
                "failed to execute command `{}` (is the shell `{}` installed?)",
                command.bold(),
                shell.bold(),
            ))?;
    }
    Ok(())
}
