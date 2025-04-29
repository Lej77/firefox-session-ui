# Firefox session UI

A desktop application for inspecting session storage data from Firefox.

Note that this program simply makes use of the code exposed by the CLI tool at <https://github.com/Lej77/firefox_session_data>.

## Usage

The desktop application can be downloaded from the assets in [latest GitHub release](https://github.com/Lej77/firefox-session-ui/releases).

You can also try the web version at <https://lej77.github.io/firefox-session-ui/>.

To use the app you need to browse and select a Firefox sessionstore file usually stored inside your Firefox profile directory and named `sessionstore.jsonlz4` (when Firefox has exited gracefully) or `sessionstore-backups/recovery.jsonlz4`  (when Firefox is still running). The desktop application has a helpful "Wizard" button that helps with finding the sessionstore file.

Useful links about Firefox's sessionstore file:

- [How do I backup a session (all the open tabs) so that it can be reloaded after a computer factory reset? | Firefox Support Forum | Mozilla Support](https://support.mozilla.org/en-US/questions/1257866)
- [Back up and restore information in Firefox profiles | Firefox Help](https://support.mozilla.org/en-US/kb/back-and-restore-information-firefox-profiles)
- [How to restore a browsing session from backup | Firefox Help](https://support.mozilla.org/en-US/kb/how-restore-browsing-session-backup)
- [Sessionstore.js - MozillaZine Knowledge Base](https://kb.mozillazine.org/index.php?title=Sessionstore.js&redirect=no)

## Build from source

### Faster installation of CLI tools

Use [`cargo-binstall`](https://crates.io/crates/cargo-binstall) instead of `cargo install` to not have to build from source. (It has quick install alternatives for itself as well.)

### Build as website

#### Dioxus CLI

Install the [Dioxus CLI](https://github.com/DioxusLabs/dioxus/tree/master/packages/cli) (`cargo install dioxus-cli`) and then run:

```shell
dx serve --platform web
```

or package this project:

```shell
dx build --platform web --release
```

#### Trunk

Install [Trunk](https://trunkrs.dev/) (`cargo install --locked trunk`) and then run:

```shell
trunk serve
```

or package this project:

```shell
trunk build --release
```

### Build as desktop program

#### Using Dioxus

```shell
cargo run
```

or install [cargo-watch](https://crates.io/crates/cargo-watch) to rebuild on changes:

```shell
cargo watch -x run
```

Finally, to package for release run:

```shell
cargo build --release
```

or use the [Dioxus CLI](https://github.com/DioxusLabs/dioxus/tree/master/packages/cli) (`cargo install dioxus-cli`) and run:

```shell
dx build --release --platform desktop
```

#### Using Tauri

Tauri builds a desktop program that uses a website as its UI. This option will
therefore build Dioxus as a WebAssembly frontend and load it as a website when the program
is started.

- Pro: some state is stored in the Tauri backend so after recompiling the Dioxus frontend we can resume what we were doing.
- Pro: the window is not closed when there is only changes in the frontend (less annoying popups when developing).
- Pro: Tauri's CLI and "framework" makes it easy to build an installer and provide auto updaters and similar advanced features.
- Pro: the WebAssembly Dioxus frontend is likely more lightweight which should improve incremental build times.
  - This is especially the case if there is a large native library that isn't included in the web frontend.
- Con: can't easily access file system and so on from the frontend code, need to use Tauri "commands" to offload such work to the backend.
- Con: slightly more complicated build setup compared to just starting a normal Rust project.

##### Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).

##### Build project as Tauri desktop application

Install Rust, see [Prerequisites | Tauri Apps](https://tauri.app/v1/guides/getting-started/prerequisites) for more details.

Then install the Tauri CLI using `cargo install tauri-cli`.

<!-- To build the frontend you also need to install the [Dioxus CLI](https://github.com/DioxusLabs/cli) using `cargo install dioxus-cli`. -->

To build the frontend you also need to install [Trunk](https://trunkrs.dev/) (`cargo install --locked trunk`) or [Dioxus CLI](https://github.com/DioxusLabs/dioxus/tree/master/packages/cli) (`cargo install dioxus-cli`).

After that you can use `cargo tauri dev` for quick debug builds and `cargo tauri build` for release builds.

## Project Structure

```text
.firefox-session-ui
- .vscode # VS Code configuration files
- public # Assets that should be included in the project.
- dist # Temp folder for web frontend.
- src # Frontend code and Dioxus app code.
- src-tauri # Code that runs on the host.
- - host_commands # Shared code used by dioxus desktop frontend and tauri backend.
- - src # Tauri backend that creates window and loads dioxus WebAssembly frontend.
```

## License

This project is released under either:

- [MIT License](./LICENSE-MIT)
- [Apache License (Version 2.0)](./LICENSE-APACHE)

at your choosing.

Note that some optional dependencies might be under different licenses.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
