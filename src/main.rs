// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod file_picker;

use std::{collections::VecDeque, fmt::Debug, future::Future};

use dioxus::prelude::*;
use file_picker::{OpenFilePicker, SaveFilePicker};
use host_commands::{
    DataId, FileManagementCommands, FileSlot, FileStatus, FirefoxProfileInfo, GenerateOptions,
    OutputFormat, OutputOptions, PathId, StatelessCommands,
};
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(any(feature = "dioxus-desktop", feature = "wasm-standalone"))]
use host_commands::host::HostCommands as Commands;
#[cfg(not(any(feature = "dioxus-desktop", feature = "wasm-standalone")))]
use host_commands::WasmClient as Commands;

#[cfg(any(feature = "dioxus-desktop", feature = "wasm-standalone"))]
pub fn ui_state() -> &'static std::sync::Mutex<host_commands::host::UiState> {
    use std::sync::OnceLock;

    static STATE: OnceLock<std::sync::Mutex<host_commands::host::UiState>> = OnceLock::new();

    STATE.get_or_init(|| {
        std::sync::Mutex::new(host_commands::host::UiState {
            #[cfg(target_family = "wasm")]
            handle_saved_data: Box::new(save_file_on_web_target),
            ..Default::default()
        })
    })
}
#[cfg(not(any(feature = "dioxus-desktop", feature = "wasm-standalone")))]
pub fn ui_state() {}

#[cfg(target_family = "wasm")]
pub fn get_context() {}
#[cfg(not(target_family = "wasm"))]
pub fn get_context() -> dioxus_desktop::DesktopContext {
    dioxus_desktop::use_window()
}

/// Save some data to a file and download it via the user's browser.
///
/// # References
///
/// <https://stackoverflow.com/questions/54626186/how-to-download-file-with-javascript>
/// <https://stackoverflow.com/questions/44147912/arraybuffer-to-blob-conversion>
#[cfg(target_family = "wasm")]
fn save_file_on_web_target(data: Vec<u8>, file_ext: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;

    let byte_array = js_sys::Uint8Array::new_with_length(
        data.len()
            .try_into()
            .map_err(|e| format!("Output file size was larger than a 32 bit number {e}"))?,
    );
    byte_array.copy_from(data.as_slice());
    let array = js_sys::Array::of1(&byte_array);
    let blob = web_sys::Blob::new_with_u8_array_sequence(&array)
        .ok()
        .ok_or("Blob creation failed")?;

    let a_tag: web_sys::HtmlAnchorElement = web_sys::window()
        .ok_or("no global window")?
        .document()
        .ok_or("no \"window.document\"")?
        .create_element("a")
        .map_err(|_| "failed to create \"a\" tag")?
        .unchecked_into();

    a_tag.set_download(&format!("firefox-tabs.{file_ext}"));

    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .ok()
        .ok_or("url creation failed")?;

    a_tag.set_href(&url);

    a_tag.click();

    web_sys::Url::revoke_object_url(&url)
        .ok()
        .ok_or("url revoke failed")?;

    Ok(())
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    /// <https://tauri.app/v1/api/js/clipboard/>
    #[wasm_bindgen(catch, js_name = "writeText", js_namespace = ["window", "__TAURI__", "clipboard"])]
    async fn write_text_to_tauri_clipboard(text: &str) -> Result<(), wasm_bindgen::JsValue>;

    /// <https://developer.mozilla.org/en-US/docs/Web/API/Clipboard/writeText>
    #[wasm_bindgen(catch, js_name = "writeText", js_namespace = ["navigator", "clipboard"])]
    async fn write_text_to_web_clipboard(text: &str) -> Result<(), wasm_bindgen::JsValue>;
}
#[cfg(target_family = "wasm")]
async fn write_text_to_clipboard(text: &str) -> Result<(), String> {
    if host_commands::has_host_access() {
        write_text_to_tauri_clipboard(text)
            .await
            .map_err(|e| e.as_string().unwrap_or_default())
    } else {
        write_text_to_web_clipboard(text)
            .await
            .map_err(|e| e.as_string().unwrap_or_default())
    }
}

#[cfg(not(target_family = "wasm"))]
static CLIPBOARD: std::sync::Mutex<Option<arboard::Clipboard>> = std::sync::Mutex::new(None);
#[cfg(not(target_family = "wasm"))]
async fn write_text_to_clipboard(text: &str) -> Result<(), String> {
    let mut guard = CLIPBOARD.lock().unwrap();
    let clipboard = if let Some(clipboard) = &mut *guard {
        clipboard
    } else {
        let clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        guard.insert(clipboard)
    };
    clipboard.set_text(text).map_err(|e| e.to_string())?;
    Ok(())
}

/// Returned by [`use_elm`]
pub struct ElmChannel<M: 'static> {
    inner: Signal<VecDeque<M>>,
}
impl<M: 'static> ElmChannel<M> {
    pub fn send(&mut self, msg: M) {
        self.inner.write().push_back(msg);
    }
}
impl<M: 'static> Clone for ElmChannel<M> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<M: 'static> Copy for ElmChannel<M> {}

/// Structure application according to [`The Elm Architecture`] by defining an
/// `update` function that mutates the program `state` after receiving a
/// message.
///
/// The Dioxus component that calls this hook should define the `view` of the
/// application.
///
/// [`The Elm Architecture`]:
///     https://en.wikipedia.org/wiki/Elm_(programming_language)#The_Elm_Architecture
pub fn use_elm<S, M>(
    init_state: impl FnOnce(ElmChannel<M>) -> S,
    mut update: impl FnMut(&mut S, M, ElmChannel<M>),
) -> (ReadOnlySignal<S>, ElmChannel<M>)
where
    M: Debug,
{
    let mut channel = ElmChannel {
        inner: use_signal::<VecDeque<M>>(VecDeque::new),
    };
    let mut state = use_signal(|| {
        log::debug!("Initializing state for {}", std::any::type_name::<S>());
        init_state(channel)
    });
    if !channel.inner.read().is_empty() {
        while let Some(msg) = {
            let mut guard = channel.inner.write();
            guard.pop_front()
        } {
            log::debug!(
                "Updating {} with message {msg:?}",
                std::any::type_name::<S>()
            );
            update(&mut *state.write(), msg, channel);
        }
    }
    (state.into(), channel)
}

fn main() {
    #[cfg(target_family = "wasm")]
    {
        // init debug tool for WebAssembly
        wasm_logger::init(wasm_logger::Config::new(if cfg!(debug_assertions) {
            // Might need to change the browser inspector to actually see these messages:
            log::Level::Trace
        } else {
            log::Level::Info
        }));
        console_error_panic_hook::set_once();

        // Show dialog if app panics (otherwise just logs to console and silently stops working):
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Log error to console:
            previous(info);
            // Then open an alert window so the user notices the issue:
            if let Some(win) = web_sys::window() {
                let _ = win.alert_with_message(&format!(
                    "App panicked and will now stop working:\n{info}"
                ));
            }
        }));

        log::info!(
            "Running inside Tauri window: {}",
            host_commands::has_host_access()
        );
        log::info!(
            "WebView has a save file picker: {}",
            file_picker::has_web_view_file_picker()
        );

        dioxus_web::launch::launch_cfg(App, Default::default());
    }

    #[cfg(not(target_family = "wasm"))]
    {
        use dioxus_desktop::{Config, WindowBuilder};

        fn start_app() -> Element {
            dioxus::dioxus_core::prelude::use_drop(|| {
                eprintln!("Window closing");
                if let Ok(mut guard) = CLIPBOARD.lock() {
                    *guard = None; // drop the clipboard
                }
            });
            App()
        }

        #[cfg(feature = "blitz")]
        {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to start tokio");

            let _guard = rt.enter();
            dioxus_native::launch(start_app);
        }
        #[cfg(not(feature = "blitz"))]
        {
            dioxus_desktop::launch::launch(
                start_app,
                Vec::new(),
                vec![Box::new(Config::new().with_window({
                    let mut builder =
                        WindowBuilder::new().with_title("Firefox Session Data Utility");
                    #[cfg(windows)]
                    {
                        use dioxus_desktop::tao::platform::windows::IconExtWindows;
                        use dioxus_desktop::tao::window::Icon;

                        builder = builder.with_window_icon(Some(
                            Icon::from_resource(1, None).expect("failed to load icon"),
                        ));
                    }
                    #[cfg(not(windows))]
                    {
                        // TODO: include icon
                    }

                    builder
                }))],
            );
        }
    }
}

/// Imports the CSS style sheet. Only needed when building for desktop;
/// otherwise the CSS is bundled with the website.
#[component]
fn StyleRef() -> Element {
    log::trace!("Rendering StyleRef");
    host_commands::const_cfg!(if cfg!(all(target_family = "wasm", feature = "trunk")) {
        // Trunk bundles the stylesheet separately (see trunk.html) so don't
        // include it here.
        rsx! {}
    } else if cfg!(target_family = "wasm") {
        // Note: assets only work if we build with dioxus-cli (the "dx" binary)
        rsx! {
            // Info from: https://dioxuslabs.com/learn/0.6/cookbook/tailwind
            // The Stylesheet component inserts a style link into the head of the document
            document::Stylesheet {
                // Urls are relative to your Cargo.toml file
                href: asset!("/public/style.css")
            }
        }
    } else if cfg!(not(debug_assertions)) {
        // Embed the stylesheet inside the binary, this is the most portable but
        // makes it hard to hot-reload style changes
        rsx! {
            style { {include_str!("../public/style.css")} }
        }
    } else {
        // On desktop targets we can read files and so can simply read the
        // stylesheet ourselves and poll it for changes.
        use std::borrow::Cow;

        // This is hard coded in release mode:
        let style_text_resource = resource::resource_str!("public/style.css");

        let initial_value: Cow<_> = style_text_resource.clone().into();
        let mut style_text = use_signal::<Cow<_>>(|| initial_value);

        use_future(move || {
            to_owned![style_text_resource];
            async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    if style_text_resource.reload_if_changed() {
                        log::trace!("Reloaded CSS style sheet");
                        style_text.set(style_text_resource.clone().into());
                    }
                }
            }
        });

        rsx! { style { "{style_text}" } }
    })
}

#[derive(PartialEq, Props, Clone)]
struct WindowSelectProps {
    open_windows: Vec<String>,
    closed_windows: Vec<String>,
    selected_open_windows: Vec<u32>,
    selected_closed_windows: Vec<u32>,
    /// Will be called with selected indexes for open windows and closed windows
    /// whenever the selection changes.
    on_change: Option<EventHandler<(Vec<u32>, Vec<u32>)>>,
}

/// A list of windows in the loaded session. Allows selecting some of the
/// windows in the list to only show some windows in the output.
#[component]
fn WindowSelect(props: WindowSelectProps) -> Element {
    log::trace!("Rendering WindowSelect");
    let WindowSelectProps {
        open_windows,
        closed_windows,
        selected_open_windows,
        selected_closed_windows,
        on_change,
    } = props;

    rsx! {
        select {
            id: "window-select",
            name: "windows",
            multiple: true,
            onchange: move |evt| {
                log::debug!("multi select event: {evt:?}");
                let (values_wasm, values_desktop);
                let values = if cfg!(target_family = "wasm") {
                    values_wasm = evt.values();
                    let slice = values_wasm
                        .get("options")
                        .map(|v| v.as_slice())
                        .unwrap_or_default();
                    slice.iter().map(String::as_str).collect::<Vec<&str>>()
                } else {
                    values_desktop = evt.value();
                    values_desktop.split(',').collect::<Vec<&str>>()
                };
                log::debug!("Changed which windows are selected to: {values:?}");
                let mut open_ix = Vec::new();
                let mut closed_ix = Vec::new();
                for value in values {
                    if value.is_empty() {
                        continue;
                    }
                    let (is_closed, ix) = if let Some(v) = value.strip_prefix("Window ") {
                        (false, v)
                    } else if let Some(v) = value.strip_prefix("Closed window ") {
                        (true, v)
                    } else {
                        log::warn!("Malformed value in window select: {value}");
                        continue;
                    };
                    match ix.parse::<u32>() {
                        Err(e) => log::warn!("Malformed index in window select \"{ix}\": {e}"),
                        Ok(ix) => {
                            if is_closed {
                                closed_ix.push(ix - 1);
                            } else {
                                open_ix.push(ix - 1);
                            }
                        }
                    }
                }
                log::trace!(
                    "Changed window filter\n\tOpen window indexes: {open_ix:?}\n\tClosed window indexes: {closed_ix:?}"
                );
                if let Some(on_change) = on_change {
                    on_change((open_ix, closed_ix));
                }
            },
            for (ix , window) in open_windows.iter().enumerate() {
                option {
                    value: "Window {ix + 1}",
                    selected: Some(selected_open_windows.contains(&(ix as u32))),
                    "{window}"
                }
            }
            if !closed_windows.is_empty() {
                option { value: "", disabled: true, "" }
                option { value: "", disabled: true, "Closed Windows:" }
            }
            for (ix , window) in closed_windows.iter().enumerate() {
                option {
                    value: "Closed window {ix + 1}",
                    selected: Some(selected_closed_windows.contains(&(ix as u32))),
                    "{window}"
                }
            }
        }
    }
}

#[derive(PartialEq, Props, Clone)]
struct InputPanelProps {
    /// Path to where new data will be loaded. This can be changed if the user
    /// manually types in the GUI.
    input_path: String,
    /// The file path where the current data was read from.
    loaded_file_path: String,
    /// The `input_path` has been manually edited and this change should be sent
    /// to the backend if accepted.
    on_input_path_edit: Option<EventHandler<String>>,
    /// The `input_path` has changed in the backend because the user browsed and
    /// selected a file.
    on_input_path_changed: Option<EventHandler<PathId>>,
    /// The user has requested that the file path that has been entered into
    /// `input_path` should now be loaded.
    on_load_new_data: Option<EventHandler<()>>,
    on_open_wizard: Option<EventHandler<()>>,
}

/// Configure where the sessionstore file is loaded from.
#[component]
fn InputPanel(props: InputPanelProps) -> Element {
    log::trace!("Rendering InputPanel");
    let InputPanelProps {
        input_path,
        loaded_file_path,
        on_input_path_edit,
        on_input_path_changed,
        on_load_new_data,
        on_open_wizard,
    } = props;

    rsx! {
        div { class: "file-input contains-columns",
            label { r#for: "file-path-to-load", "Path to sessionstore file:" }
            input {
                id: "file-path-to-load",
                r#type: "text",
                // Can't read file from arbitrary location inside a web page:
                readonly: Some(true).filter(|_| !host_commands::has_host_access()),
                disabled: Some(true).filter(|_| !host_commands::has_host_access()),
                value: "{input_path}",
                oninput: move |evt| {
                    let new_path = evt.value();
                    log::trace!("Manually modified input path to: {new_path}");
                    on_input_path_edit.inspect(|f| f(new_path));
                },
            }
            if host_commands::has_host_access() {
                button {
                    title: "Open a \"software wizard\"/\"setup assistant\" to help you select a Firefox sessionstore file.",
                    style: "margin-right: 5px;",
                    onclick: move |_| {
                        log::debug!("Requested wizard to pick input file",);
                        on_open_wizard.inspect(|f| f(()));
                    },
                    "Wizard"
                }
            }
            OpenFilePicker {
                on_input: move |v| {
                    log::debug!("Selected input path, id={v:?}");
                    on_input_path_changed.inspect(|f| f(v));
                },
                "Browse"
            }
        }
        div { class: "file-input contains-columns",
            label { r#for: "loaded-file-path", "Current data was loaded from:" }
            input {
                id: "loaded-file-path",
                r#type: "text",
                readonly: true,
                disabled: true,
                value: "{loaded_file_path}",
            }
            button {
                onclick: move |_| {
                    log::debug!("Requested to load new data from input path",);
                    on_load_new_data.inspect(|f| f(()));
                },
                "Load new data"
            }
        }
    }
}

#[derive(PartialEq, Props, Clone)]
struct OutputPanelProps {
    output_options: OutputOptions,
    format_info: Vec<(OutputFormat, String)>,
    output_path: String,
    on_overwrite_change: Option<EventHandler<bool>>,
    on_create_folder_change: Option<EventHandler<bool>>,
    on_output_format_change: Option<EventHandler<OutputFormat>>,
    /// User manually edited the save file path. If this change is accepted then
    /// it should be sent to the backend.
    on_output_path_edit: Option<EventHandler<String>>,
    /// User browsed to a new save file path. The backend has already saved the
    /// new path.
    on_output_path_changed: Option<EventHandler<String>>,
    on_copy_to_clipboard: Option<EventHandler<()>>,
    on_write_to_file: Option<EventHandler<()>>,
}

/// Handle configuration of output format and path and has a button to start
/// writing links from the sessionstore to a file or to the user's clipboard.
#[component]
fn OutputPanel(props: OutputPanelProps) -> Element {
    log::trace!("Rendering OutputPanel");

    let OutputPanelProps {
        output_options,
        format_info,
        output_path,
        on_overwrite_change,
        on_create_folder_change,
        on_output_format_change,
        on_output_path_edit,
        on_output_path_changed,
        on_copy_to_clipboard,
        on_write_to_file,
    } = props;

    let get_title_for_format = |format: OutputFormat| {
        format_info
            .iter()
            .find(|(f, _)| *f == format)
            .map(|(_, t)| t.as_str())
            .filter(|desc| !desc.is_empty())
    };

    rsx! {
        div { class: "contains-rows output-settings", style: "margin: 8px;",
            if cfg!(any(not(target_family = "wasm"), not(feature = "wasm-standalone"))) {
                // Show file path selection on native targets or for Tauri frontend
                div { class: "file-input contains-columns",
                    label { r#for: "output-path", "File path to write links to:" }
                    input {
                        id: "output-path",
                        r#type: "text",
                        value: "{output_path}",
                        oninput: move |evt| {
                            let new_path = evt.value();
                            log::trace!("Manually modified output path to: {new_path}");
                            on_output_path_edit.inspect(|f| f(new_path));
                        },
                    }
                    SaveFilePicker {
                        on_input: move |v| {
                            log::trace!("Selected new output path: {v}");
                            on_output_path_changed.inspect(|f| f(v));
                        },
                        "Browse"
                    }
                }
                div { class: "contains-columns",
                    div { class: "contains-columns",
                        input {
                            r#type: "checkbox",
                            id: "create-output-folder",
                            checked: "{output_options.create_folder}",
                            onchange: move |e| {
                                log::trace!("Clicked on create folder checkbox {e:?}");
                                on_create_folder_change.inspect(|f| f(e.checked()));
                            },
                        }
                        label { r#for: "create-output-folder", "Create folder if it doesn't exist" }
                    }
                    div {
                        class: "contains-columns",
                        style: "margin-left: 10px;",
                        input {
                            r#type: "checkbox",
                            id: "overwrite-output-file",
                            checked: "{output_options.overwrite}",
                            onchange: move |e| {
                                log::trace!("Clicked on overwrite checkbox {e:?}");
                                on_overwrite_change.inspect(|f| f(e.checked()));
                            },
                        }
                        label { r#for: "overwrite-output-file", "Overwrite file if it already exists" }
                    }
                }
            }
            div { class: "spacer", style: "flex: 0 1 auto; height: 5px;" }
            div { class: "contains-columns",
                button {
                    onclick: move |_| {
                        on_copy_to_clipboard.inspect(|f| f(()));
                    },
                    "Copy links to clipboard"
                }
                div { class: "spacer", style: "flex: 1 1 auto;" }
                fieldset {
                    class: "contains-rows output-format-group output-format-drop-down",
                    style: "margin: 6px;",
                    title: get_title_for_format(output_options.format),
                    legend { "Output format" }
                    select {
                        id: "output-format",
                        style: "flex: 1 1 auto;",
                        onchange: move |evt| {
                            let Some(on_output_format_change) = &on_output_format_change else {
                                return;
                            };
                            let value = evt.value();
                            if let Some(&format) = OutputFormat::all().iter().find(|f| f.as_str() == value) {
                                on_output_format_change(format);
                            }
                        },
                        for format in OutputFormat::all().iter().copied() {
                            option {
                                value: format.as_str(),
                                selected: Some(output_options.format == format),
                                title: get_title_for_format(format),
                                "{format.as_str()}"
                            }
                        }
                    }
                }
                fieldset {
                    class: "contains-columns output-format-group output-format-radio-buttons",
                    style: "margin: 6px; align-items: baseline; align-self: stretch;",
                    legend { "Output format" }
                    input {
                        r#type: "radio",
                        name: "output-format",
                        id: "output-format-text",
                        checked: Some(output_options.format == OutputFormat::TEXT),
                        title: get_title_for_format(OutputFormat::TEXT),
                        onclick: move |_| {
                            log::trace!("Checked Text output format");
                            on_output_format_change.inspect(|f| f(OutputFormat::TEXT));
                        },
                    }
                    label {
                        r#for: "output-format-text",
                        title: get_title_for_format(OutputFormat::TEXT),
                        "Text"
                    }
                    div { class: "spacer", style: "flex: 1 1 auto;" }
                    input {
                        r#type: "radio",
                        name: "output-format",
                        id: "output-format-html",
                        checked: Some(output_options.format == OutputFormat::HTML),
                        title: get_title_for_format(OutputFormat::HTML),
                        onclick: move |_| {
                            log::trace!("Checked HTML output format");
                            on_output_format_change.inspect(|f| f(OutputFormat::HTML));
                        },
                    }
                    label {
                        r#for: "output-format-html",
                        title: get_title_for_format(OutputFormat::HTML),
                        "HTML"
                    }
                    div { class: "spacer", style: "flex: 1 1 auto;" }
                    input {
                        r#type: "radio",
                        name: "output-format",
                        id: "output-format-rtf",
                        checked: Some(output_options.format == OutputFormat::RTF),
                        title: get_title_for_format(OutputFormat::RTF),
                        onclick: move |_| {
                            log::trace!("Checked RTF output format");
                            on_output_format_change.inspect(|f| f(OutputFormat::RTF));
                        },
                    }
                    label {
                        r#for: "output-format-rtf",
                        title: get_title_for_format(OutputFormat::RTF),
                        "Rich Text Format"
                    }
                    div { class: "spacer", style: "flex: 1 1 auto;" }
                    input {
                        r#type: "radio",
                        name: "output-format",
                        id: "output-format-pdf",
                        checked: Some(output_options.format == OutputFormat::PDF),
                        title: get_title_for_format(OutputFormat::PDF),
                        onclick: move |_| {
                            log::trace!("Checked PDF output format");
                            on_output_format_change.inspect(|f| f(OutputFormat::PDF));
                        },
                    }
                    label {
                        r#for: "output-format-pdf",
                        title: get_title_for_format(OutputFormat::PDF),
                        "PDF"
                    }
                }
                button {
                    onclick: move |_| {
                        on_write_to_file.inspect(|f| f(()));
                    },
                    "Save links to file"
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    SetInputPath(String),
    UpdateInputPath(PathId),
    SyncInputPath(String, PathId),
    OpenWizard,
    CloseWizard,
    FetchedFirefoxProfiles(Vec<FirefoxProfileInfo>),
    SyncLoadedPath(String, PathId),
    SetPreview(String),
    LoadInputPath(String),
    LoadNewData,
    SetTabGroups {
        open: Vec<String>,
        closed: Vec<String>,
        open_selected: Vec<u32>,
        closed_selected: Vec<u32>,
    },
    SetSelectedTabGroups {
        open: Vec<u32>,
        closed: Vec<u32>,
    },
    SetOutputPath(String),
    /// Backend changed its output path.
    SyncOutputPath(String),
    SetOverwrite(bool),
    SetCreateFolder(bool),
    SetOutputFormat(OutputFormat),
    SetStatus(String),
    FetchedOutputFormatInfo(Vec<(OutputFormat, String)>),
    CopyLinksToClipboard,
    WriteLinksToFile,
}

#[derive(Debug)]
pub struct State {
    input_path: String,
    input_path_id: PathId,
    loaded_path: String,
    loaded_path_id: PathId,
    preview: String,
    save_path: String,
    output_options: OutputOptions,
    open_window_groups: Vec<String>,
    closed_window_groups: Vec<String>,
    selected_open_window_groups: Vec<u32>,
    selected_closed_window_groups: Vec<u32>,
    status: String,
    format_info: Vec<(OutputFormat, String)>,
    wizard: bool,
    wizard_profiles: Vec<FirefoxProfileInfo>,
}
impl State {
    pub fn init(mut sender: ElmChannel<Message>) -> Self {
        // Restore state from backend in case Tauri frontend website was reloaded:
        spawn(async move {
            if let Some(save_path) = Commands.get_save_path(ui_state()).await {
                log::info!("Save path at startup: {save_path}");
                sender.send(Message::SyncOutputPath(save_path));
            }

            let info_new = Commands.get_info_for_slot(ui_state(), FileSlot::New).await;
            log::info!("Input/New path id at startup: {:?}", info_new.path_id);
            if info_new.path_id != PathId::null() {
                if info_new.data_id != DataId::null() {
                    log::info!(
                        "Reset data for input/new file slot, to not use more RAM than needed"
                    );
                    Commands.forget_data(ui_state(), info_new.data_id).await;
                }
                sender.send(Message::SyncInputPath(
                    info_new.file_path.unwrap_or_default(),
                    info_new.path_id,
                ));
            }

            let current = Commands
                .get_info_for_slot(ui_state(), FileSlot::Current)
                .await;
            log::info!("Loaded/Current path id at startup: {:?}", current.path_id);
            if current.path_id != PathId::null() {
                sender.send(Message::SyncLoadedPath(
                    current.file_path.unwrap_or_default(),
                    current.path_id,
                ));
                // Regenerate preview:
                sender.send(Message::SetSelectedTabGroups {
                    open: vec![],
                    closed: vec![],
                });
            }
        });
        spawn(async move {
            sender.send(Message::FetchedOutputFormatInfo(
                Commands.format_descriptions().await,
            ));
        });

        Self {
            input_path: String::new(),
            input_path_id: Default::default(),
            loaded_path: Default::default(),
            loaded_path_id: Default::default(),
            preview: String::new(),
            save_path: String::new(),
            output_options: Default::default(),
            open_window_groups: Vec::new(),
            closed_window_groups: Vec::new(),
            selected_open_window_groups: Vec::new(),
            selected_closed_window_groups: Vec::new(),
            status: String::new(),
            format_info: OutputFormat::all()
                .iter()
                .map(|&f| (f, String::new()))
                .collect(),
            wizard: false,
            wizard_profiles: Vec::new(),
        }
    }
    fn generate_preview(&self, mut sender: ElmChannel<Message>) -> impl Future<Output = ()> {
        log::trace!("Creating preview future");

        let loaded_path_id = self.loaded_path_id;
        let mut open_window_groups = self.open_window_groups.clone();
        let mut closed_window_groups = self.closed_window_groups.clone();
        let mut selected_open_window_groups = self.selected_open_window_groups.clone();
        let mut selected_closed_window_groups = self.selected_closed_window_groups.clone();

        let fut = async move {
            log::trace!("Generating preview!");
            if loaded_path_id == PathId::null() {
                // Just started
                log::trace!("Generating preview -> No path selected");
                return Ok::<_, String>(None::<String>);
            };
            let id = loaded_path_id;

            let new_info = Commands.get_info_for_slot(ui_state(), FileSlot::New).await;
            if new_info.path_id == id {
                log::trace!("Generating preview -> Commit new path id ({id:?}) so that data related to it won't be lost when we select another file");
                // Ensure that selecting a new file won't cancel what we do after this point:
                Commands.commit_new_file(ui_state()).await;
                let new_info = Commands.get_info_for_slot(ui_state(), FileSlot::New).await;
                // After commit the path id of the new file slot will have changed:
                sender.send(Message::SyncInputPath(
                    new_info.file_path.unwrap_or_default(),
                    new_info.path_id,
                ));
            }

            let mut info = Commands
                .get_info_for_path_id(ui_state(), id)
                .await
                .ok_or("file id has expired")?;

            let id = if info.data_id == DataId::null() {
                log::trace!("Generating preview -> Reading file data");
                sender.send(Message::SetStatus("Reading input data".to_owned()));
                let id = if host_commands::has_host_access() {
                    Commands.load_data(ui_state(), id).await?
                } else {
                    log::trace!("Generating preview -> Using old input data (no host access)");
                    Commands
                        .get_info_for_path_id(ui_state(), id)
                        .await
                        .ok_or("Path id has expired")?
                        .data_id
                };

                open_window_groups.clear();
                closed_window_groups.clear();
                selected_open_window_groups.clear();
                selected_closed_window_groups.clear();

                sender.send(Message::SetTabGroups {
                    open: Vec::new(),
                    closed: Vec::new(),
                    open_selected: Vec::new(),
                    closed_selected: Vec::new(),
                });

                info = Commands
                    .get_info_for_data_id(ui_state(), id)
                    .await
                    .ok_or("file id has expired")?;
                id
            } else {
                info.data_id
            };
            if matches!(info.status, FileStatus::Compressed) {
                sender.send(Message::SetStatus("Decompressing".to_owned()));
                Commands.decompress_data(ui_state(), id).await?;
            }
            if !matches!(info.status, FileStatus::Parsed) {
                sender.send(Message::SetStatus("Parsing".to_owned()));
                Commands.parse_session_data(ui_state(), id).await?;
            }

            let groups = Commands
                .get_groups_from_session(ui_state(), id, true)
                .await?;

            log::info!("Groups in loaded session {groups:#?}");

            // These won't change if we have the same DataId:
            let open_windows: Vec<_> = groups.open.iter().map(|g| g.name.clone()).collect();
            let closed_windows: Vec<_> = groups.closed.iter().map(|g| g.name.clone()).collect();
            if open_windows != open_window_groups || closed_windows != closed_window_groups {
                selected_open_window_groups.clear(); // = (0..open_windows.len() as u32).collect();
                selected_closed_window_groups.clear();
                sender.send(Message::SetTabGroups {
                    open: open_windows,
                    closed: closed_windows,
                    open_selected: selected_open_window_groups.clone(),
                    closed_selected: Vec::new(),
                });
            }

            sender.send(Message::SetStatus("Generating output".to_owned()));

            let has_any_filter = !selected_open_window_groups.is_empty()
                || !selected_closed_window_groups.is_empty();

            let links = Commands
                .to_text_links(
                    ui_state(),
                    id,
                    GenerateOptions {
                        open_group_indexes: Some(selected_open_window_groups)
                            .filter(|_| has_any_filter),
                        closed_group_indexes: Some(selected_closed_window_groups),
                        ..Default::default()
                    },
                )
                .await?;

            sender.send(Message::SetStatus(
                "Successfully loaded session data!".to_owned(),
            ));
            Ok(Some(links))
        };

        struct StatusGuard(Option<ElmChannel<Message>>);
        impl Drop for StatusGuard {
            fn drop(&mut self) {
                if let Some(channel) = &mut self.0 {
                    channel.send(Message::SetStatus(
                        "Background work was cancelled unexpectedly".to_string(),
                    ));
                }
            }
        }

        // Wrap the above future to handle errors:
        async move {
            // Set status if canceled:
            let mut guard = StatusGuard(Some(sender));

            sender.send(Message::SetPreview("".to_string()));
            match fut.await {
                Ok(Some(v)) => sender.send(Message::SetPreview(v)),
                Ok(None) => {}
                Err(e) => {
                    sender.send(Message::SetStatus(format!("Error: {e}")));
                }
            }
            guard.0.take();
        }
    }
    pub fn update(&mut self, msg: Message, mut sender: ElmChannel<Message>) {
        match msg {
            Message::SetInputPath(new_path) => {
                self.input_path.clone_from(&new_path);
                spawn(async move {
                    let new_id = Commands
                        .set_open_path(ui_state(), FileSlot::New, new_path.clone())
                        .await;
                    sender.send(Message::SyncInputPath(new_path, new_id));
                });
            }
            Message::UpdateInputPath(id) => {
                spawn(async move {
                    if let Some(info) = Commands.get_info_for_path_id(ui_state(), id).await {
                        sender.send(Message::SyncInputPath(
                            info.file_path.unwrap_or_default(),
                            id,
                        ));
                    };
                });
            }
            Message::SyncInputPath(input_path, path_id) => {
                self.input_path = input_path;
                self.input_path_id = path_id;
            }
            Message::OpenWizard => {
                self.wizard = true;
                spawn(async move {
                    match Commands.find_firefox_profiles().await {
                        Ok(profiles) => sender.send(Message::FetchedFirefoxProfiles(profiles)),
                        Err(e) => {
                            sender.send(Message::SetStatus(format!(
                                "Failed to gather info about firefox profiles: {e}"
                            )));
                            sender.send(Message::CloseWizard);
                        }
                    }
                });
            }
            Message::CloseWizard => {
                self.wizard = false;
            }
            Message::FetchedFirefoxProfiles(profiles) => {
                self.wizard_profiles = profiles;
            }
            Message::SyncLoadedPath(loaded_path, path_id) => {
                self.loaded_path = loaded_path;
                self.loaded_path_id = path_id;
            }
            Message::LoadNewData => {
                self.loaded_path_id = self.input_path_id;
                self.loaded_path.clone_from(&self.input_path);
                // TODO: cancellation
                spawn(self.generate_preview(sender));
            }
            Message::LoadInputPath(new_path) => {
                self.input_path.clone_from(&new_path);
                spawn(async move {
                    let new_id = Commands
                        .set_open_path(ui_state(), FileSlot::New, new_path.clone())
                        .await;
                    sender.send(Message::SyncInputPath(new_path, new_id));
                    sender.send(Message::LoadNewData);
                });
            }
            Message::SetPreview(preview) => {
                self.preview = preview;
            }
            Message::SetOutputPath(save_path) => {
                self.save_path.clone_from(&save_path);
                spawn(async move {
                    Commands.set_save_path(ui_state(), save_path).await;
                });
            }
            Message::SyncOutputPath(save_path) => {
                self.save_path = save_path;
            }
            Message::SetOverwrite(overwrite) => {
                self.output_options.overwrite = overwrite;
            }
            Message::SetCreateFolder(create_folder) => {
                self.output_options.create_folder = create_folder;
            }
            Message::SetOutputFormat(format) => {
                self.output_options.format = format;
            }
            Message::SetTabGroups {
                open,
                closed,
                open_selected,
                closed_selected,
            } => {
                self.open_window_groups = open;
                self.closed_window_groups = closed;
                self.selected_open_window_groups = open_selected;
                self.selected_closed_window_groups = closed_selected;
            }
            Message::SetSelectedTabGroups { open, closed } => {
                self.selected_open_window_groups = open;
                self.selected_closed_window_groups = closed;
                // TODO: cancellation
                spawn(self.generate_preview(sender));
            }
            Message::SetStatus(status) => {
                self.status = status;
            }
            Message::FetchedOutputFormatInfo(info) => {
                self.format_info = info;
            }
            Message::CopyLinksToClipboard => {
                let preview = self.preview.clone();
                spawn(async move {
                    if let Err(e) = write_text_to_clipboard(&preview).await {
                        sender.send(Message::SetStatus(format!(
                            "Failed to copy links to clipboard: {e}"
                        )));
                    }
                });
            }
            Message::WriteLinksToFile => {
                let options = self.output_options.clone();
                let open_group_indexes = self.selected_open_window_groups.clone();
                let closed_group_indexes = self.selected_closed_window_groups.clone();
                let has_any_filter =
                    !open_group_indexes.is_empty() || !closed_group_indexes.is_empty();
                log::info!("Saving links with {options:?}");
                spawn(async move {
                    sender.send(Message::SetStatus("Saving links".to_owned()));
                    let save_path = if cfg!(any(
                        not(target_family = "wasm"),
                        not(feature = "wasm-standalone")
                    )) {
                        // Use specified save path for native and Tauri frontend:

                        let Some(save_path) = Commands.get_save_path(ui_state()).await else {
                            sender.send(Message::SetStatus(
                                "Failed to save links: no save path selected".to_owned(),
                            ));
                            return;
                        };
                        sender.send(Message::SetStatus(format!("Saving links to {}", save_path)));
                        save_path
                    } else {
                        String::new()
                    };

                    let current = Commands
                        .get_info_for_slot(ui_state(), FileSlot::Current)
                        .await;
                    if let Err(e) = Commands
                        .save_links(
                            ui_state(),
                            current.data_id,
                            GenerateOptions {
                                open_group_indexes: Some(open_group_indexes)
                                    .filter(|_| has_any_filter),
                                closed_group_indexes: Some(closed_group_indexes),
                                ..Default::default()
                            },
                            options,
                        )
                        .await
                    {
                        sender.send(Message::SetStatus(format!(
                            "Failed to save links to file: {e}"
                        )));
                    } else if save_path.is_empty() {
                        sender.send(Message::SetStatus(
                            "Successfully saved links to a file".to_owned(),
                        ));
                    } else {
                        sender.send(Message::SetStatus(format!(
                            "Successfully saved links to a file at: {save_path}"
                        )));
                    }
                });
            }
        }
    }
}

#[component]
fn App() -> Element {
    log::trace!("Rendering App");

    let (state, mut sender) = use_elm(State::init, State::update);
    let state = state.read();

    let mut prev_wizard = use_signal(|| false);
    if prev_wizard() != state.wizard {
        prev_wizard.set(state.wizard);

        if state.wizard {
            dioxus::document::eval(
                r#"document.getElementById('find-session-data-wizard').showModal();"#,
            );
        } else {
            dioxus::document::eval(
                r#"
                document.getElementById('find-session-data-wizard').close();
                document.getElementById('wizard-select-firefox-profile').value = null;
                "#,
            );
        }
    }

    rsx! {
        StyleRef {}
        dialog {
            // TODO: allow clicking on backdrop to close dialog, see: https://stackoverflow.com/questions/25864259/how-to-close-the-new-html-dialog-tag-by-clicking-on-its-backdrop/72916231#72916231
            id: "find-session-data-wizard",
            // TODO: listen to onclose event if that is supported (its not in dioxus v0.5)
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    sender.send(Message::CloseWizard);
                }
            },
            div { class: "contains-rows",
                // method: "dialog",
                h2 { "Select Firefox Session Data" }
                p { "Firefox Profiles:" }
                select {
                    id: "wizard-select-firefox-profile",
                    size: Some(state.wizard_profiles.len() as i64),
                    style: "margin: 5px;",
                    onchange: move |evt| {
                        let file_path = evt.value();
                        sender.send(Message::LoadInputPath(file_path));
                        sender.send(Message::CloseWizard);
                    },
                    for profile in &state.wizard_profiles {
                        option {
                            title: profile
                                .session_files
                                .first()
                                .map(|v| v.file_path.as_str())
                                .unwrap_or(profile.file_path.as_str()),
                            value: profile.session_files.first().map(|v| v.file_path.as_str()).unwrap_or_default(),
                            "{profile.name}"
                        }
                    }
                }
                button {
                    onclick: move |_| {
                        sender.send(Message::CloseWizard);
                    },
                    "Cancel"
                }
            }
        }
        main { class: "contains-columns",
            WindowSelect {
                open_windows: state.open_window_groups.clone(),
                closed_windows: state.closed_window_groups.clone(),
                selected_open_windows: state.selected_open_window_groups.clone(),
                selected_closed_windows: state.selected_closed_window_groups.clone(),
                on_change: move |(open, closed)| {
                    sender
                        .send(Message::SetSelectedTabGroups {
                            open,
                            closed,
                        });
                },
            }
            div { class: "contains-rows", style: "flex: 1 1 auto;",
                InputPanel {
                    input_path: state.input_path.clone(),
                    loaded_file_path: state.loaded_path.clone(),
                    on_input_path_edit: move |path| {
                        sender.send(Message::SetInputPath(path));
                    },
                    on_input_path_changed: move |id| {
                        sender.send(Message::UpdateInputPath(id));
                    },
                    on_load_new_data: move |()| {
                        sender.send(Message::LoadNewData);
                    },
                    on_open_wizard: move |()| {
                        sender.send(Message::OpenWizard);
                    },
                }
                div { class: "contains-rows", style: "flex: 1 1 auto;",
                    label { "Tabs as links:" }
                    textarea {
                        id: "preview",
                        style: "flex: 1 1 auto; resize: none;",
                        readonly: true,
                        disabled: true,
                        value: state.preview.clone(),
                    }
                }
                OutputPanel {
                    output_options: state.output_options.clone(),
                    format_info: state.format_info.clone(),
                    output_path: state.save_path.clone(),
                    on_overwrite_change: move |overwrite| {
                        sender.send(Message::SetOverwrite(overwrite));
                    },
                    on_create_folder_change: move |create_folder| {
                        sender.send(Message::SetCreateFolder(create_folder));
                    },
                    on_output_format_change: move |new_format| {
                        sender.send(Message::SetOutputFormat(new_format));
                    },
                    on_output_path_edit: move |path| {
                        sender.send(Message::SetOutputPath(path));
                    },
                    on_output_path_changed: move |path| {
                        sender.send(Message::SyncOutputPath(path));
                    },
                    on_copy_to_clipboard: move |_| {
                        sender.send(Message::CopyLinksToClipboard);
                    },
                    on_write_to_file: move |_| {
                        sender.send(Message::WriteLinksToFile);
                    },
                }
                // Status Bar:
                div {
                    class: "contains-columns status-info",
                    style: "margin: 8px;",
                    label {
                        class: "vertically-centered-text",
                        style: "margin: 8px;",
                        "Status: "
                    }
                    input {
                        r#type: "text",
                        style: "flex: 1 1 auto;",
                        readonly: true,
                        disabled: true,
                        value: "{state.status}",
                    }
                }
            }
        }
    }
}
