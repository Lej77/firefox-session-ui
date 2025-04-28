// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use host_commands::*;
use std::sync::Mutex;

mod commands {
    use host_commands::*;
    use std::sync::Mutex;

    macro_rules! forward_arg {
        ("state", $arg:ident) => {
            let $arg = $arg.inner();
        };
        ($($other:tt)*) => {};
    }

    pub struct TauriCommands;

    #[tauri_commands::tauri_commands(
        delegate_empty_methods_to = host::HostCommands,
        delegate_args_using = forward_arg,
        fix_async_command_results = true,
        crate = "tauri_commands",
        module_path = commands,
    )]
    #[async_trait]
    impl FilePromptCommands for TauriCommands {
        type Context = tauri::Window;
        type State<'a> = tauri::State<'a, Mutex<host::UiState>>;

        async fn file_open(
            &self,
            state: Self::State<'_>,
            cx: Self::Context,
            slot: FileSlot,
        ) -> Option<PathId> {
        }

        async fn prompt_save_file(
            &self,
            state: Self::State<'_>,
            cx: Self::Context,
        ) -> Option<String> {
        }
    }

    #[tauri_commands::tauri_commands(
        delegate_empty_methods_to = host::HostCommands,
        delegate_args_using = forward_arg,
        fix_async_command_results = true,
        crate = "tauri_commands",
        module_path = commands,
    )]
    #[async_trait]
    impl FileManagementCommands for TauriCommands {
        type State<'a> = tauri::State<'a, Mutex<host::UiState>>;

        async fn get_info_for_slot(&self, state: Self::State<'_>, slot: FileSlot) -> FileInfo {}
        async fn get_info_for_path_id(
            &self,
            state: Self::State<'_>,
            id: PathId,
        ) -> Option<FileInfo> {
        }
        async fn get_info_for_data_id(
            &self,
            state: Self::State<'_>,
            id: DataId,
        ) -> Option<FileInfo> {
        }

        async fn set_open_path(
            &self,
            state: Self::State<'_>,
            slot: FileSlot,
            file_path: String,
        ) -> PathId {
        }
        async fn set_save_path(&self, state: Self::State<'_>, file_path: String) {}
        async fn get_save_path(&self, state: Self::State<'_>) -> Option<String> {}

        async fn forget_data(&self, state: Self::State<'_>, id: DataId) {}
        async fn forget_path(&self, state: Self::State<'_>, id: PathId) {}

        async fn commit_new_file(&self, state: Self::State<'_>) {}

        async fn set_data(&self, state: Self::State<'_>, id: PathId, data: Vec<u8>)  -> Result<DataId, String> {}
        async fn load_data(&self, state: Self::State<'_>, id: PathId) -> Result<DataId, String> {}
        async fn decompress_data(&self, state: Self::State<'_>, id: DataId) -> Result<(), String> {}
        async fn parse_session_data(
            &self,
            state: Self::State<'_>,
            id: DataId,
        ) -> Result<(), String> {
        }

        async fn get_groups_from_session(
            &self,
            state: Self::State<'_>,
            id: DataId,
            sort_groups: bool,
        ) -> Result<AllTabGroups, String> {
        }
        async fn to_text_links(
            &self,
            state: Self::State<'_>,
            id: DataId,
            generate_options: GenerateOptions,
        ) -> Result<String, String> {
        }
        async fn save_links(
            &self,
            state: Self::State<'_>,
            id: DataId,
            generate_options: GenerateOptions,
            output_options: OutputOptions,
        ) -> Result<(), String> {
        }
    }

    #[tauri_commands::tauri_commands(
        delegate_empty_methods_to = host::HostCommands,
        fix_async_command_results = true,
        crate = "tauri_commands",
        module_path = commands,
    )]
    #[async_trait]
    impl StatelessCommands for TauriCommands {
        async fn format_descriptions(&self) -> Vec<(OutputFormat, String)> {}
        async fn find_firefox_profiles(&self) -> Result<Vec<FirefoxProfileInfo>, String> {}
    }
}

tauri_commands::combine_commands!(with_all_commands,
    commands::with_commands_for_FilePromptCommands then
    commands::with_commands_for_FileManagementCommands then
    commands::with_commands_for_StatelessCommands
);

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command

#[allow(unused_mut)]
fn main() -> firefox_session_data::Result<()> {
    if std::env::args_os().nth(1).is_some() {
        // If called with arguments then behave like a CLI tool:
        return firefox_session_data::run();
    }

    // Build app:
    let mut builder = tauri::Builder::default()
        .manage(Mutex::new(host::UiState::default()))
        .invoke_handler(with_all_commands!(tauri::generate_handler));
    #[cfg(debug_assertions)]
    {
        builder = builder
            .on_page_load(|_window, payload| {
                eprintln!("Reloaded page with URL: {}", payload.url());
            })
            .on_window_event(|_window, event| {
                eprintln!("Window event: {:?}", event);
            });
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    Ok(())
}
