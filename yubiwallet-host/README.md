# Browser native host

Build an optimized host with PC/SC development headers installed:

```sh
cargo build --locked --release -p yubiwallet-host
```

On Debian/Ubuntu, install `libpcsclite-dev`, `pkg-config`, `pcscd` and
`pinentry-qt`. Start the PC/SC service using your system's service manager.
Use an already configured card; installing the host does not generate keys or
change OpenPGP/PIV slots. macOS needs its PC/SC setup and a GUI pinentry such as
`pinentry-mac`; these platform installation steps are not hardware-tested here.

The host defaults to `pinentry-qt` for local GUI PIN entry. For another GUI
implementation, set `YUBIWALLET_PINENTRY` to its executable path in the host's
launch environment. A browser already running may not inherit variables set in
a terminal. A small launcher can select the executable without containing a PIN:

```sh
#!/bin/sh
export YUBIWALLET_PINENTRY=/usr/bin/pinentry-gnome3
exec /absolute/path/to/yubiwallet-host
```

Make the launcher executable, then register it for your extension:

```sh
integration/native-messaging/install.sh YOUR_EXTENSION_ID /absolute/path/to/launcher
```

Use the extension ID shown by the browser, and restart the extension/browser
connection after installing. The installer writes only native host manifests;
it does not configure the card. The host returns `get_info` without touching a
card. Account discovery and signing require a working PC/SC/card setup.

PIN entry uses a separate local GUI process. Cancelling, an empty PIN or a
pinentry error returns `PIN_FAILED`; the host does not automatically try another
PIN or switch prompts. The PIN is not sent to the extension, placed in command
arguments or included in error logs. Native host stdout contains only the framed
JSON protocol. GUI pinentry needs access to your graphical login session.

`YUBIWALLET_PIN_MODE=tty` explicitly selects a controlling-terminal prompt with
echo disabled; browser-launched hosts normally have no controlling terminal.
`YUBIWALLET_PIN` is an explicit test-only override, including rejection of an
empty value. Do not put a real PIN in a launcher or use that override for normal
browser signing.

Hardware-free checks:

```sh
cargo fmt -p yubiwallet-host --check
cargo test --locked -p yubiwallet-host
cargo clippy --locked -p yubiwallet-host --all-targets -- -D warnings
```

Tests use fake pinentry subprocesses and protocol vectors, plus the real host's
`get_info`/unknown-method frames. They do not discover cards, guess PINs, generate
keys or sign. Physical card/browser signing still requires separate validation.
