use std::{
    collections, env,
    ffi::OsString,
    fs::{self, ReadDir},
    io::Write,
    panic,
    path::PathBuf,
    process::{self, Command, Stdio},
    thread,
};

use anyhow::{anyhow, Context};
use is_executable::IsExecutable;
use mimalloc::MiMalloc;

use config::{BinPath, Config, Custom, Entry, Numbered, Run, Shell};
use tag::{Binary, Decimal, Tag};

mod config;
mod tag;

type HashMap<K, V> = collections::HashMap<K, V, ahash::RandomState>;
type HashSet<T> = collections::HashSet<T, ahash::RandomState>;

#[global_allocator]
static GLOBAL_ALLOCATOR: MiMalloc = MiMalloc;

fn main() {
    if let Err(err) = run() {
        report_errors(&err);

        process::exit(1);
    }
}

fn report_errors(err: &anyhow::Error) {
    let mut chain = err.chain();
    let err = chain.next().unwrap_or_else(|| unreachable!());

    eprintln!("{} {err}", style_stderr!("error:", red bold));

    for err in chain {
        eprintln!("  {} {err}", style_stderr!("-", yellow bold));
    }
}

fn run() -> anyhow::Result<()> {
    let config = config::get()?;

    let commands = if let Numbered::Enabled(_) = config.numbered {
        get_selection::<Decimal>(&config)?
    } else {
        get_selection::<Binary>(&config)?
    };

    run_commands(&commands, &config)?;

    Ok(())
}

fn get_selection<T: Tag>(config: &Config) -> anyhow::Result<Vec<Run>> {
    let entries = build_entries(config)?;
    let menu_display = display_entries::<T>(config, &entries);
    let choices = run_dmenu(menu_display, &config.dmenu.args()).context("problem running dmenu")?;
    let choices = choices
        .split('\n')
        .filter(|choice| !choice.trim().is_empty());

    choices
        .map(|choice| {
            if let Some((id, _)) = T::pop_tag(choice) {
                let entry = entries
                    .get(id)
                    .expect("logic error: mismatch between entry tag and entry index");

                Ok(entry.run.clone())
            } else if let Custom::Enabled = config.custom {
                Ok(Run::Shell(choice.into()))
            } else {
                Err(anyhow!(
                    "ad-hoc commands are disabled; consider setting `config.custom = true`"
                ))
            }
        })
        .collect()
}

fn build_entries(config: &Config) -> anyhow::Result<Vec<RunEntry>> {
    fn walk_dir(
        dir: ReadDir,
        recur: &mut Vec<PathBuf>,
        files: &mut Vec<(OsString, String)>,
    ) -> anyhow::Result<()> {
        for entry in dir {
            let entry = entry.context("error trying to walk PATH directory")?;
            let filetype = entry.file_type().context("error reading file metadata")?;
            let follow_symlink_is_dir = || {
                fs::metadata(entry.path())
                    .context("error reading file metadata")
                    .map(|entry| entry.is_dir())
            };

            if filetype.is_dir() || follow_symlink_is_dir()? {
                recur.push(entry.path());
            } else if entry.path().is_executable() {
                files.push((
                    entry.path().into_os_string(),
                    entry.file_name().to_string_lossy().into_owned(),
                ));
            }
        }

        Ok(())
    }

    let mut entries = if let BinPath::Enabled {
        path,
        env,
        replace,
        recursive,
        group,
    } = &config.path
    {
        let mut entries = Vec::new();
        let mut menu_entries = config
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.name().to_owned(),
                    RunEntry::try_from(entry.clone(), !config.shell.is_enabled()),
                )
            })
            .collect::<HashMap<String, Option<RunEntry>>>();

        let env_paths = env.then(|| env::var_os("PATH")).flatten();
        let env_paths = env_paths
            .as_ref()
            .map(env::split_paths)
            .into_iter()
            .flatten();

        let paths = path
            .iter()
            .map(|pathstr| {
                if pathstr.starts_with("~/") {
                    let start = '~'.len_utf8() + '/'.len_utf8();
                    let mut path = PathBuf::new();
                    path.push(config.base_dirs.home_dir());
                    path.push(&pathstr[start..]);
                    path
                } else {
                    PathBuf::from(pathstr)
                }
            })
            .chain(env_paths);

        let path_bins = paths.filter_map(|path| {
            let mut files = Vec::new();
            let mut recur = Vec::new();

            match fs::read_dir(&path) {
                Ok(dir) => {
                    if let Err(err) = walk_dir(dir, &mut recur, &mut files) {
                        return Some(Err(err));
                    }
                }
                Err(_) => return None,
            }

            if *recursive {
                while let Some(path) = recur.pop() {
                    match fs::read_dir(&path) {
                        Ok(dir) => {
                            if let Err(err) = walk_dir(dir, &mut recur, &mut files) {
                                return Some(Err(err));
                            }
                        }
                        Err(_) => continue,
                    }
                }
            }

            Some(Ok(files))
        });

        for bins in path_bins {
            let bins = bins?;
            for (path, name) in bins {
                let path = path.into_string().map_err(|path| {
                    anyhow!(
                        "the path `{}` contained invalid unicode",
                        style_stderr!(path.to_string_lossy(), bold)
                    )
                })?;

                if menu_entries.contains_key(&name) {
                    if *replace {
                        let (key_name, entry) =
                            menu_entries.remove_entry(&name).expect("unreachable");
                        match entry {
                            Some(RunEntry { name, group, .. }) => {
                                menu_entries.insert(
                                    key_name,
                                    Some(RunEntry {
                                        name,
                                        run: Run::binary(path),
                                        group,
                                    }),
                                );
                            }
                            None => {
                                menu_entries.insert(key_name, None);
                            }
                        }
                    }
                } else {
                    entries.push(RunEntry {
                        name,
                        run: Run::binary(path),
                        group: *group,
                    });
                }
            }
        }

        entries.extend(menu_entries.into_iter().filter_map(|(_, entry)| entry));

        entries
    } else {
        config
            .entries
            .iter()
            .filter_map(|entry| RunEntry::try_from(entry.clone(), !config.shell.is_enabled()))
            .collect::<Vec<RunEntry>>()
    };

    entries.sort_unstable_by(|l, r| {
        let by_group = l.group.cmp(&r.group).reverse();
        let by_lowercase_name = || {
            l.name
                .to_ascii_lowercase()
                .cmp(&r.name.to_ascii_lowercase())
        };
        let by_name = || l.name.cmp(&r.name);

        by_group.then_with(by_lowercase_name).then_with(by_name)
    });

    Ok(entries)
}

fn display_entries<T: Tag>(config: &Config, entries: &[RunEntry]) -> String {
    let mut display = String::new();

    for (i, entry) in entries.iter().enumerate() {
        T::push_tag(i, &mut display);
        display.push_str(config.numbered.separator());
        display.push_str(&entry.name);
        display.push('\n');
    }

    display
}

fn run_dmenu(menu_display: String, dmenu_args: &[String]) -> anyhow::Result<String> {
    let mut dmenu = Command::new("dmenu")
        .args(dmenu_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context(format!(
            "failed to run command `{}` (is it installed?)",
            style_stderr!("dmenu", bold)
        ))?;
    let mut stdin = dmenu
        .stdin
        .take()
        .context("failed to establish pipe to dmenu??")?;

    let thread = thread::spawn(move || {
        stdin
            .write_all(menu_display.as_bytes())
            .context("failed to write to dmenu stdin??")
    });
    match thread.join() {
        Ok(result) => result?,
        Err(err) => panic::resume_unwind(err),
    }

    let output = dmenu
        .wait_with_output()
        .context("failed to read dmenu stdout??")?;

    Ok(String::from_utf8(output.stdout)?)
}

fn run_commands(commands: &[Run], config: &Config) -> anyhow::Result<()> {
    let mut errs = Vec::<anyhow::Error>::new();

    for command in commands {
        match command {
            Run::Bare(run) => {
                if let Some(bin) = run.first() {
                    let args = &run[1..];
                    let result = Command::new(bin).args(args).spawn().context(format!(
                        "couldn't run bare command `{}`",
                        style_stderr!(display_bare(run), bold)
                    ));

                    if let Err(err) = result {
                        errs.push(err);
                    }
                }
            }
            Run::Shell(run) => {
                if !run.is_empty() {
                    match &config.shell {
                        Shell::Disabled => errs.push(
                            anyhow!(
                                "shell execution is disabled; to enable, set `config.shell = true`"
                            )
                            .context(format!(
                                "can't execute shell command `{}`",
                                style_stderr!(run, bold)
                            )),
                        ),
                        Shell::Enabled { shell, piped } => {
                            if let Some(shell_name) = shell.first() {
                                let args = &shell[1..];
                                if !piped {
                                    let result = Command::new(shell_name)
                                        .args(args)
                                        .arg(run)
                                        .spawn()
                                        .context(format!(
                                            "problem running shell command `{}`",
                                            style_stderr!(run, bold)
                                        ));

                                    if let Err(err) = result {
                                        errs.push(err);
                                    }
                                } else {
                                    let mut shell = Command::new(shell_name)
                                        .args(args)
                                        .stdin(Stdio::piped())
                                        .stdout(Stdio::piped())
                                        .stderr(Stdio::piped())
                                        .spawn()
                                        .context(format!(
                                            "failed to run shell `{}` (is it installed?)",
                                            style_stderr!(shell_name, bold)
                                        ))?;
                                    let mut stdin = shell
                                        .stdin
                                        .take()
                                        .context("failed to establish pipe to shell??")?;

                                    stdin
                                        .write_all(run.as_bytes())
                                        .context("failed to write to shell stdin??")?;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn display_bare(run: &[String]) -> String {
    let mut buf = String::new();

    match run {
        [] => (),
        [run] => {
            buf.push_str(run);
        }
        [first, rest @ ..] => {
            buf.push_str(first);
            for option in rest {
                buf.push(' ');
                buf.push_str(option);
            }
        }
    }

    buf
}

macro_rules! style_stream {
    ($stream:ident, $string:expr, $($style:ident)+) => {
        {
            #[allow(unused_imports)]
            use owo_colors::OwoColorize;
            $string
                $(
                    .if_supports_color(owo_colors::Stream::$stream, owo_colors::OwoColorize::$style)
                )+
        }
    };
}

macro_rules! style_stdout {
    ($string:expr, $($style:ident)+) => {
        crate::style_stream!(Stdout, $string, $($style )+)
    };
}

macro_rules! style_stderr {
    ($string:expr, $($style:ident)+) => {
        crate::style_stream!(Stderr, $string, $($style )+)
    };
}

use style_stderr;
use style_stdout;
use style_stream;

#[derive(Debug, Clone)]
struct RunEntry {
    name: String,
    run: Run,
    group: i64,
}

impl RunEntry {
    fn try_from(entry: Entry, shell_is_enabled: bool) -> Option<Self> {
        match entry {
            Entry::Full { name, run, group } => Some(Self { name, run, group }),
            Entry::Name(name) => Some(Self {
                run: if shell_is_enabled {
                    Run::Shell(name.clone())
                } else {
                    Run::binary(name.clone())
                },
                name,
                group: 0,
            }),
            Entry::Filter(_) => None,
        }
    }
}
