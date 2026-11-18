# Maya Charge

Battery indicator for LAMZU Maya on Linux, with a Noctalia bar widget.
The battery reader is written in Rust and communicates through hidraw.

- Checks the battery every 5 seconds; click to refresh.
- Shows the charge percentage, charging state and voltage.
- Uses the current theme, with a low-battery warning at 20%.
- Shares one reader across all monitors and updates the UI only when values change.
- Shows `—` when the mouse is asleep, disconnected or unavailable.
- Keeps the kernel input drivers attached.

## Compatibility

Linux with Noctalia 5.3 or newer. The reader targets the CompX protocol used by
these USB IDs:

| Device | USB ID |
| --- | --- |
| LAMZU Pro 1K receiver | `3554:f50d` |
| Cable connection | `3554:f50f` |

The cable mode and firmware compatibility still need hardware testing.
Maya X and other receiver variants are not supported. The correct hidraw
interface is discovered automatically after each reconnect.

## Install

Requirements: Rust with Cargo, Python 3.11+, Noctalia and udev.
Python is used only for installation and development tools.

```sh
cargo fetch --locked
bash install.sh
```

Run the installer as your regular user. It builds the release binary, installs
the plugin and enables it in Noctalia. `sudo` is needed only to install or update
the udev rule. Running the same command again updates an existing installation.

The installer currently expects a bar named `default` with a `group:g1` entry
in its `start` list. It places the indicator immediately after that group:

```toml
[bar.default]
start = ["launcher", "clock", "spacer_1", "group:g1", "mouse_charge"]

[widget.mouse_charge]
type = "local/mouse-charge:battery"
```

For other layouts, install the plugin files and add the widget through Noctalia's
bar settings, or adjust the placement in `scripts/configure.py`.
The executable and plugin ID remain `mouse-charge` and `local/mouse-charge`
for compatibility with existing installations.

Config changes preserve unrelated settings and create timestamped `.bak` files.
To preview the changes without applying them:

```sh
python scripts/configure.py
```

Preview files are written to `target/noctalia-preview/`.
An unknown-widget warning before the plugin is installed is expected.

The enabled plugin starts with Noctalia. Configure Noctalia to start with your
Wayland session if it is not already in your compositor's autostart settings.
No separate daemon or systemd service is needed.

## Usage

```sh
~/.local/bin/mouse-charge
~/.local/bin/mouse-charge --json
~/.local/bin/mouse-charge --diagnose
```

`--json` emits a result even on failure. Missing battery readings use
`"percent": null`; a measured empty battery uses `0`. Exit codes are `0` for
a successful query or an offline mouse, `1` for a device error and `2` for
invalid arguments.

If access is denied, reconnect the receiver after installing the udev rule.
For timeouts, wake the mouse and close any vendor configuration software that
may be using the same device.

## Resource usage

The reader runs briefly for each query and exits between polls. It does not
write a cache or keep a device handle open. USB replies use blocking `poll()`
with a one-second timeout per command. Queries never overlap.

Measure the reader on your hardware with:

```sh
python scripts/benchmark.py
```

The benchmark makes 12 queries at five-second intervals and reports CPU time,
elapsed time and successful/error counts. It does not measure Noctalia rendering
or the effect of polling on the mouse's sleep behaviour and battery life.

## Uninstall

Remove the widget from the bar and disable the plugin:

```sh
python scripts/configure.py --apply --remove
noctalia msg plugins disable local/mouse-charge
noctalia msg config-reload
```

Installed files can then be removed from:

- `~/.local/bin/mouse-charge`
- `$XDG_DATA_HOME/noctalia/plugins/mouse-charge/` (default: `~/.local/share/`)
- `/etc/udev/rules.d/70-lamzu-maya-battery.rules`

Reload udev rules after removing the rule file: `sudo udevadm control --reload-rules`.

## Development

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
python -m unittest discover -s tests -v
lua tests/plugin.lua
noctalia plugins lint noctalia
bash -n install.sh
```

Tests cover packet framing, response parsing, timeouts, device selection,
config preservation and widget lifecycle. Transport tests use a packet pipe;
widget tests use a local Noctalia API mock. They do not replace hardware tests.

## References

- [CompX battery protocol](https://gist.github.com/Deamhan/a4cd8f7122510e3c301952313dc661e8)
- [Noctalia plugin documentation](https://docs.noctalia.dev/noctalia/v5.3.0/plugins/)
- [Noctalia runtime API](https://docs.noctalia.dev/noctalia/v5.3.0/plugins/development/runtime-api/)

## License

[MIT](LICENSE)
