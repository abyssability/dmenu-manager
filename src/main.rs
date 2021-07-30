use clap::{crate_authors, crate_description, crate_name, crate_version, App, Arg, ArgMatches};
use atty::Stream;
use std::io::{self, Read};
use anyhow::Context;

fn parse_args() -> ArgMatches {
    App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(concat!(
            crate_description!(),
            "\n",
            "The toml config may be piped in instead of specifying a file path.",
        ))
        .arg(
            Arg::new("CONFIG")
                .about("Path to the toml config file to use")
                .index(1),
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
        println!("Hello, world!");
    } else {
        let piped_input = get_stdin().context("failed to read piped content")?;
        println!("Hello, pipe! I got: {}", piped_input);
    }
    Ok(())
}

fn main() {
    let result = run();
    if let Err(err) = result {
        println!("Error: {:#}", err);
    }
}
