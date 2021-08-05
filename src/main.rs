use anyhow::Context;
use atty::Stream;
use clap::{
    crate_authors, crate_description, crate_name, crate_version, App, AppSettings, Arg, ArgMatches,
};
use colored::Colorize;
use serde::Deserialize;
use std::convert::TryInto;
use std::io::{self, Read, Write};
use std::process::{self, Command, Stdio};
use std::{env, fs, panic, thread};
use tap::prelude::*;
use toml::Value;

const ZWJ: char = '\u{200d}';
const ZWNJ: char = '\u{200c}';

struct JoinerNum(String);

impl JoinerNum {
    fn new(num: u32) -> Self {
        let binary = format!("{:b}", num);
        let jnum = binary
            .chars()
            .filter_map(|char| match char {
                '0' => Some(ZWJ),
                '1' => Some(ZWNJ),
                _ => unreachable!(),
            })
            .collect::<String>();
        assert!(!jnum.is_empty());

        Self(jnum)
    }

    fn value(&self) -> u32 {
        if self.0.is_empty() {
            return 0;
        }

        let binary = self
            .0
            .chars()
            .filter_map(|char| match char {
                ZWJ => Some('0'),
                ZWNJ => Some('1'),
                _ => unreachable!(),
            })
            .collect::<String>();

        u32::from_str_radix(binary.as_str(), 2).expect("unreachable")
    }

    fn from_str(string: &str) -> Self {
        if string.is_empty() {
            return Self(String::from(ZWJ));
        }

        for char in string.chars() {
            if char != ZWJ && char != ZWNJ {
                panic!(
                    "provided string contains character other than `zero width {{,non}} joiner`{}",
                    ""
                )
            }
        }

        Self(String::from(string))
    }

    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Deserialize)]
struct RawMenu {
    menu: Option<Vec<Value>>,
    #[serde(rename = "entry")]
    entries: Option<Vec<Value>>,
    config: Option<Config>,
}

#[derive(Default, Deserialize)]
struct Config {
    #[serde(rename = "ad-hoc")]
    ad_hoc: Option<bool>,
    dmenu: Option<DmenuConfig>,
}

#[derive(Default, Deserialize)]
struct DmenuConfig {
    bottom: Option<bool>,
    fast: Option<bool>,
    insensitive: Option<bool>,
    lines: Option<u32>,
    monitor: Option<u32>,
    prompt: Option<String>,
    font: Option<String>,
    background: Option<String>,
    foreground: Option<String>,
    #[serde(rename = "selected-background")]
    selected_background: Option<String>,
    #[serde(rename = "selected-foreground")]
    selected_foreground: Option<String>,
    #[serde(rename = "window-id")]
    window_id: Option<String>,
}

struct Menu {
    menu: Vec<Entry>,
    config: Config,
}

impl Menu {
    fn try_new(mut raw_menu: RawMenu) -> anyhow::Result<Self> {
        let (entries, mut errs) = raw_menu
            .menu
            .take()
            .into_iter()
            .chain(raw_menu.entries.take().into_iter())
            .flatten()
            .map(Entry::try_new)
            .partition::<Vec<_>, _>(|result| result.is_ok());

        if !errs.is_empty() {
            if let Err(err) = errs.remove(0) {
                return Err(err);
            }
        }

        let menu = entries
            .into_iter()
            .map(|result| result.expect("unreachable"))
            .collect();

        let config = if let Some(config) = raw_menu.config {
            config
        } else {
            Config::default()
        };

        Ok(Self { menu, config })
    }
}

#[derive(Debug)]
struct Entry {
    name: String,
    run: String,
}

impl Entry {
    fn new(name: String) -> Self {
        Self {
            run: name.clone(),
            name,
        }
    }

    fn try_from_table(mut table: toml::map::Map<String, Value>) -> anyhow::Result<Self> {
        let mut get_value = |key| {
            table
                .remove(key)
                .map(|value| {
                    if let Value::String(string) = value {
                        Some(string)
                    } else {
                        None
                    }
                })
                .flatten()
                .context(format!("menu entry `{}` not valid", key))
        };
        let name = get_value("name")?;
        let run = get_value("run")?;

        Ok(Self { name, run })
    }

    fn try_new(value: Value) -> anyhow::Result<Self> {
        match value {
            Value::String(name) => Ok(Entry::new(name)),
            Value::Table(table) => Ok(Entry::try_from_table(table)?),
            err => anyhow::bail!("failed to parse menu entry `{}`", err),
        }
    }
}

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

fn parse_config(config: String) -> anyhow::Result<Menu> {
    let raw_menu = toml::from_str::<RawMenu>(config.as_str())
        .context("can't parse menu entries in toml config")?;
    let menu = Menu::try_new(raw_menu)?;
    Ok(menu)
}

fn dmenu_args(mut config: DmenuConfig) -> Vec<String> {
    let mut args = Vec::new();

    let mut add_arg = |arg: Option<bool>, default, flag| {
        if arg.unwrap_or(default) {
            args.push(String::from(flag));
        }
    };
    fn push_arg<T>(
        args: &mut Vec<String>,
        arg: Option<T>,
        flag: &str,
        f: impl FnOnce(T) -> String,
    ) {
        if let Some(value) = arg {
            args.push(String::from(flag));
            args.push(f(value));
        }
    }

    add_arg(config.bottom, false, "-b");
    add_arg(config.fast, false, "-f");
    add_arg(config.insensitive, true, "-i");

    push_arg(&mut args, config.lines.take(), "-l", |lines| {
        format!("{}", lines)
    });
    push_arg(&mut args, config.monitor.take(), "-m", |monitor| {
        format!("{}", monitor)
    });

    let args_list = [
        (config.prompt.take(), "-p"),
        (config.font.take(), "-fn"),
        (config.background.take(), "-nb"),
        (config.foreground.take(), "-nf"),
        (config.selected_background.take(), "-sb"),
        (config.selected_foreground.take(), "-sf"),
        (config.window_id.take(), "-w"),
    ];

    for (arg, flag) in args_list {
        push_arg(&mut args, arg, flag, |value| value);
    }

    args
}

fn run_dmenu(entries: String, dmenu_args: Vec<String>) -> anyhow::Result<String> {
    let mut dmenu = Command::new("dmenu")
        .args(dmenu_args.as_slice())
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

fn strip_joiners(string: &str) -> &str {
    let first = string
        .chars()
        .enumerate()
        .skip_while(|(_, c)| *c == ZWJ || *c == ZWNJ)
        .map(|(i, _)| i)
        .next();

    if let Some(first) = first {
        &string[first * 3..]
    } else {
        string
    }
}

fn take_joiners(string: &str) -> &str {
    let last = string
        .chars()
        .enumerate()
        .take_while(|(_, c)| *c == ZWJ || *c == ZWNJ)
        .count();

    if last > 0 {
        &string[..last * 3]
    } else {
        ""
    }
}

fn get_command_choice(mut menu: Menu) -> anyhow::Result<String> {
    let capacity = menu
        .menu
        .iter()
        .fold(0, |capacity, entry| entry.name.len() + capacity);
    let capacity = capacity + (menu.menu.len() * 2);
    let entries = String::with_capacity(capacity).tap_mut(|string| {
        for (i, entry) in menu.menu.iter().enumerate() {
            string.push_str(JoinerNum::new(i.try_into().expect("too many menu entries")).as_str());
            string.push_str(strip_joiners(entry.name.as_str()));
            string.push('\n')
        }
    });
    let dmenu_args = if let Some(config) = menu.config.dmenu.take() {
        dmenu_args(config)
    } else {
        Vec::new()
    };
    let raw_choice = run_dmenu(entries, dmenu_args)?;
    let idstr = take_joiners(raw_choice.as_str());
    let choice = JoinerNum::from_str(idstr).value();
    let id: usize = choice.try_into().expect("error converting u32 to usize?");
    let command = if idstr.is_empty() {
        if raw_choice.trim().is_empty() {
            String::new()
        } else if menu.config.ad_hoc.unwrap_or(false) {
            raw_choice.tap_mut(|string| {
                string.pop();
            })
        } else {
            anyhow::bail!("ad-hoc commands are disabled; choose a menu option or set `config.ad-hoc = true` in the config");
        }
    } else {
        menu.menu[id].run.clone()
    };

    Ok(command)
}

fn run_command(command: String) {
    if !command.is_empty() {
        Command::new("sh")
            .arg("-c")
            .arg(&command)
            .spawn()
            .unwrap_or_else(|_| panic!("failed to run command `{}`", command));
    }
}

fn run() -> anyhow::Result<()> {
    let args = parse_args();
    let config = if atty::is(Stream::Stdin) {
        read_file(&args)?
    } else {
        read_stdin()?
    };
    let config = parse_config(config)?;
    let command = get_command_choice(config)?;
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
