use std::convert::TryFrom;

use anyhow::Context;
use serde::Deserialize;
use toml::{value::Table, Value};

#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(rename = "ad-hoc")]
    pub ad_hoc: bool,
    pub numbered: bool,
    pub separator: Separator,
    pub shell: String,
    pub dmenu: Dmenu,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ad_hoc: false,
            numbered: false,
            separator: Separator::True,
            shell: "sh".to_owned(),
            dmenu: Dmenu::default(),
        }
    }
}

#[derive(Default, Deserialize)]
pub struct Dmenu {
    #[serde(default)]
    pub bottom: bool,
    #[serde(default)]
    pub fast: bool,
    #[serde(default)]
    #[serde(rename = "case-sensitive")]
    pub case_sensitive: bool,
    pub lines: Option<u32>,
    pub monitor: Option<u32>,
    pub prompt: Option<String>,
    pub font: Option<String>,
    pub background: Option<String>,
    pub foreground: Option<String>,
    #[serde(rename = "selected-background")]
    pub selected_background: Option<String>,
    #[serde(rename = "selected-foreground")]
    pub selected_foreground: Option<String>,
    #[serde(rename = "window-id")]
    pub window_id: Option<String>,
}

impl Dmenu {
    pub fn args(&self) -> Vec<String> {
        fn push_arg<T>(
            args: &mut Vec<String>,
            flag: &str,
            arg: Option<T>,
            f: impl FnOnce(T) -> String,
        ) {
            if let Some(value) = arg {
                args.push(flag.to_owned());
                args.push(f(value));
            }
        }

        let mut args = Vec::new();

        let mut add_arg = |cond, flag: &str| {
            if cond {
                args.push(flag.to_owned());
            }
        };

        add_arg(self.bottom, "-b");
        add_arg(self.fast, "-f");
        add_arg(!self.case_sensitive, "-i");

        push_arg(&mut args, "-l", self.lines, |n| n.to_string());
        push_arg(&mut args, "-m", self.monitor, |n| n.to_string());

        let args_list = [
            ("-p", &self.prompt),
            ("-fn", &self.font),
            ("-nb", &self.background),
            ("-nf", &self.foreground),
            ("-sb", &self.selected_background),
            ("-sf", &self.selected_foreground),
            ("-w", &self.window_id),
        ];

        for (flag, arg) in args_list {
            push_arg(&mut args, flag, arg.as_ref(), String::clone);
        }

        args
    }
}

#[derive(Deserialize)]
#[serde(try_from = "Value")]
pub enum Separator {
    False,
    True,
    Custom(String),
}

impl Separator {
    pub fn custom_or<'a>(&'a self, def: &'a str) -> Option<&'a str> {
        match self {
            Self::False => None,
            Self::True => Some(def),
            Self::Custom(custom) => Some(custom),
        }
    }
}

impl TryFrom<Value> for Separator {
    type Error = anyhow::Error;
    fn try_from(value: Value) -> anyhow::Result<Self> {
        match value {
            Value::Boolean(false) => Ok(Self::False),
            Value::Boolean(true) => Ok(Self::True),
            Value::String(separator) => Ok(Self::Custom(separator)),
            other => Err(not_valid("separator", "`boolean` or `string`", &other)),
        }
    }
}

pub struct Menu {
    pub entries: Vec<Entry>,
    pub config: Config,
}

impl Menu {
    pub fn try_new(config: &str) -> anyhow::Result<Self> {
        if config.trim().is_empty() {
            anyhow::bail!("provided config is empty");
        }

        let value = config
            .parse::<Value>()
            .context("can't parse provided config")?;

        match value {
            Value::Table(mut table) => {
                let config = table
                    .remove("config")
                    .map_or_else(|| Ok(Config::default()), Value::try_into)?;
                let mut entries = Entries::try_new(&mut table)?;
                entries.sort_unstable_by(|a, b| {
                    a.group
                        .cmp(&b.group)
                        .reverse()
                        .then_with(|| a.name.cmp(&b.name))
                });

                Ok(Self { entries, config })
            }
            _ => unreachable!(),
        }
    }
}

struct Entries;

impl Entries {
    fn try_new(table: &mut Table) -> anyhow::Result<Vec<Entry>> {
        let menu = table
            .remove("menu")
            .map(|value| match value {
                Value::Table(table) => Ok(table),
                other => Err(not_valid("menu", "`table`", &other)),
            })
            .transpose()?;

        let entries = table
            .remove("entries")
            .map(|value| match value {
                Value::Array(array) => Ok(array),
                other => Err(not_valid("entries", "`array`", &other)),
            })
            .transpose()?;

        match (menu, entries) {
            (Some(menu), Some(entries)) => menu
                .into_iter()
                .map(|(name, value)| Entry::try_new(value, Some(name)))
                .chain(entries.into_iter().map(|value| Entry::try_new(value, None)))
                .collect::<Result<Vec<_>, _>>(),
            (Some(menu), None) => menu
                .into_iter()
                .map(|(name, value)| Entry::try_new(value, Some(name)))
                .collect::<Result<Vec<_>, _>>(),
            (None, Some(entries)) => entries
                .into_iter()
                .map(|value| Entry::try_new(value, None))
                .collect::<Result<Vec<_>, _>>(),
            (None, None) => Err(anyhow::anyhow!(
                "no menu entries defined; \
                give at least one of `menu` or `entries` a value;\n\
                try --help for more info"
            )),
        }
    }
}

pub struct Entry {
    pub name: String,
    pub run: String,
    pub group: i32,
}

impl Entry {
    fn new(run: String) -> Self {
        Self {
            name: run.clone(),
            run,
            group: 0,
        }
    }

    fn try_new(value: Value, name: Option<String>) -> anyhow::Result<Self> {
        match value {
            Value::String(run) => {
                if let Some(name) = name {
                    Ok(Self {
                        name,
                        run,
                        group: 0,
                    })
                } else {
                    Ok(Self::new(run))
                }
            }
            Value::Table(mut table) => {
                let run: String = table.remove("run").map(Value::try_into).context(
                    if let Some(ref name) = name {
                        format!("`menu.{}` has no `run` value", name)
                    } else {
                        "entry has no `run` value".to_owned()
                    },
                )??;
                let group = table
                    .remove("group")
                    .map(Value::try_into)
                    .transpose()?
                    .unwrap_or(0);
                let name = name.unwrap_or_else(|| run.clone());

                Ok(Self { name, run, group })
            }
            other => {
                if let Some(name) = name {
                    Err(not_valid(
                        &format!("menu.{}", name),
                        "`string` or `table`",
                        &other,
                    ))
                } else {
                    Err(not_valid("entries", "`string` or `table`", &other))
                }
            }
        }
    }
}

fn not_valid(target: &str, valid: &str, found: &Value) -> anyhow::Error {
    anyhow::anyhow!(
        "only toml {} is a valid type for `{}`; found `{}`",
        valid,
        target,
        found.type_str()
    )
}
