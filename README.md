# DMM -- DMenu Manager

Easily create menus in `dmenu` to quickly launch programs and execute shell scripts.
The menu and other settings are defined in a [toml](https://toml.io/) formatted file.
These files are referred to as patterns.

## Installation

[Precompiled binaries of each version are available for x86_64 Linux.
](https://github.com/abysssol/dmm/releases)
These are static binaries, so they are compatible with all Linux distributions.

To install the binary:

1. Download the desired version.
2. Decompress `dmm`.
3. Give `dmm` executable permissions.
4. Move `dmm` to `~/bin`.

Execute the following in a terminal to install the latest stable version:

```sh
mkdir -p ~/bin
curl -L https://github.com/abysssol/dmm/releases/download/1.0.0/dmm-x86_64-linux.gz \
  | gzip -dc > ~/bin/dmm
chmod 744 ~/bin/dmm
```

Many distributions add `~/bin/` to `$PATH` by default, but not all do.
If `echo $PATH | grep -o ~/bin` outputs a path (like `/home/user/bin`), no action is needed.
However, if it doesn't output anything, you will need to add `~/bin/` to `$PATH` yourself.

In `fish`, run:

```fish
echo \n'set -gxp PATH ~/bin/' >> ~/.config/fish/config.fish
exec fish
```

In `bash`, run:

```bash
echo -e '\n''export PATH=$HOME/bin/:$PATH' >> ~/.bashrc
exec bash
```

In `zsh`, run:

```zsh
echo -e '\n''export PATH=$HOME/bin/:$PATH' >> ~/.zshrc
exec zsh
```

## Basics

Invoke `dmm` by giving it a path to a pattern.

```sh
dmm ~/example-pattern.toml
```

Below is a short example pattern.
See the [example config](./EXAMPLE.toml) for an explanation of all configuration options.

```toml
# ~/example-pattern.toml

[menu]
# name = "command"
"Say Hi" = "echo 'Hello, world!'"

# name = { run = "command", group = <number> }
first = { run = "echo 'first!'", group = 1 }
last = { run = "echo 'last ...'", group = -1 }

[config]
dmenu.prompt = "example:"
shell = [ "fish", "-c" ]
```

It can also have a pattern piped to it.

```sh
cat pattern.toml | dmm
```

```sh
echo 'config.path = true' | dmm
```

That last command will actually make `dmm` act much like `dmenu_run` does.
Setting `config.path = true` will cause `dmm` to search `$PATH` for all executables,
add them to the menu, and run them when selected.

## Configuration

A config file may be written to `~/.config/dmm/config.toml` on most systems.
See `dmm --home-config` for the directory that will be checked for config files on your system.

The format and options are the same as patterns.
Menu entries from the config and pattern are merged together.
All other config values are a default that can be overridden.

```toml
# ~/.config/dmm/config.toml

[config.dmenu]
font = "Hack Nerd Font:size=16"
background = "#101010"
foreground = "#f0f0f0"
selected-background = "#00ffc0"
selected-foreground = "#000000"
```

## License

This software is dedicated to the public domain under the [Creative Commons Zero
](https://creativecommons.org/publicdomain/zero/1.0/).
Read the CC0 in the [LICENSE file](./LICENSE) or [online
](https://creativecommons.org/publicdomain/zero/1.0/legalcode).

## Contribution

Any contribution intentionally submitted for inclusion in the project is subject to the CC0.
