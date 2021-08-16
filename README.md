# Dmenu Manager

Dmenu wrapper allowing the use of a toml file to configure dmenu.

See the [example config](example.toml) for a full explanation of the config options.
Below is a minimal configuration.

`dmenu-manager ~/config.toml`
``` toml
# ~/config.toml
[menu]
#name = "command"
say-hi = "echo 'Hello, world!'"
first = { run = "echo first", group = 1 }
browser = "firefox"
music = "vlc ~/music"

[config]
dmenu.prompt = "example:"
```
