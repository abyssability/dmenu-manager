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

fn get_stdin() -> io::Result<String> {
    let mut buffer = String::new();
    let result = io::stdin().read_to_string(&mut buffer);
    if let Err(err) = result {
        Err(err)
    } else {
        Ok(buffer)
    }
}

fn run() -> anyhow::Result<()> {
    let matches = parse_args();
    if atty::is(Stream::Stdin) {
        let path = matches.value_of("CONFIG").unwrap();
        let config = fs::read_to_string(&path)
            .context(format!("can't read config file `{}`", path.bold()))?;
        println!("Hello, world! Config: {}", config);
    } else {
        let piped_input = get_stdin().context("failed to read piped input")?;
        println!("Hello, pipe! I got: {}", piped_input);
    }
    Ok(())
}

fn report_errors(result: &anyhow::Result<()>) {
    if let Err(err) = result {
        let header = "Error:".red().bold();
        let err = format!("{:#}", err);
        eprintln!("{} {}.", header, err);
        process::exit(1);
    }
}

fn main() {
    let result = run();
    report_errors(&result);
}
