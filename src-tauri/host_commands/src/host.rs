use std::{
    borrow::Cow,
    fs::OpenOptions,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::UNIX_EPOCH,
};

use crate::{
    DataId, FileInfo, FileSlot, FileStatus, FirefoxProfileInfo, FoundSessionFile, OutputFormat,
    PathId, TabGroup,
};
use firefox_session_data::session_store::FirefoxSessionStore;
use tauri_commands::const_cfg;

/// A version of [`tokio::task::spawn_blocking`] that works for the WebAssembly
/// target where we don't have access to threads, in that case we simply block
/// the runtime (i.e. the event loop).
pub async fn spawn_blocking<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    if cfg!(target_family = "wasm") {
        f()
    } else {
        tokio::task::spawn_blocking(f).await.unwrap()
    }
}

#[derive(Debug)]
pub struct FileState {
    pub path_id: PathId,
    pub data_id: DataId,
    pub file_path: Option<PathBuf>,
    pub is_compressed: bool,
    pub data: Option<Arc<[u8]>>,
    pub session: Option<Arc<FirefoxSessionStore>>,
}
impl FileState {
    pub fn to_info(&self) -> FileInfo {
        FileInfo {
            file_path: self
                .file_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            path_id: self.path_id,
            data_id: self.data_id,
            status: if self.session.is_some() {
                FileStatus::Parsed
            } else if self.data.is_some() {
                if self.is_compressed {
                    FileStatus::Compressed
                } else {
                    FileStatus::Uncompressed
                }
            } else if self.file_path.is_some() {
                FileStatus::Found
            } else {
                FileStatus::Empty
            },
        }
    }
}
impl Default for FileState {
    fn default() -> Self {
        Self {
            path_id: PathId::null(),
            data_id: DataId::null(),
            file_path: None,
            is_compressed: true,
            data: None,
            session: None,
        }
    }
}

pub struct UiState {
    pub current_file: FileState,
    pub new_file: FileState,
    pub save_path: Option<PathBuf>,
    #[cfg(target_family = "wasm")]
    pub handle_saved_data: Box<dyn FnMut(Vec<u8>, &'static str) -> Result<(), String> + Send + 'static>,
}
impl std::fmt::Debug for UiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiState")
            .field("current_file", &self.current_file)
            .field("new_file", &self.new_file)
            .field("save_path", &self.save_path)
            .finish()
    }
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            current_file: Default::default(),
            new_file: Default::default(),
            // TODO: more robust finding of downloads folder.
            save_path: std::env::var("USERPROFILE")
                .map(|home| home + r"\Downloads\firefox-links")
                .map(Into::into)
                .ok(),
            #[cfg(target_family = "wasm")]
            handle_saved_data: Box::new(|_, _| Ok(())),
        }
    }
}
impl UiState {
    pub fn get_file_mut(&mut self, slot: FileSlot) -> &mut FileState {
        match slot {
            FileSlot::New => &mut self.new_file,
            FileSlot::Current => &mut self.current_file,
        }
    }
    pub fn get_file_for_path_id(&mut self, id: PathId) -> Option<&mut FileState> {
        if self.current_file.path_id == id {
            Some(&mut self.current_file)
        } else if self.new_file.path_id == id {
            Some(&mut self.new_file)
        } else {
            None
        }
    }
    pub fn get_file_for_data_id(&mut self, id: DataId) -> Option<&mut FileState> {
        if self.current_file.data_id == id {
            Some(&mut self.current_file)
        } else if self.new_file.data_id == id {
            Some(&mut self.new_file)
        } else {
            None
        }
    }
}

impl PathId {
    pub fn new() -> PathId {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        PathId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}
impl DataId {
    pub fn new() -> DataId {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        DataId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HostCommands;

#[cfg_attr(target_family = "wasm", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait::async_trait)]
impl super::StatelessCommands for HostCommands {
    async fn format_descriptions(&self) -> Vec<(OutputFormat, String)> {
        use firefox_session_data::to_links::ttl_formats::FormatInfo;
        OutputFormat::all()
            .iter()
            .map(|&f| (f, FormatInfo::from(f).to_string()))
            .collect()
    }
    async fn find_firefox_profiles(&self) -> Result<Vec<FirefoxProfileInfo>, String> {
        let finder = ::firefox_session_data::find::FirefoxProfileFinder::new()
            .map_err(|e| format!("{e}"))?;
        let profiles = finder.all_profiles().map_err(|e| format!("{e}"))?;
        Ok(profiles
            .iter()
            .filter_map(|(path, time)| {
                let potential = [
                    "sessionstore.jsonlz4",
                    "sessionstore-backups/recovery.jsonlz4",
                    "sessionstore-backups/recovery.baklz4",
                    "sessionstore-backups/previous.jsonlz4",
                ];
                let mut session_files: Vec<_> = potential
                    .into_iter()
                    .filter_map(|suffix| {
                        let path = path.join(suffix);
                        if path.exists() {
                            Some(FoundSessionFile {
                                name: path
                                    .file_name()
                                    .expect("Created path with file name")
                                    .to_str()?
                                    .to_owned(),
                                file_path: path.to_str()?.to_owned(),
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
                if session_files.is_empty() {
                    let recovery = potential[0].to_owned();
                    if let Some(file_path) = path.join(&recovery).to_str() {
                        // This is written most often so allow trying it later
                        // even if it doesn't exist right now:
                        session_files = vec![FoundSessionFile {
                            name: recovery,
                            file_path: file_path.to_owned(),
                        }];
                    }
                }
                Some(FirefoxProfileInfo {
                    name: path.file_name()?.to_str()?.to_owned(),
                    file_path: path.to_str()?.to_owned(),
                    modified_at: time
                        .as_ref()
                        .ok()
                        .map(|time| time.duration_since(UNIX_EPOCH).unwrap().as_secs()),
                    session_files,
                })
            })
            .collect())
    }
}

#[cfg_attr(any(target_family = "wasm", not(feature = "tauri-export")), async_trait::async_trait(?Send))]
#[cfg_attr(
    all(not(target_family = "wasm"), feature = "tauri-export"),
    async_trait::async_trait
)]
impl super::FilePromptCommands for HostCommands {
    type State<'a> = &'a Mutex<UiState>;

    #[cfg(all(not(target_family = "wasm"), feature = "tauri-export"))]
    type Context = tauri::Window;
    #[cfg(all(
        not(target_family = "wasm"),
        feature = "dioxus-export",
        not(feature = "tauri-export")
    ))]
    type Context = dioxus_desktop::DesktopContext;
    #[cfg(all(
        not(target_family = "wasm"),
        not(feature = "dioxus-export"),
        not(feature = "tauri-export")
    ))]
    type Context = ();
    #[cfg(target_family = "wasm")]
    type Context = ();

    async fn file_open(
        &self,
        state: Self::State<'_>,
        cx: Self::Context,
        slot: FileSlot,
    ) -> Option<PathId> {
        const_cfg!(if cfg!(any(
            target_family = "wasm",
            all(
                not(feature = "tauri-export"),
                not(feature = "dioxus-export")
            )
        )) {
            unimplemented!("the file_open command isn't implemented for this target");
        } else {
            use std::{env, path::PathBuf};
            #[cfg(feature = "tauri-export")]
            use tauri_plugin_dialog::DialogExt;

            #[cfg(feature = "tauri-export")]
            let (sender, receiver) = futures_channel::oneshot::channel();

            let mut builder = const_cfg!(if cfg!(feature = "tauri-export") {
                cx.dialog().file().set_parent(&cx)
            } else {
                rfd::AsyncFileDialog::new().set_parent(&**cx)
            })
            .add_filter("Firefox session file", &["js", "baklz4", "jsonlz4"])
            .add_filter("All files", &["*"])
            .set_title("Open Firefox Sessionstore File");
            if let Some(data) = env::var_os("APPDATA") {
                let data = PathBuf::from(data);
                builder = builder.set_directory(data.join("Mozilla\\Firefox\\Profiles"));
            }

            let file_path = const_cfg!(if cfg!(feature = "tauri-export") {
                builder.pick_file(move |file_path| {
                    sender.send(file_path).unwrap();
                });

                receiver.await.expect("sender dropped")?.into_path().ok()?
            } else {
                let handle = builder.pick_file().await?;
                handle.path().to_owned()
            });

            let mut guard = state.lock().unwrap();
            let file_info = guard.get_file_mut(slot);
            *file_info = Default::default();
            file_info.path_id = PathId::new();
            file_info.file_path = Some(file_path);
            Some(file_info.path_id)
        })
    }

    async fn prompt_save_file(&self, state: Self::State<'_>, cx: Self::Context) -> Option<String> {
        const_cfg!(if cfg!(any(
            target_family = "wasm",
            all(
                not(feature = "tauri-export"),
                not(feature = "dioxus-export")
            )
        )) {
            unimplemented!("the prompt_save_file command isn't implemented for this target");
        } else {
            #[cfg(feature = "tauri-export")]
            use tauri_plugin_dialog::DialogExt;

            #[cfg(feature = "tauri-export")]
            let (sender, receiver) = futures_channel::oneshot::channel();

            let builder = const_cfg!(if cfg!(feature = "tauri-export") {
                cx.dialog().file().set_parent(&cx)
            } else {
                rfd::AsyncFileDialog::new().set_parent(&**cx)
            })
            // .add_filter("All files", &["*"])
            .set_title("Save Links from Firefox Tabs");

            let path = const_cfg!(if cfg!(feature = "tauri-export") {
                builder.save_file(move |file_path| {
                    sender.send(file_path).unwrap();
                });
                receiver.await.expect("sender dropped")?.into_path().ok()?
            } else {
                let handle = builder.save_file().await?;
                handle.path().to_owned()
            });
            let path_str = path.to_string_lossy().into_owned();
            state.lock().unwrap().save_path = Some(path);
            Some(path_str)
        })
    }
}

#[cfg_attr(target_family = "wasm", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait::async_trait)]
impl super::FileManagementCommands for HostCommands {
    type State<'a> = &'a Mutex<UiState>;

    async fn get_info_for_slot(&self, state: Self::State<'_>, slot: FileSlot) -> FileInfo {
        state.lock().unwrap().get_file_mut(slot).to_info()
    }
    async fn get_info_for_path_id(&self, state: Self::State<'_>, id: PathId) -> Option<FileInfo> {
        Some(state.lock().unwrap().get_file_for_path_id(id)?.to_info())
    }
    async fn get_info_for_data_id(&self, state: Self::State<'_>, id: DataId) -> Option<FileInfo> {
        Some(state.lock().unwrap().get_file_for_data_id(id)?.to_info())
    }
    async fn set_open_path(
        &self,
        state: Self::State<'_>,
        slot: FileSlot,
        file_path: String,
    ) -> PathId {
        let mut guard = state.lock().unwrap();
        let file_info = guard.get_file_mut(slot);
        *file_info = Default::default();
        file_info.path_id = PathId::new();
        file_info.file_path = Some(file_path.into());
        file_info.path_id
    }
    async fn set_save_path(&self, state: Self::State<'_>, file_path: String) {
        let mut guard = state.lock().unwrap();
        guard.save_path = Some(file_path.into());
    }
    async fn get_save_path(&self, state: Self::State<'_>) -> Option<String> {
        let guard = state.lock().unwrap();
        guard
            .save_path
            .clone()
            .map(|p| p.to_string_lossy().into_owned())
    }

    async fn forget_data(&self, state: Self::State<'_>, id: DataId) {
        let mut guard = state.lock().unwrap();
        let Some(file_info) = guard.get_file_for_data_id(id) else {
            return;
        };
        *file_info = FileState {
            path_id: file_info.path_id,
            file_path: file_info.file_path.take(),
            ..Default::default()
        };
        #[cfg(debug_assertions)]
        {
            eprintln!("Forget data with {id:?}");
        }
    }
    async fn forget_path(&self, state: Self::State<'_>, id: PathId) {
        let mut guard = state.lock().unwrap();
        let Some(file_info) = guard.get_file_for_path_id(id) else {
            return;
        };
        *file_info = Default::default();
        #[cfg(debug_assertions)]
        {
            eprintln!("Forget path with {id:?}");
        }
    }
    async fn commit_new_file(&self, state: Self::State<'_>) {
        let mut guard = state.lock().unwrap();
        guard.current_file = std::mem::take(&mut guard.new_file);
        // Leave path but give it a new id to not cause confusion:
        guard.new_file.path_id = PathId::new();
        guard.new_file.file_path = guard.current_file.file_path.clone();
        #[cfg(debug_assertions)]
        {
            eprintln!("Commit new file");
        }
    }

    async fn set_data(
        &self,
        state: Self::State<'_>,
        id: PathId,
        data: Vec<u8>,
    ) -> Result<DataId, String> {
        let mut guard = state.lock().unwrap();

        let path = {
            let file_info = guard
                .get_file_for_path_id(id)
                .ok_or("path id has expired")?;

            file_info
                .file_path
                .as_ref()
                .ok_or("file hasn't been selected yet")?
                .clone()
        };

        let is_compressed = path
            .extension()
            .and_then(|ext| ext.to_str().map(|v| v.ends_with("lz4")))
            .unwrap_or(false);

        let file_info = guard
            .get_file_for_path_id(id)
            .ok_or("path id has expired")?;

        *file_info = FileState {
            file_path: file_info.file_path.clone(),
            is_compressed,
            data: Some(data.into()),
            data_id: DataId::new(),
            path_id: id,
            session: None,
        };
        Ok(file_info.data_id)
    }
    async fn load_data(&self, state: Self::State<'_>, id: PathId) -> Result<DataId, String> {
        use std::{
            fs::File,
            io::{BufReader, Read},
        };
        let path = {
            let mut guard = state.lock().unwrap();
            let file_info = guard
                .get_file_for_path_id(id)
                .ok_or("path id has expired")?;

            file_info
                .file_path
                .as_ref()
                .ok_or("file hasn't been selected yet")?
                .clone()
        };

        let (is_compressed, data) = spawn_blocking(move || -> Result<_, String> {
            let file = File::open(&path)
                .map_err(|e| format!("failed to open file at {}: {e}", path.display()))?;

            let mut buffer = BufReader::new(file);
            let mut data = Vec::new();

            buffer
                .read_to_end(&mut data)
                .map_err(|e| format!("failed to read file data from {}: {e}", path.display()))?;

            let is_compressed = path
                .extension()
                .and_then(|ext| ext.to_str().map(|v| v.ends_with("lz4")))
                .unwrap_or(false);

            Ok((is_compressed, data))
        })
        .await?;

        let mut guard = state.lock().unwrap();
        let file_info = guard
            .get_file_for_path_id(id)
            .ok_or("path id expired while reading file data")?;

        *file_info = FileState {
            file_path: file_info.file_path.clone(),
            is_compressed,
            data: Some(data.into()),
            data_id: DataId::new(),
            path_id: id,
            session: None,
        };
        Ok(file_info.data_id)
    }

    async fn decompress_data(&self, state: Self::State<'_>, id: DataId) -> Result<(), String> {
        use {either::Either, std::io::Empty};

        let data = {
            let mut guard = state.lock().unwrap();
            let host_data = guard
                .get_file_for_data_id(id)
                .ok_or("file id has expired")?;

            let data = host_data.data.clone().ok_or("file data not loaded")?;

            if !host_data.is_compressed {
                return Err("the data was already uncompressed".to_string());
            }
            data
        };
        let decompressed = spawn_blocking(move || {
            std::panic::catch_unwind(|| {
                firefox_session_data::io_utils::decompress_lz4_data(Either::<_, Empty>::Left(
                    Vec::<u8>::from(&*data).into(),
                ))
                .map(|reader| -> Vec<u8> { reader.into() })
                .map_err(|e| format!("failed to decompress data: {e}"))
            })
            .unwrap_or_else(|_| Err("decompression of sessionstore data panicked".to_string()))
        })
        .await?;

        let mut guard = state.lock().unwrap();
        let host_data = guard
            .get_file_for_data_id(id)
            .ok_or("file id expired while decompressing")?;
        host_data.data = Some(decompressed.into());
        host_data.is_compressed = false;
        Ok(())
    }

    async fn parse_session_data(&self, state: Self::State<'_>, id: DataId) -> Result<(), String> {
        use firefox_session_data::session_store::FirefoxSessionStore;
        use std::sync::Arc;

        let data = {
            let mut guard = state.lock().unwrap();
            let host_data = guard
                .get_file_for_data_id(id)
                .ok_or("file id has expired")?;

            let data = host_data.data.clone().ok_or("file data not loaded")?;

            if host_data.is_compressed {
                return Err("can't parse compressed data".to_string());
            }
            data
        };

        let session = spawn_blocking(move || {
            serde_json::from_slice::<FirefoxSessionStore>(&data)
                .map_err(|e| format!("failed to parse sessionstore JSON data: {e}"))
        })
        .await?;

        let mut guard = state.lock().unwrap();
        let host_data = guard
            .get_file_for_data_id(id)
            .ok_or("file id expired while parsing JSON")?;
        host_data.session = Some(Arc::new(session));
        host_data.data = None; // <- Free memory

        Ok(())
    }

    async fn get_groups_from_session(
        &self,
        state: Self::State<'_>,
        id: DataId,
        sort_groups: bool,
    ) -> Result<crate::AllTabGroups, String> {
        use firefox_session_data::session_store::session_info::get_groups_from_session;

        let session = state
            .lock()
            .unwrap()
            .get_file_for_data_id(id)
            .ok_or("file id has expired")?
            .session
            .clone()
            .ok_or("must deserialize JSON sessionstore data before tab groups can be inspected")?;

        Ok(spawn_blocking(move || crate::AllTabGroups {
            open: get_groups_from_session(&session, true, false, sort_groups)
                .enumerate()
                .map(|(ix, group)| TabGroup {
                    index: ix as _,
                    name: group.name().to_owned(),
                })
                .collect::<Vec<_>>(),
            closed: get_groups_from_session(&session, false, true, sort_groups)
                .enumerate()
                .map(|(ix, group)| TabGroup {
                    index: ix as _,
                    name: group.name().to_owned(),
                })
                .collect::<Vec<_>>(),
        })
        .await)
    }

    async fn to_text_links(
        &self,
        state: Self::State<'_>,
        id: DataId,
        generate_options: crate::GenerateOptions,
    ) -> Result<String, String> {
        use firefox_session_data::{
            pdf_converter::html_to_pdf::WriteBuilderSimple,
            session_store::{
                session_info::{get_groups_from_session, TreeDataSource},
                to_links::{LinkFormat, ToLinksOptions},
            },
            to_links::TabsToLinksOutput,
        };

        let session = state
            .lock()
            .unwrap()
            .get_file_for_data_id(id)
            .ok_or("file id has expired")?
            .session
            .clone()
            .ok_or("must deserialize JSON sessionstore data before converting tabs to links")?;

        spawn_blocking(move || {
            let mut output: Vec<u8> = Vec::new();

            let open_groups =
                get_groups_from_session(&session, true, false, generate_options.sort_groups)
                    .enumerate()
                    .filter(|(ix, _)| {
                        if let Some(indexes) = &generate_options.open_group_indexes {
                            indexes.contains(&(*ix as u32))
                        } else {
                            true
                        }
                    })
                    .map(|(_, g)| g);

            let closed_groups =
                get_groups_from_session(&session, false, true, generate_options.sort_groups)
                    .enumerate()
                    .filter(|(ix, _)| {
                        if let Some(indexes) = &generate_options.closed_group_indexes {
                            indexes.contains(&(*ix as u32))
                        } else {
                            true
                        }
                    })
                    .map(|(_, g)| g);

            firefox_session_data::tabs_to_links(
                &open_groups.chain(closed_groups).collect::<Vec<_>>(),
                TabsToLinksOutput {
                    format: LinkFormat::TXT,
                    as_pdf: None,
                    conversion_options: ToLinksOptions {
                        format: LinkFormat::TXT,
                        page_breaks_after_group: false,
                        skip_page_break_after_last_group: true,
                        table_of_contents: generate_options.table_of_content,
                        indent_all_links: true,
                        custom_page_break: "".into(),
                        // If there is any data from Sidebery then TST data
                        // won't be used and so on:
                        tree_sources: Cow::Borrowed(&[
                            TreeDataSource::Sidebery,
                            TreeDataSource::TstWebExtension,
                            TreeDataSource::TstLegacy,
                        ]),
                    },
                },
                WriteBuilderSimple(&mut output),
            )
            .map_err(|e| e.to_string())?;

            /// UTF 8 Byte Order Mark. Write to the beginning of a text file to indicate the text encoding of the data.
            const UTF_8_BOM: &[u8] = b"\xEF\xBB\xBF";

            let output = if output.starts_with(UTF_8_BOM) {
                &output[UTF_8_BOM.len()..]
            } else {
                output.as_slice()
            };

            Ok(String::from_utf8_lossy(output).into_owned())
        })
        .await
    }

    async fn save_links(
        &self,
        state: Self::State<'_>,
        id: DataId,
        generate_options: crate::GenerateOptions,
        output_options: crate::OutputOptions,
    ) -> Result<(), String> {
        use firefox_session_data::{
            pdf_converter::html_to_pdf::WriteBuilderSimple,
            session_store::{
                session_info::{get_groups_from_session, TreeDataSource},
                to_links::{LinkFormat, ToLinksOptions},
            },
            to_links::{ttl_formats::FormatInfo, TabsToLinksOutput},
        };

        let (mut save_path, session) = {
            let mut guard = state.lock().unwrap();
            let save_path = if cfg!(target_family = "wasm") {
                Default::default()
            } else {
                guard.save_path.clone().ok_or("no save path selected")?
            };
            let file = guard
                .get_file_for_data_id(id)
                .ok_or("file id has expired")?;
            let session = file
                .session
                .clone()
                .ok_or("must deserialize JSON sessionstore data before converting tabs to links")?;
            (save_path, session)
        };

        let _data = spawn_blocking(move || -> Result<_, String> {
            let (format, as_pdf) = FormatInfo::from(output_options.format)
                .as_format()
                .to_link_format();

            let file_ext = if as_pdf.is_some() {
                "pdf"
            } else {
                match format {
                    LinkFormat::TXT => "txt",
                    LinkFormat::RTF { .. } => "rtf",
                    LinkFormat::HTML => "html",
                    LinkFormat::Markdown => "md",
                    LinkFormat::Typst => "typ",
                }
            };

            let mut file = {
                #[cfg(target_family = "wasm")]
                {
                    Vec::new()
                }
                #[cfg(not(target_family = "wasm"))]
                {
                    if save_path.extension().is_none() {
                        save_path.set_extension(file_ext);
                    }

                    if let Some(folder) = save_path.parent() {
                        if output_options.create_folder {
                            std::fs::create_dir_all(folder).map_err(|e| {
                                format!("failed to create folder at \"{}\": {e}", folder.display())
                            })?;
                        }
                    }

                    OpenOptions::new()
                        .write(true)
                        .truncate(true)
                        .create(true)
                        .create_new(!output_options.overwrite)
                        .open(&save_path)
                        .map_err(|e| {
                            format!(
                                "failed to create new file at \"{}\": {e}",
                                save_path.display()
                            )
                        })?
                }
            };

            let open_groups =
                get_groups_from_session(&session, true, false, generate_options.sort_groups)
                    .enumerate()
                    .filter(|(ix, _)| {
                        if let Some(indexes) = &generate_options.open_group_indexes {
                            indexes.contains(&(*ix as u32))
                        } else {
                            true
                        }
                    })
                    .map(|(_, g)| g);

            let closed_groups =
                get_groups_from_session(&session, false, true, generate_options.sort_groups)
                    .enumerate()
                    .filter(|(ix, _)| {
                        if let Some(indexes) = &generate_options.closed_group_indexes {
                            indexes.contains(&(*ix as u32))
                        } else {
                            true
                        }
                    })
                    .map(|(_, g)| g);

            let page_breaks = !matches!(format, LinkFormat::TXT);
            let mut tree_sources = Vec::with_capacity(3);
            if generate_options.sidebery_trees {
                // Prefer first found source, so if there is any data from
                // Sidebery then TST data won't be used and so on.
                tree_sources.push(TreeDataSource::Sidebery);
            }
            if generate_options.tree_style_tab_trees {
                tree_sources.extend_from_slice(&[
                    TreeDataSource::TstWebExtension,
                    TreeDataSource::TstLegacy,
                ]);
            }

            firefox_session_data::tabs_to_links(
                &open_groups.chain(closed_groups).collect::<Vec<_>>(),
                TabsToLinksOutput {
                    format,
                    as_pdf,
                    conversion_options: ToLinksOptions {
                        format,
                        // No page break character for text files so fallback to
                        // several new lines:
                        page_breaks_after_group: page_breaks,
                        skip_page_break_after_last_group: page_breaks && (format.is_html() || format.is_typst()),
                        table_of_contents: generate_options.table_of_content,
                        indent_all_links: true,
                        custom_page_break: "".into(),
                        tree_sources: Cow::Owned(tree_sources),
                    },
                },
                WriteBuilderSimple(&mut file),
            )
            .map_err(|e| e.to_string())?;

            #[cfg(target_family = "wasm")]
            {
                Ok((file, file_ext))
            }
            #[cfg(not(target_family = "wasm"))]
            {
                Ok(())
            }
        })
        .await?;

        #[cfg(target_family = "wasm")]
        {
            let mut guard = state.lock().unwrap();
            (guard.handle_saved_data)(_data.0, _data.1)?;
        }

        Ok(())
    }
}
