//! Defines commands that should run with access to the host computer.

pub use tauri_commands;
pub use tauri_commands::{const_cfg, has_host_access};
use tauri_commands::{TauriDeserialize, TauriSerialize};

// To easier access this in Tauri main.rs:
pub use async_trait::async_trait;
#[cfg(any(feature = "tauri-export", feature = "dioxus-export", feature = "wasm-standalone"))]
pub use firefox_session_data;

#[TauriSerialize]
#[TauriDeserialize]
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct PathId(u64);
impl PathId {
    pub fn null() -> PathId {
        PathId(0)
    }
}

#[TauriSerialize]
#[TauriDeserialize]
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct DataId(u64);
impl DataId {
    pub fn null() -> DataId {
        DataId(0)
    }
}

#[TauriSerialize]
#[TauriDeserialize]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSlot {
    New,
    Current,
}
#[TauriSerialize]
#[TauriDeserialize]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum FileStatus {
    /// No path selected.
    #[default]
    Empty,
    /// Path selected so ready for reading.
    Found,
    /// Data loaded but it was compressed.
    Compressed,
    /// Data read and uncompressed.
    Uncompressed,
    /// Read data has been parsed as JSON.
    Parsed,
}

#[TauriSerialize]
#[TauriDeserialize]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileInfo {
    pub path_id: PathId,
    pub data_id: DataId,
    pub status: FileStatus,
    pub file_path: Option<String>,
}
impl std::fmt::Display for FileInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(file_path) = &self.file_path {
            write!(f, "{}", file_path)
        } else {
            Ok(())
        }
    }
}

#[TauriSerialize]
#[TauriDeserialize]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabGroup {
    pub index: u32,
    pub name: String,
}

#[TauriSerialize]
#[TauriDeserialize]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllTabGroups {
    pub open: Vec<TabGroup>,
    pub closed: Vec<TabGroup>,
}

#[TauriSerialize]
#[TauriDeserialize]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateOptions {
    pub open_group_indexes: Option<Vec<u32>>,
    pub closed_group_indexes: Option<Vec<u32>>,
    pub sort_groups: bool,
    pub table_of_content: bool,
    pub tree_style_tab_trees: bool,
    pub sidebery_trees: bool,
}
impl Default for GenerateOptions {
    fn default() -> Self {
        Self {
            open_group_indexes: None,
            closed_group_indexes: None,
            sort_groups: true,
            table_of_content: true,
            tree_style_tab_trees: true,
            sidebery_trees: true,
        }
    }
}

macro_rules! declare_formats {
    ($(
        $(#[default $(@ $default_:ident)?])?
        $(#[doc = $($attr:tt)*])*
        $format:ident = $as_str:literal
    ),* $(,)?) => {
        #[TauriSerialize]
        #[TauriDeserialize]
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        #[allow(non_camel_case_types)]
        pub enum OutputFormat {
            $(
                $(#[default $(@ $default_)?])?
                $(#[doc = $($attr)*])*
                $format,
            )*
        }
        impl OutputFormat {
            pub fn all() -> &'static [Self] {
                &[$(Self::$format,)*]
            }
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$format => $as_str,)*
                }
            }
        }
        #[cfg(any(feature = "tauri-export", feature = "dioxus-export", feature = "wasm-standalone"))]
        impl From<OutputFormat> for firefox_session_data::to_links::ttl_formats::FormatInfo {
            fn from(format: OutputFormat) -> Self {
                match format {
                    $(
                        OutputFormat::$format => Self::$format,
                    )*
                }
            }
        }
        #[cfg(any(feature = "tauri-export", feature = "dioxus-export", feature = "wasm-standalone"))]
        impl From<firefox_session_data::to_links::ttl_formats::FormatInfo> for OutputFormat {
            fn from(format: firefox_session_data::to_links::ttl_formats::FormatInfo) -> Self {
                match format {
                    $(
                        firefox_session_data::to_links::ttl_formats::FormatInfo::$format => Self::$format,
                    )*
                }
            }
        }
    };
}
declare_formats!(
    TEXT = "text",
    RTF = "rtf",
    RTF_SIMPLE = "rtf-simple",
    MARKDOWN = "markdown",
    HTML = "html",
    TYPST = "typst",

    #[default]
    PDF = "pdf",

    PDF_TYPST = "pdf-typst",

    PDF_MODERN = "pdf-modern",
    PDF_LEGACY = "pdf-legacy",
    PDF_XML_SIMPLE = "pdf-xml-simple",
    PDF_XML_ADV = "pdf-xml-adv",

    PDF_WK_HTML = "pdf-wk-html",
    PDF_WK_HTML_LINKED = "pdf-wk-html-linked",

    PDF_CHROMIUM_OXIDE = "pdf-chromium-oxide",
);

#[TauriSerialize]
#[TauriDeserialize]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputOptions {
    pub format: OutputFormat,
    pub overwrite: bool,
    pub create_folder: bool,
}
impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            format: Default::default(),
            overwrite: false,
            create_folder: false,
        }
    }
}

#[TauriSerialize]
#[TauriDeserialize]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirefoxProfileInfo {
    pub name: String,
    pub file_path: String,
    pub modified_at: Option<u64>,
    pub session_files: Vec<FoundSessionFile>,
}

#[TauriSerialize]
#[TauriDeserialize]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundSessionFile {
    pub name: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Copy)]
pub struct WasmClient;

#[tauri_commands::tauri_commands(wasm_client_impl_for = WasmClient)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
pub trait StatelessCommands {
    /// Get descriptions for all output formats.
    async fn format_descriptions(&self) -> Vec<(OutputFormat, String)>;

    async fn find_firefox_profiles(&self) -> Result<Vec<FirefoxProfileInfo>, String>;
}

#[tauri_commands::tauri_commands(wasm_client_impl_for = WasmClient)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
pub trait FileManagementCommands {
    type State<'a>;

    async fn get_info_for_slot(&self, state: Self::State<'_>, slot: FileSlot) -> FileInfo;
    async fn get_info_for_path_id(&self, state: Self::State<'_>, id: PathId) -> Option<FileInfo>;
    async fn get_info_for_data_id(&self, state: Self::State<'_>, id: DataId) -> Option<FileInfo>;

    async fn set_open_path(
        &self,
        state: Self::State<'_>,
        slot: FileSlot,
        file_path: String,
    ) -> PathId;

    async fn set_save_path(&self, state: Self::State<'_>, file_path: String);
    async fn get_save_path(&self, state: Self::State<'_>) -> Option<String>;

    async fn forget_data(&self, state: Self::State<'_>, id: DataId);
    async fn forget_path(&self, state: Self::State<'_>, id: PathId);

    /// Commit the data loaded into the [`FileSlot::New`] into [`FileSlot::Current`].
    async fn commit_new_file(&self, state: Self::State<'_>);

    /// Manually specify some data as loaded form a specific path. Usually
    /// prefer [`FileManagementCommands::load_data`].
    async fn set_data(&self, state: Self::State<'_>, id: PathId, data: Vec<u8>)  -> Result<DataId, String>;
    /// Read data from the selected file.
    async fn load_data(&self, state: Self::State<'_>, id: PathId) -> Result<DataId, String>;
    /// Decompress loaded data.
    async fn decompress_data(&self, state: Self::State<'_>, id: DataId) -> Result<(), String>;
    /// Parse uncompressed data as JSON.
    async fn parse_session_data(&self, state: Self::State<'_>, id: DataId) -> Result<(), String>;

    /// Get info about browser windows/groups from the parsed JSON data.
    async fn get_groups_from_session(
        &self,
        state: Self::State<'_>,
        id: DataId,
        sort_groups: bool,
    ) -> Result<AllTabGroups, String>;

    /// Generate text with links from JSON data.
    async fn to_text_links(
        &self,
        state: Self::State<'_>,
        id: DataId,
        generate_options: GenerateOptions,
    ) -> Result<String, String>;

    /// Generate document with links from JSON data and write to the save file.
    async fn save_links(
        &self,
        state: Self::State<'_>,
        id: DataId,
        generate_options: GenerateOptions,
        output_options: OutputOptions,
    ) -> Result<(), String>;
}

#[tauri_commands::tauri_commands(wasm_client_impl_for = WasmClient)]
#[cfg_attr(any(target_family = "wasm", not(feature = "tauri-export")), async_trait(?Send))]
#[cfg_attr(
    all(not(target_family = "wasm"), feature = "tauri-export"),
    async_trait
)]
pub trait FilePromptCommands {
    type Context;
    type State<'a>;

    /// Set the path to use when opening a file.
    async fn file_open(
        &self,
        state: Self::State<'_>,
        cx: Self::Context,
        slot: FileSlot,
    ) -> Option<PathId>;

    /// Change the current save location.
    async fn prompt_save_file(&self, state: Self::State<'_>, cx: Self::Context) -> Option<String>;
}

#[cfg(any(feature = "tauri-export", feature = "dioxus-export", feature = "wasm-standalone"))]
pub mod host;
