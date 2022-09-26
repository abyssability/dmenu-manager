use std::{
    env,
    fmt::{self, Display, Write},
    fs,
    io::{self, ErrorKind, Read},
    panic,
    path::Path,
    process,
};

use anyhow::{anyhow, Context};
use atty::Stream;
use clap::{command, crate_description, Arg, ArgMatches};
use directories::{BaseDirs, ProjectDirs};
use termcolor::{Color, ColorSpec};
use toml::{map::Map, Value};

use crate::{bold, imstr::ImStr, style_stderr, style_stdout, HashSet};

const SHORT_EXAMPLE: &str = r#"    # A short example config; see `--help` for more info.
    [menu]
    # name = "command"
    "Say Hi" = "echo 'Hello, world!'"

    # name = { run = "command", group = <number> }
    first = { run = "echo 'first!'", group = 1 }
    last = { run = "echo 'last ...'", group = -1 }

    [config]
    shell = [ "fish", "-c" ]
    dmenu.prompt = "example:"
"#;
const LONG_EXAMPLE: &str = include_str!("../EXAMPLE.toml");
const HELP_FOOTER: &str = "Use `-h` for short descriptions, or `--help` for more detail.";

pub fn get() -> anyhow::Result<Config> {
    let dirs = ProjectDirs::from("", "", "dmm")
        .context("no valid home directory could be detected")
        .context("could not access config or cache directories")?;
    let base_dirs = BaseDirs::new().expect("unreachable");
    let args = parse_args(&dirs);

    let config = if let Some(path) = args.get_one::<String>("PATTERN") {
        fs::read_to_string(path).context(format!(
            "unable to read config file `{}`",
            style_stderr!(bold(), "{path}")
        ))?
    } else {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .context("unable to read piped input")?;
        buf
    };
    let config = config
        .parse::<Value>()
        .context("found incorrect formatting in target config")?;

    let home_config = read_home_config(dirs.config_dir())?;
    let home_config = home_config.map(|config| {
        config.parse::<Value>().context(format!(
            "found incorrect formatting in home config `{}`",
            style_stderr!(
                bold(),
                "{}",
                dirs.config_dir().join("config.toml").display()
            )
        ))
    });
    let home_config = if let Some(home_config) = home_config {
        Some(home_config?)
    } else {
        None
    };

    Config::try_new(&config, home_config.as_ref(), args, dirs, base_dirs)
}

fn read_home_config(dirs: &Path) -> anyhow::Result<Option<String>> {
    let config_path = dirs.join("config.toml");
    let result = fs::read_to_string(&config_path);
    match result {
        Ok(config) => Ok(Some(config)),
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(err).context(format!(
                    "unable to read home config file `{}`",
                    style_stderr!(bold(), "{}", config_path.display())
                ))
            }
        }
    }
}

fn parse_args(dirs: &ProjectDirs) -> ArgMatches {
    let args = command!()
        .about(concat!(crate_description!(), ".\n"))
        .long_about(&*format!(
            concat!(
                crate_description!(),
                ".\n",
                "The toml config may be piped in instead of specifying a file path.\n",
                "A config may be written at `{}/config.toml`.\n",
                "This will define default options that are overridden by the main pattern."
            ),
            dirs.config_dir().display()
        ))
        .after_help(&*format!(
            "{}\n{}\n\n{}",
            style_stdout!(ColorSpec::new().set_fg(Some(Color::Yellow)), "PATTERN:"),
            SHORT_EXAMPLE,
            HELP_FOOTER
        ))
        .after_long_help(&*format!(
            "{}\n{}\n\n{}",
            style_stdout!(ColorSpec::new().set_fg(Some(Color::Yellow)), "PATTERN:"),
            LONG_EXAMPLE,
            HELP_FOOTER
        ))
        .arg(
            Arg::new("home-config")
                .help("Output the directory that will be checked for config files")
                .long("home-config-path"),
        )
        .arg({
            let config = Arg::new("PATTERN")
                .help("Path to a pattern file")
                .long_help(
                    "Path to a pattern file.\n\
                     Either this must be specified, or the pattern must be piped in.\n\
                     If specified, anything piped through stdin is ignored.",
                )
                .index(1);
            if atty::is(Stream::Stdin) {
                config.required_unless_present("home-config")
            } else {
                config
            }
        })
        .get_matches();

    if args.contains_id("home-config") {
        println!("{}", dirs.config_dir().display());
        process::exit(0);
    }

    args
}

#[derive(Debug, Clone)]
pub enum Run {
    Shell(ImStr),
    Bare(Vec<ImStr>),
}

impl Run {
    pub fn binary(run: ImStr) -> Self {
        Self::Bare(vec![run])
    }
}

impl Display for Run {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shell(command) => write!(f, "{command}"),
            Self::Bare(command) => match command.as_slice() {
                [] => Ok(()),
                [run, args @ ..] => {
                    write!(f, "{run}")?;
                    for arg in args {
                        write!(f, " {arg}")?;
                    }
                    Ok(())
                }
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum Entry {
    Full { name: ImStr, run: Run, group: i64 },
    Name(ImStr),
    Filter(ImStr),
}

impl Entry {
    fn try_new(name: ImStr, entry: &Value) -> anyhow::Result<Self> {
        match entry {
            Value::Boolean(true) => Ok(Self::Name(name)),
            Value::Boolean(false) => Ok(Self::Filter(name)),
            Value::String(run) => Ok(Self::Full {
                name,
                run: Run::Shell(ImStr::from(run)),
                group: 0,
            }),
            Value::Array(run) => {
                let run = run
                    .iter()
                    .map(try_into_array_string(&format!("menu.{name}")))
                    .collect::<Result<Vec<ImStr>, _>>()?;

                Ok(Self::Full {
                    name,
                    run: Run::Bare(run),
                    group: 0,
                })
            }
            Value::Table(table) => {
                let group = table
                    .get("group")
                    .map(try_into_integer(&format!("menu.{name}.group")))
                    .transpose()?
                    .unwrap_or(0);

                let missing_run_error = format!(
                    "`{}` must have a value if `{}` is a table",
                    style_stderr!(bold(), "menu.{name}.run"),
                    style_stderr!(bold(), "menu.{name}"),
                );

                table
                    .get("run")
                    .map(|value| match value {
                        Value::Boolean(true) => Ok(Self::Name(name)),
                        Value::Boolean(false) => Ok(Self::Filter(name)),
                        Value::String(run) => Ok(Self::Full {
                            name,
                            run: Run::Shell(ImStr::from(run)),
                            group,
                        }),
                        Value::Array(run) => {
                            let run = run
                                .iter()
                                .map(try_into_array_string(&format!("menu.{name}.run")))
                                .collect::<Result<Vec<ImStr>, _>>()?;

                            Ok(Self::Full {
                                name,
                                run: Run::Bare(run),
                                group,
                            })
                        }
                        other => type_error(
                            "menu.{name}.run",
                            &["string", "array", "boolean"],
                            other.type_str(),
                        ),
                    })
                    .transpose()?
                    .context(missing_run_error)
            }
            other => type_error(
                "menu.{name}",
                &["string", "array", "boolean", "table"],
                other.type_str(),
            ),
        }
    }

    pub fn name(&self) -> ImStr {
        match self {
            Self::Full { name, .. } | Self::Name(name) | Self::Filter(name) => name.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Shell {
    Disabled,
    Enabled { shell: Vec<ImStr>, piped: bool },
}

impl Shell {
    pub const fn is_enabled(&self) -> bool {
        match self {
            Self::Disabled => false,
            Self::Enabled { .. } => true,
        }
    }
}

impl ConfigItem for Shell {
    fn name() -> &'static str {
        "shell"
    }
    fn merge(self, _: Self) -> Self {
        self
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::Enabled {
            shell: vec![ImStr::new("sh"), ImStr::new("-c")],
            piped: false,
        }
    }
}

impl TryFrom<&Value> for Shell {
    type Error = anyhow::Error;
    fn try_from(shell: &Value) -> anyhow::Result<Self> {
        match shell {
            Value::Boolean(false) => Ok(Self::Disabled),
            Value::Boolean(true) => Ok(Self::default()),
            Value::Array(shell) => {
                let shell = shell
                    .iter()
                    .map(try_into_array_string("config.shell"))
                    .collect::<Result<Vec<ImStr>, _>>()?;

                Ok(Self::Enabled {
                    shell,
                    piped: false,
                })
            }
            Value::Table(table) => {
                let shell = table
                    .get("shell")
                    .map(try_into_array("config.shell.shell"))
                    .transpose()?
                    .map(|value| {
                        value
                            .iter()
                            .map(try_into_array_string("config.shell.shell"))
                            .collect::<Result<Vec<ImStr>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();

                let piped = table
                    .get("piped")
                    .map(try_into_boolean("config.shell.piped"))
                    .transpose()?
                    .unwrap_or(false);

                Ok(Self::Enabled { shell, piped })
            }
            other => type_error(
                "config.shell",
                &["boolean", "array", "table"],
                other.type_str(),
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Custom {
    Disabled,
    Enabled,
}

impl ConfigItem for Custom {
    fn name() -> &'static str {
        "custom"
    }
    fn merge(self, _: Self) -> Self {
        self
    }
}

impl Default for Custom {
    fn default() -> Self {
        Self::Disabled
    }
}

impl TryFrom<&Value> for Custom {
    type Error = anyhow::Error;
    fn try_from(custom: &Value) -> anyhow::Result<Self> {
        if try_into_boolean("config.custom")(custom)? {
            Ok(Self::Disabled)
        } else {
            Ok(Self::Enabled)
        }
    }
}

#[derive(Debug, Clone)]
pub enum Numbered {
    Disabled,
    Enabled(Separator),
}

impl Numbered {
    pub fn separator(&self) -> &str {
        match self {
            Self::Disabled | Self::Enabled(Separator::Disabled) => "",
            Self::Enabled(Separator::Enabled(separator)) => separator.as_str(),
        }
    }

    pub const fn is_enabled(&self) -> bool {
        match self {
            Self::Disabled => false,
            Self::Enabled(_) => true,
        }
    }
}

impl ConfigItem for Numbered {
    fn name() -> &'static str {
        "numbered"
    }
    fn merge(self, _: Self) -> Self {
        self
    }
}

impl Default for Numbered {
    fn default() -> Self {
        Self::Disabled
    }
}

impl TryFrom<&Value> for Numbered {
    type Error = anyhow::Error;
    fn try_from(numbered: &Value) -> anyhow::Result<Self> {
        match numbered {
            Value::Boolean(false) => Ok(Self::Disabled),
            Value::Boolean(true) => Ok(Self::Enabled(Separator::default())),
            Value::Table(numbered) => {
                let enabled = numbered
                    .get("numbered")
                    .map(try_into_boolean("config.numbered.numbered"))
                    .transpose()?
                    .unwrap_or(false);

                let separator = numbered
                    .get("separator")
                    .map(Separator::try_from)
                    .transpose()?
                    .unwrap_or_default();

                if enabled {
                    Ok(Self::Enabled(separator))
                } else {
                    Ok(Self::Disabled)
                }
            }
            other => type_error("config.numbered", &["boolean", "table"], other.type_str()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Separator {
    Disabled,
    Enabled(ImStr),
}

impl Default for Separator {
    fn default() -> Self {
        Self::Enabled(ImStr::new(": "))
    }
}

impl TryFrom<&Value> for Separator {
    type Error = anyhow::Error;
    fn try_from(separator: &Value) -> anyhow::Result<Self> {
        match separator {
            Value::Boolean(false) => Ok(Self::Disabled),
            Value::Boolean(true) => Ok(Self::default()),
            Value::String(separator) => Ok(Self::Enabled(ImStr::from(separator))),
            other => type_error(
                "config.numbered.separator",
                &["boolean", "string"],
                other.type_str(),
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub enum BinPath {
    Disabled,
    Enabled {
        path: Vec<ImStr>,
        env: bool,
        replace: bool,
        recursive: bool,
        group: i64,
    },
}

impl ConfigItem for BinPath {
    fn name() -> &'static str {
        "path"
    }
    fn merge(self, _: Self) -> Self {
        self
    }
}

impl Default for BinPath {
    fn default() -> Self {
        Self::Disabled
    }
}

impl TryFrom<&Value> for BinPath {
    type Error = anyhow::Error;
    fn try_from(path: &Value) -> anyhow::Result<Self> {
        match path {
            Value::Boolean(false) => Ok(Self::Disabled),
            Value::Boolean(true) => Ok(Self::Enabled {
                path: Vec::new(),
                env: true,
                replace: false,
                recursive: false,
                group: 0,
            }),
            Value::Array(array) => {
                let path = array
                    .iter()
                    .map(try_into_array_string("config.path"))
                    .collect::<Result<Vec<ImStr>, _>>()?;

                Ok(Self::Enabled {
                    path,
                    env: false,
                    replace: false,
                    recursive: false,
                    group: 0,
                })
            }
            Value::Table(table) => {
                let path = table
                    .get("path")
                    .map(try_into_array("config.path.path"))
                    .transpose()?
                    .map(|value| {
                        value
                            .iter()
                            .map(try_into_array_string("config.path.path"))
                            .collect::<Result<Vec<ImStr>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();

                let env = table
                    .get("env")
                    .map(try_into_boolean("config.path.env"))
                    .transpose()?
                    .unwrap_or(false);

                let replace = table
                    .get("replace")
                    .map(try_into_boolean("config.path.replace"))
                    .transpose()?
                    .unwrap_or(false);

                let recursive = table
                    .get("recursive")
                    .map(try_into_boolean("config.path.recursive"))
                    .transpose()?
                    .unwrap_or(false);

                let group = table
                    .get("group")
                    .map(try_into_integer("config.path.group"))
                    .transpose()?
                    .unwrap_or(0);

                Ok(Self::Enabled {
                    path,
                    env,
                    replace,
                    recursive,
                    group,
                })
            }
            other => type_error(
                "config.numbered.separator",
                &["boolean", "array", "table"],
                other.type_str(),
            ),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Dmenu {
    pub prompt: Option<ImStr>,
    pub font: Option<ImStr>,
    pub background: Option<ImStr>,
    pub foreground: Option<ImStr>,
    pub selected_background: Option<ImStr>,
    pub selected_foreground: Option<ImStr>,
    pub lines: Option<u64>,
    pub bottom: bool,
    pub case_sensitive: bool,
    pub fast: bool,
    pub monitor: Option<u64>,
    pub window_id: Option<ImStr>,
}

impl Dmenu {
    pub fn args(&self) -> Vec<ImStr> {
        let imstr_from_int = |int: u64| ImStr::from(int.to_string());

        let mut args = Vec::new();

        let options = [
            ("-p", self.prompt.clone()),
            ("-fn", self.font.clone()),
            ("-nb", self.background.clone()),
            ("-nf", self.foreground.clone()),
            ("-sb", self.selected_background.clone()),
            ("-sf", self.selected_foreground.clone()),
            ("-l", self.lines.map(imstr_from_int)),
            ("-m", self.monitor.map(imstr_from_int)),
            ("-w", self.window_id.clone()),
        ];

        self.bottom.then(|| args.push(ImStr::new("-b")));
        (!self.case_sensitive).then(|| args.push(ImStr::new("-i")));
        self.fast.then(|| args.push(ImStr::new("-f")));

        for (flag, option) in options {
            if let Some(option) = option {
                args.extend([ImStr::new(flag), option]);
            }
        }

        args
    }
}

impl ConfigItem for Dmenu {
    fn name() -> &'static str {
        "dmenu"
    }
    fn merge(self, default: Self) -> Self {
        Self {
            prompt: self.prompt.or(default.prompt),
            font: self.font.or(default.font),
            background: self.background.or(default.background),
            foreground: self.foreground.or(default.foreground),
            selected_background: self.selected_background.or(default.selected_background),
            selected_foreground: self.selected_foreground.or(default.selected_foreground),
            lines: self.lines.or(default.lines),
            bottom: self.bottom || default.bottom,
            case_sensitive: self.case_sensitive || default.case_sensitive,
            fast: self.fast || default.fast,
            monitor: self.monitor.or(default.monitor),
            window_id: self.window_id.or(default.window_id),
        }
    }
}

impl TryFrom<&Value> for Dmenu {
    type Error = anyhow::Error;
    fn try_from(dmenu: &Value) -> anyhow::Result<Self> {
        let dmenu = try_into_table("config.dmenu")(dmenu)?;

        Ok(Self {
            prompt: dmenu
                .get("prompt")
                .map(try_into_string("config.dmenu.prompt"))
                .transpose()?,
            font: dmenu
                .get("font")
                .map(try_into_string("config.dmenu.font"))
                .transpose()?,
            background: dmenu
                .get("background")
                .map(try_into_string("config.dmenu.background"))
                .transpose()?,
            foreground: dmenu
                .get("foreground")
                .map(try_into_string("config.dmenu.foreground"))
                .transpose()?,
            selected_background: dmenu
                .get("selected-background")
                .map(try_into_string("config.dmenu.selected-background"))
                .transpose()?,
            selected_foreground: dmenu
                .get("selected-foreground")
                .map(try_into_string("config.dmenu.selected-foreground"))
                .transpose()?,
            lines: dmenu
                .get("lines")
                .map(try_into_integer("config.dmenu.lines"))
                .transpose()?
                .map(try_into_unsigned_integer("config.dmenu.lines"))
                .transpose()?,
            bottom: dmenu
                .get("bottom")
                .map(try_into_boolean("config.dmenu.bottom"))
                .transpose()?
                .unwrap_or(false),
            case_sensitive: dmenu
                .get("case-sensitive")
                .map(try_into_boolean("config.dmenu.case-sensitive"))
                .transpose()?
                .unwrap_or(false),
            fast: dmenu
                .get("fast")
                .map(try_into_boolean("config.dmenu.fast"))
                .transpose()?
                .unwrap_or(false),
            monitor: dmenu
                .get("monitor")
                .map(try_into_integer("config.dmenu.monitor"))
                .transpose()?
                .map(try_into_unsigned_integer("config.dmenu.monitor"))
                .transpose()?,
            window_id: dmenu
                .get("window-id")
                .map(try_into_string("config.dmenu.window-id"))
                .transpose()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub args: ArgMatches,
    pub dirs: ProjectDirs,
    pub base_dirs: BaseDirs,
    pub entries: Vec<Entry>,
    pub shell: Shell,
    pub custom: Custom,
    pub numbered: Numbered,
    pub path: BinPath,
    pub dmenu: Dmenu,
}

impl Config {
    pub fn try_new(
        config: &Value,
        home_config: Option<&Value>,
        args: ArgMatches,
        dirs: ProjectDirs,
        base_dirs: BaseDirs,
    ) -> anyhow::Result<Self> {
        let config_path = dirs.config_dir().join("config.toml");
        Ok(Self {
            entries: try_get_entries(config, home_config, &config_path)?,
            shell: try_get_config::<Shell>(config, home_config, &config_path)?,
            custom: try_get_config::<Custom>(config, home_config, &config_path)?,
            numbered: try_get_config::<Numbered>(config, home_config, &config_path)?,
            path: try_get_config::<BinPath>(config, home_config, &config_path)?,
            dmenu: try_get_config::<Dmenu>(config, home_config, &config_path)?,
            args,
            dirs,
            base_dirs,
        })
    }
}

fn try_get_entries(
    config: &Value,
    home_config: Option<&Value>,
    config_path: &Path,
) -> anyhow::Result<Vec<Entry>> {
    let mut menu = config
        .get("menu")
        .map(try_into_table("menu"))
        .transpose()?
        .into_iter()
        .flatten()
        .map(|(name, value)| Entry::try_new(ImStr::from(name), value))
        .collect::<Result<Vec<Entry>, _>>()
        .context(target_config_error())?;

    let home_menu = home_config
        .and_then(|config| config.get("menu"))
        .map(try_into_table("menu"))
        .transpose()?
        .into_iter()
        .flatten()
        .map(|(name, value)| Entry::try_new(ImStr::from(name), value))
        .collect::<Result<Vec<Entry>, _>>()
        .context(home_config_error(config_path))?;

    let entry_names = menu.iter().map(Entry::name).collect::<HashSet<ImStr>>();

    menu.extend(
        home_menu
            .into_iter()
            .filter(|entry| !entry_names.contains(&entry.name())),
    );

    Ok(menu)
}

fn try_get_config<'a, T: ConfigItem>(
    config: &'a Value,
    home_config: Option<&'a Value>,
    config_path: &Path,
) -> anyhow::Result<T> {
    let config = config
        .get("config")
        .map(try_into_table("config"))
        .transpose()
        .context(target_config_error())?
        .and_then(|config| config.get(T::name()))
        .map(T::try_from)
        .transpose()
        .context(target_config_error())?;

    let home_config = home_config
        .and_then(|config| config.get("config"))
        .map(try_into_table("config"))
        .transpose()
        .context(home_config_error(config_path))?
        .and_then(|config| config.get(T::name()))
        .map(T::try_from)
        .transpose()
        .context(home_config_error(config_path))?
        .map(|config| config.merge(T::default()))
        .unwrap_or_default();

    if let Some(config) = config {
        Ok(config.merge(home_config))
    } else {
        Ok(home_config)
    }
}

fn type_error<T>(name: &str, valid: &[&str], found: &str) -> anyhow::Result<T> {
    let mut types = String::new();
    match valid {
        [] => panic!("provide at least one valid type"),
        [valid] => write!(types, "`{}`", style_stderr!(bold(), "{valid}")).unwrap(),
        [left, right] => write!(
            types,
            "`{}` or `{}`",
            style_stderr!(bold(), "{left}"),
            style_stderr!(bold(), "{right}")
        )
        .expect("unreachable"),
        [valid @ .., last] => {
            for valid in valid {
                write!(types, "`{}`, ", style_stderr!(bold(), "{valid}")).unwrap();
            }
            write!(types, "or `{}`", style_stderr!(bold(), "{last}")).unwrap();
        }
    }

    Err(anyhow!(
        "`{}` must be of type {types}, but is of type `{}`",
        style_stderr!(bold(), "{name}"),
        style_stderr!(bold(), "{found}")
    ))
}

fn try_into_string(name: &str) -> impl Fn(&Value) -> anyhow::Result<ImStr> + '_ {
    move |value| match value {
        Value::String(value) => Ok(ImStr::from(value)),
        other => type_error(name, &["string"], other.type_str()),
    }
}

fn try_into_boolean(name: &str) -> impl Fn(&Value) -> anyhow::Result<bool> + '_ {
    move |value| match value {
        Value::Boolean(value) => Ok(*value),
        other => type_error(name, &["boolean"], other.type_str()),
    }
}

fn try_into_integer(name: &str) -> impl Fn(&Value) -> anyhow::Result<i64> + '_ {
    move |value| match value {
        Value::Integer(value) => Ok(*value),
        other => type_error(name, &["integer"], other.type_str()),
    }
}

fn try_into_table(name: &str) -> impl Fn(&Value) -> anyhow::Result<&Map<String, Value>> + '_ {
    move |value| match value {
        Value::Table(value) => Ok(value),
        other => type_error(name, &["table"], other.type_str()),
    }
}

fn try_into_array(name: &str) -> impl Fn(&Value) -> anyhow::Result<&Vec<Value>> + '_ {
    move |value| match value {
        Value::Array(value) => Ok(value),
        other => type_error(name, &["array"], other.type_str()),
    }
}

fn try_into_array_string(name: &str) -> impl Fn(&Value) -> anyhow::Result<ImStr> + '_ {
    move |value| {
        match value {
        Value::String(value) => Ok(ImStr::from(value)),
        other => Err(anyhow!(
            "the array `{}` must only contain elements of type `{}`, but an element is of type `{}`",
            style_stderr!(bold(), "{name}"),
            style_stderr!(bold(), "string"),
            style_stderr!(bold(), "{}", other.type_str())
        )),
        }
    }
}

fn try_into_unsigned_integer(name: &str) -> impl Fn(i64) -> anyhow::Result<u64> + '_ {
    move |value| {
        value.try_into().map_err(|_| {
            anyhow!(
                "`{}` must be a positive integer, but is negative",
                style_stderr!(bold(), "{name}"),
            )
        })
    }
}

fn home_config_error(path: &Path) -> String {
    format!(
        "found a problem with home config `{}`",
        style_stderr!(bold(), "{}", path.display())
    )
}

const fn target_config_error() -> &'static str {
    "found a problem with provided config"
}

trait ConfigItem: for<'a> TryFrom<&'a Value, Error = anyhow::Error> + Default {
    fn name() -> &'static str;
    fn merge(self, default: Self) -> Self;
}
