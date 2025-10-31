use crate::Commands;
use dioxus::prelude::*;
use host_commands::{FileManagementCommands, FilePromptCommands, FileSlot, PathId};

#[derive(PartialEq, Props, Clone)]
pub struct OpenFilePickerProps {
    /// Invoked with the Id of a new file when the user selects a file using the
    /// browse button.
    on_input: EventHandler<PathId>,
    /// Text to show inside the button.
    children: Element,
}

/// A button that opens a file input prompt used to select a Firefox
/// sessionstore file.
#[component]
pub fn OpenFilePicker(props: OpenFilePickerProps) -> Element {
    let OpenFilePickerProps { on_input, children } = props;

    if host_commands::has_host_access() {
        rsx! {
            button { onclick: move |_| {
                    let fut =
                        Commands.file_open(
                            crate::ui_state(),
                            crate::get_context(),
                            FileSlot::New
                        );
                    spawn(async move {
                        if let Some(id) = fut.await {
                            on_input.call(id);
                        }
                    });
                },
                {children}
            }
        }
    } else {
        // Use input element for file browser
        rsx! {
            label { class: "custom-button",
                input {
                    r#type: "file",
                    style: "display: none",
                    oninput: move |e| {
                        let mut files = e.files();
                        if files.len() != 1 {
                            log::warn!("Expected a single file but found: {}", files.len());
                        }
                        let file = files.remove(0);
                        spawn(async move {
                            let id = Commands.set_open_path(crate::ui_state(), FileSlot::New, e.value()).await;
                            if let Ok(data) =  file.read_bytes().await {
                                if let Err(e) = Commands.set_data(crate::ui_state(), id, Vec::<u8>::from(data)).await {
                                    log::error!("Failed to set data for file: {e}");
                                }
                            }
                            on_input.call(id);
                        });
                    }
                }
                {children}
            }
        }
    }
}

#[cfg(target_family = "wasm")]
mod web_file_picker {
    //! Use the experimental [`showSaveFilePicker`] API.
    //!
    //! [`showSaveFilePicker`]:
    //!     https://developer.mozilla.org/en-US/docs/Web/API/Window/showSaveFilePicker

    use js_sys::{ArrayBuffer, Uint8Array};
    use wasm_bindgen::prelude::*;
    use web_sys::File;

    #[wasm_bindgen]
    extern "C" {
        pub type FileSystemFileHandle;

        pub type FileSystemWritableFileStream;

        pub type Window;

        #[wasm_bindgen(catch, static_method_of = Window, js_class = "window", js_name  = "showOpenFilePicker")]
        async fn _show_open_file_picker() -> Result<JsValue, JsValue>;

        #[wasm_bindgen(getter, catch, static_method_of = Window, js_class = "window", js_name  = "showSaveFilePicker")]
        pub fn get_show_save_file_picker() -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch, static_method_of = Window, js_class = "window", js_name  = "showSaveFilePicker")]
        async fn _show_save_file_picker() -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch, method, js_name = "createWritable")]
        async fn _create_writable(this: &FileSystemFileHandle) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch, method, js_name = "getFile")]
        async fn _get_file(this: &FileSystemFileHandle) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch, method)]
        pub async fn write(
            this: &FileSystemWritableFileStream,
            data: ArrayBuffer,
        ) -> Result<(), JsValue>;

        #[wasm_bindgen(catch, method)]
        pub async fn truncate(this: &FileSystemWritableFileStream, len: u32)
            -> Result<(), JsValue>;

        #[wasm_bindgen(catch, method)]
        pub async fn close(this: &FileSystemWritableFileStream) -> Result<(), JsValue>;
    }

    impl Window {
        pub async fn show_open_file_picker() -> Result<FileSystemFileHandle, JsValue> {
            Ok(Self::_show_open_file_picker().await?.unchecked_into())
        }
        pub async fn show_save_file_picker() -> Result<FileSystemFileHandle, JsValue> {
            Ok(Self::_show_save_file_picker().await?.unchecked_into())
        }
    }
    impl FileSystemFileHandle {
        pub async fn create_writable(&self) -> Result<FileSystemWritableFileStream, JsValue> {
            Ok(self._create_writable().await?.unchecked_into())
        }
        pub async fn get_file(&self) -> Result<File, JsValue> {
            Ok(self._get_file().await?.unchecked_into())
        }
    }
    impl FileSystemWritableFileStream {
        pub async fn write_all(&self, data: &[u8]) -> Result<(), JsValue> {
            let array = Uint8Array::new_with_length(
                data.len()
                    .try_into()
                    .expect("attempt to write more than 32bit number of data into file"),
            );
            array.copy_from(data);
            let buffer = array.buffer();
            self.write(buffer).await?;
            self.truncate(data.len() as u32).await?;
            self.close().await?;
            Ok(())
        }
    }

    pub fn has_save_file_picker() -> bool {
        matches!(Window::get_show_save_file_picker(), Ok(value) if value.is_function())
    }
}

#[cfg(not(target_family = "wasm"))]
mod web_file_picker {
    pub fn has_save_file_picker() -> bool {
        false
    }
}
pub use web_file_picker::has_save_file_picker as has_web_view_file_picker;

#[derive(PartialEq, Props, Clone)]
pub struct SaveFilePickerProps {
    /// Invoked with a file path when the user selects an output path using the
    /// browse button.
    on_input: EventHandler<String>,
    /// Text to show inside the button.
    children: Element,
}
/// A button that allows browsing after an output file path.
///
/// # Web
///
/// When targeting the web without Tauri commands this will attempt to use the
/// experimental
/// [`showSaveFilePicker`](https://developer.mozilla.org/en-US/docs/Web/API/Window/showSaveFilePicker)
/// API. If that isn't available then a disabled button will be shown.
#[component]
pub fn SaveFilePicker(props: SaveFilePickerProps) -> Element {
    let SaveFilePickerProps { on_input, children } = props;

    rsx! {
        button {
            disabled: Some(true).filter(|_| !host_commands::has_host_access() && !has_web_view_file_picker()),
            onclick: move |_| {
                let fut = host_commands::const_cfg!(
                    if cfg!(target_family = "wasm") {
                        if host_commands::has_host_access() {
                            // Tauri will preform the prompt:
                            let cx = crate::get_context();
                            Box::pin(async move {
                                Commands.prompt_save_file(crate::ui_state(), cx).await
                            }) as std::pin::Pin<Box<dyn std::future::Future<Output = _> + '_>>
                        } else {
                            // Use the web API to preform the prompt:
                            Box::pin(async {
                                use web_file_picker::*;
                                //use wasm_bindgen::JsCast;

                                let handle = Window::show_save_file_picker().await.ok()?;
                                let file = handle.get_file().await.ok()?;
                                let name = file.name();

                                Commands.set_save_path(crate::ui_state(), name.clone()).await;

                                //let buffer: js_sys::ArrayBuffer =
                                //    wasm_bindgen_futures::JsFuture::from(file.array_buffer())
                                //    .await
                                //    .map_err(|_e| log::error!("Failed to read file"))
                                //    .ok()?
                                //    .unchecked_into();
                                //let buffer = js_sys::Uint8Array::new(&buffer).to_vec();
                                //
                                //Commands.set_data(crate::ui_state(), todo!(), buffer).await.map_err(|_e| log::error!("Failed to read file")).ok()?;

                                Some(name)
                            })
                        }
                    } else {
                        Commands.prompt_save_file(crate::ui_state(), crate::get_context())
                    }
                );
                spawn(async move {
                    if let Some(info) = fut.await {
                        on_input.call(info);
                    }
                });
            },
            {children}
        }
    }
}
