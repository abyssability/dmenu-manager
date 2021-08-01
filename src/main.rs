use anyhow::Context;
use atty::Stream;
use clap::{
    crate_authors, crate_description, crate_name, crate_version, App, AppSettings, Arg, ArgMatches,
};
use colored::Colorize;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::process;
use tap::prelude::*;

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

fn parse_config(config: String) -> String {
    config
}

fn run_dmenu(config: String) -> String {
    println!("Dmenu got:\n{}", config);
    String::new()
}

fn run_command(_command: String) {}

fn run() -> anyhow::Result<()> {
    let args = parse_args();
    let config = if atty::is(Stream::Stdin) {
        read_file(&args)?
    } else {
        read_stdin()?
    };
    let config = parse_config(config);
    let command = run_dmenu(config);
    run_command(command);
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
