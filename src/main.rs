use std::io::{self, Read, Write};
use std::process::{self, Command, Stdio};
use std::{env, fs, panic, thread};

use anyhow::Context;
use atty::Stream;
use clap::{
    crate_authors, crate_description, crate_name, crate_version, App, AppSettings, Arg, ArgMatches,
};
use colored::Colorize;
use tap::prelude::*;

use config::{Dmenu, Menu};
use tag::{Decimal, Tag, Ternary};

pub mod config;
pub mod tag;

fn parse_args() -> ArgMatches {
    App::new(crate_name!())
        .global_setting(AppSettings::ColoredHelp)
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .long_about(concat!(
            crate_description!(),
            "\n\n",
            "The toml config may be piped in instead of specifying a file path.",
        ))
        .after_help("Use `-h` for short descriptions, or `--help` for more detail.")
        .arg(
            Arg::new("CONFIG")
                .about("Path to the target toml config file")
                .index(1)
                .pipe(|arg| {
                    if atty::is(Stream::Stdin) {
                        arg.required(true)
                    } else {
                        arg
                    }
                }),
        )
        .get_matches()
}

fn read_file(args: &ArgMatches) -> anyhow::Result<String> {
    let path = args.value_of("CONFIG").expect("unreachable");
    fs::read_to_string(&path).context(format!("can't read config file `{}`", path.bold()))
}

fn read_stdin() -> anyhow::Result<String> {
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .context("failed to read piped input")?;
    Ok(buf)
}

fn run_dmenu(entries: String, dmenu_args: &[String]) -> anyhow::Result<String> {
    let mut dmenu = Command::new("dmenu")
        .args(dmenu_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn dmenu")?;
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
    let join_result = thread.join();
    match join_result {
        Ok(result) => result?,
        Err(err) => panic::resume_unwind(err),
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn get_command_choice<T: Tag>(menu: &mut Menu) -> anyhow::Result<String> {
    let capacity = menu
        .entries
        .iter()
        .fold(0, |capacity, entry| entry.name.len() + capacity);
    let capacity = capacity + (menu.entries.len() * 2);
    let entries = String::with_capacity(capacity).tap_mut(|string| {
        for (i, entry) in menu.entries.iter().enumerate() {
            string.push_str(T::new(i).as_str());
            let separator = T::separator().and_then(|def| {
                menu.config
                    .separator
                    .as_ref()
                    .and_then(|sep| sep.custom_or(def))
            });
            if let Some(separator) = separator {
                string.push_str(&separator);
            }
            string.push_str(&entry.name);
            string.push('\n');
        }
    });
    let dmenu_args = menu
        .config
        .dmenu
        .as_ref()
        .map_or_else(Vec::new, Dmenu::args);
    let raw_choice = run_dmenu(entries, &dmenu_args)?;
    let command = {
        let choice = raw_choice.trim();
        let tag = T::find(choice);

        if let Some(tag) = tag {
            let id = tag.value();
            menu.entries[id].run.clone()
        } else if choice.is_empty() {
            String::new()
        } else if menu.config.ad_hoc.unwrap_or(false) {
            String::from(choice)
        } else {
            anyhow::bail!(
                "ad-hoc commands are disabled; \
                    choose a menu option or set `config.ad-hoc = true`"
            );
        }
    };

    Ok(command)
}

fn run_command(command: &str, shell: &str) -> anyhow::Result<()> {
    if !command.is_empty() {
        Command::new(shell)
            .arg("-c")
            .arg(command)
            .spawn()
            .context(format!("failed to run command `{}`", command))?;
    }
    Ok(())
}

fn run() -> anyhow::Result<()> {
    let args = parse_args();
    let config = if args.is_present("CONFIG") {
        read_file(&args)?
    } else {
        read_stdin()?
    };
    let mut menu = Menu::try_new(&config)?;
    let numbered = menu.config.numbered.unwrap_or(false);
    let command = if numbered {
        get_command_choice::<Decimal>(&mut menu)?
    } else {
        get_command_choice::<Ternary>(&mut menu)?
    };
    let shell = menu.config.shell.as_deref().unwrap_or("sh");
    run_command(&command, shell)?;
    Ok(())
}

fn report_errors(result: &anyhow::Result<()>) {
    if let Err(err) = result {
        let header = "Error".red().bold();
        let err = format!("{:#}", err);
        eprintln!("{}: {}.", header, err);
        process::exit(1);
    }
}

fn main() {
    let result = run();
    report_errors(&result);
}
