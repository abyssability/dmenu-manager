use std::convert::TryFrom;

use anyhow::Context;
use serde::Deserialize;
use toml::{value::Table, Value};

#[derive(Default, Deserialize)]
pub struct Config {
    #[serde(rename = "ad-hoc")]
    pub ad_hoc: Option<bool>,
    pub numbered: Option<bool>,
    pub separator: Option<Separator>,
    pub shell: Option<String>,
    pub dmenu: Option<Dmenu>,
}

#[derive(Default, Deserialize)]
pub struct Dmenu {
    pub bottom: Option<bool>,
    pub fast: Option<bool>,
    #[serde(rename = "case-sensitive")]
    pub case_sensitive: Option<bool>,
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
            arg: Option<T>,
            flag: &str,
            f: impl FnOnce(T) -> String,
        ) {
            if let Some(value) = arg {
                args.push(String::from(flag));
                args.push(f(value));
            }
        }

        let mut args = Vec::new();

        let mut add_arg = |arg: Option<bool>, cond, flag| {
            if arg.unwrap_or(false) == cond {
                args.push(String::from(flag));
            }
        };

        add_arg(self.bottom, true, "-b");
        add_arg(self.fast, true, "-f");
        add_arg(self.case_sensitive, false, "-i");

        push_arg(&mut args, self.lines, "-l", |lines| lines.to_string());
        push_arg(&mut args, self.monitor, "-m", |monitor| monitor.to_string());

        let args_list = [
            (&self.prompt, "-p"),
            (&self.font, "-fn"),
            (&self.background, "-nb"),
            (&self.foreground, "-nf"),
            (&self.selected_background, "-sb"),
            (&self.selected_foreground, "-sf"),
            (&self.window_id, "-w"),
        ];

        for (arg, flag) in args_list {
            push_arg(&mut args, arg.as_ref(), flag, Clone::clone);
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
    pub fn custom_or(&self, def: &str) -> Option<String> {
        match self {
            Self::False => None,
            Self::True => Some(def.to_string()),
            Self::Custom(custom) => Some(custom.clone()),
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

impl Default for Separator {
    fn default() -> Self {
        Self::False
    }
}

pub struct Menu {
    pub entries: Vec<Entry>,
    pub config: Config,
}

impl Menu {
    pub fn try_new(config: &str) -> anyhow::Result<Self> {
        if config.trim().is_empty() {
            anyhow::bail!("provided toml config is empty");
        }

        let value = config
            .parse::<Value>()
            .context("can't parse provided toml config")?;

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
            other => panic!(
                "`config::Menu` construction requires a `toml::Value::Table`; found `{:?}`",
                other
            ),
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
                .map(Entry::try_from_menu)
                .chain(entries.into_iter().map(Entry::try_from_entry))
                .collect::<Result<Vec<_>, _>>(),
            (Some(menu), None) => menu
                .into_iter()
                .map(Entry::try_from_menu)
                .collect::<Result<Vec<_>, _>>(),
            (None, Some(entries)) => entries
                .into_iter()
                .map(Entry::try_from_entry)
                .collect::<Result<Vec<_>, _>>(),
            (None, None) => Err(anyhow::anyhow!(
                "no menu entries defined; give at least one of `menu` or `entries` a value"
            )),
        }
    }
}

#[derive(Debug)]
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

    fn try_from_menu((name, value): (String, Value)) -> anyhow::Result<Self> {
        match value {
            Value::String(run) => Ok(Self {
                name,
                run,
                group: 0,
            }),
            Value::Table(mut table) => Ok(Self::try_new(&mut table, Some(name))?),
            other => Err(not_valid(
                &format!("menu.{}", name),
                "`string` or `table`",
                &other,
            )),
        }
    }

    fn try_from_entry(value: Value) -> anyhow::Result<Self> {
        match value {
            Value::String(run) => Ok(Self::new(run)),
            Value::Table(mut table) => Ok(Self::try_new(&mut table, None)?),
            other => Err(not_valid("entries", "`string` or `table`", &other)),
        }
    }

    fn try_new(table: &mut Table, name: Option<String>) -> anyhow::Result<Self> {
        let run: String =
            table
                .remove("run")
                .map(Value::try_into)
                .context(if let Some(ref name) = name {
                    format!("menu.{} has no `run` value", name)
                } else {
                    "entry has no `run` value".to_string()
                })??;
        let group = table
            .remove("group")
            .map(Value::try_into)
            .transpose()?
            .unwrap_or(0);
        let name = name.unwrap_or_else(|| run.clone());

        Ok(Self { name, run, group })
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
