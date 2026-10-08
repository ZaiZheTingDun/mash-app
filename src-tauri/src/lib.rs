mod adb;
mod automation_coordinator;
mod battle_statistics;
mod commands;
mod craft_essence_enhancement_runner;
mod enhancement_runner;
mod friend_point_summon_runner;
mod models;
mod operation_log;
mod paths;
mod platform;
mod runner;
mod screen;
mod server;
mod storage;
mod touch;

#[cfg(test)]
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::io::Write;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Manager;

use automation_coordinator::AutomationCoordinator;
use craft_essence_enhancement_runner::CraftEssenceEnhancementRunnerHandle;
use enhancement_runner::EnhancementRunnerHandle;
use friend_point_summon_runner::FriendPointSummonRunnerHandle;
use runner::RunnerHandle;

pub use models::*;
pub use server::{
    stream_meets_minimum_resolution, stream_resolution_error, Server, STREAM_BIT_RATE,
    STREAM_MAX_FPS, STREAM_MAX_SIZE,
};

pub(crate) use commands::assets::*;
pub(crate) use commands::catalog::*;
#[cfg(test)]
pub(crate) use commands::projects::*;
pub(crate) use commands::runtime::*;
pub(crate) use commands::settings::*;
pub(crate) use paths::*;

// ---------------------------------------------------------------------------
// mash-cv runtime management
// ---------------------------------------------------------------------------

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let adb_device_settings =
                load_adb_device_settings(app.handle()).map_err(std::io::Error::other)?;
            let server = load_server_setting(app.handle()).map_err(std::io::Error::other)?;
            let recognition_settings =
                load_recognition_settings(app.handle()).map_err(std::io::Error::other)?;
            let debug_settings = commands::settings::load_debug_settings(app.handle())
                .map_err(std::io::Error::other)?;
            let app_ui_settings = commands::projects::read_app_ui_settings_from_path(
                &app_ui_settings_path(app.handle()),
            )
            .map_err(std::io::Error::other)?;
            platform::configure_app_menu(app)?;
            refresh_asset_protocol_scope(app.handle())?;
            app.manage(Mutex::new(adb_device_settings));
            app.manage(Mutex::new(server));
            app.manage(Mutex::new(recognition_settings));
            app.manage(Mutex::new(debug_settings));
            app.manage(Mutex::new(app_ui_settings));
            app.manage(Mutex::new(RunnerHandle::new_idle()));
            app.manage(Mutex::new(EnhancementRunnerHandle::new_idle()));
            app.manage(Mutex::new(CraftEssenceEnhancementRunnerHandle::new_idle()));
            app.manage(Mutex::new(FriendPointSummonRunnerHandle::new_idle()));
            app.manage(AutomationCoordinator::default());
            app.manage(commands::projects::CommandEditorHistory::default());
            app.manage(Arc::new(ResourceDownloadCancelState::default()));
            app.manage(commands::debug::DebugSidecar::new());
            app.manage(commands::rank_up_quest::RankUpQuestCaptureState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::catalog::get_servants,
            commands::catalog::get_craft_essences,
            commands::catalog::get_mystic_codes,
            commands::catalog::get_servant_skill_selection,
            commands::catalog::get_servant_skill_targeting,
            commands::assets::get_self_check_status,
            commands::assets::get_asset_bundle_status,
            commands::assets::pick_asset_bundle,
            commands::assets::import_asset_bundle,
            commands::assets::download_asset_bundles,
            commands::assets::cancel_asset_operation,
            commands::assets::cancel_resource_downloads,
            commands::runtime::get_runtime_status,
            commands::runtime::pick_runtime_bundle,
            commands::runtime::import_runtime_bundle,
            commands::runtime::download_runtime_bundles,
            commands::catalog::get_servant_portrait_path,
            commands::catalog::get_servant_face_path,
            commands::catalog::get_craft_essence_card_path,
            commands::catalog::get_skill_icon_paths,
            commands::catalog::get_template_asset_path,
            commands::catalog::list_servant_portraits,
            commands::catalog::save_servant_portrait_selection,
            commands::projects::save_battle_scenes,
            commands::projects::load_battle_scenes,
            commands::projects::load_command_editor,
            commands::projects::mutate_command_editor,
            commands::projects::save_advanced_battle_scenes,
            commands::projects::load_advanced_battle_scenes,
            commands::projects::list_exportable_configs,
            commands::projects::export_configs,
            commands::projects::pick_config_import_file,
            commands::projects::preview_config_import,
            commands::projects::import_configurations,
            commands::projects::list_projects,
            commands::projects::get_project_catalog,
            commands::projects::create_project_group,
            commands::projects::rename_project_group,
            commands::projects::delete_project_group,
            commands::projects::move_project_to_group,
            commands::projects::reorder_project_groups,
            commands::projects::reorder_projects_in_group,
            commands::projects::get_grand_class_definitions,
            commands::projects::get_active_project_id,
            commands::projects::get_home_master_figure_id,
            commands::projects::set_home_master_figure_id,
            commands::projects::set_active_project_id,
            commands::projects::get_app_theme,
            commands::projects::set_app_theme,
            commands::projects::get_battle_start_panel,
            commands::projects::set_battle_start_panel,
            commands::projects::get_mystic_code_gender,
            commands::projects::set_mystic_code_gender,
            commands::projects::create_project,
            commands::projects::duplicate_project,
            commands::projects::update_project,
            commands::projects::delete_project,
            commands::projects::delete_slot_servant,
            commands::adb::check_adb,
            commands::adb::connect_adb_port,
            commands::adb::get_selected_adb_device,
            commands::adb::refresh_adb_devices_with_previews,
            commands::adb::reset_bluestacks_adb_connection,
            commands::adb::save_adb_screenshot,
            commands::adb::select_adb_device,
            commands::settings::run_startup_migration,
            commands::settings::get_use_bluestack,
            commands::settings::set_use_bluestack,
            commands::settings::get_server,
            commands::settings::set_server,
            commands::settings::get_recognition_settings,
            commands::settings::get_debug_settings,
            commands::settings::set_image_recognition_debug_mode,
            commands::settings::set_sequence_recognition_debug_mode,
            commands::settings::set_noble_phantasm_detection_mode,
            commands::settings::set_support_ce_threshold,
            commands::settings::set_support_ce_full_gate_threshold,
            commands::settings::set_support_mlb_icon_threshold,
            commands::settings::set_support_bond_icon_threshold,
            commands::settings::set_stop_on_bond_level_up,
            commands::settings::set_stop_on_bond_max_level,
            commands::settings::set_auto_capture_bond_level_up,
            commands::settings::open_bond_level_up_screenshot_folder,
            commands::settings::set_verify_skill_activation,
            commands::settings::set_enable_extra_class_filter,
            commands::settings::set_support_full_list_ocr_fallback,
            commands::settings::set_auto_friend_request,
            commands::settings::set_unknown_screen_timeout_count,
            commands::settings::set_auto_capture_battle_before_attack,
            commands::settings::set_auto_capture_battle_result_loot,
            commands::settings::set_auto_capture_unknown_screen_timeout,
            commands::settings::set_auto_capture_skill_use_probe,
            commands::settings::set_auto_capture_unrecognized_critical_chance,
            commands::settings::set_simulate_stuck_attack_selection,
            commands::settings::should_check_updates_today,
            commands::settings::mark_update_checked_today,
            commands::automation::start_automation,
            commands::rank_up_quest::capture_rank_up_quest_page,
            commands::automation::start_rank_up_quest_automation,
            commands::automation::stop_automation,
            commands::automation::stop_automation_after_current,
            commands::automation::get_automation_status,
            battle_statistics::get_battle_daily_statistics,
            commands::automation::start_enhancement_automation,
            commands::automation::stop_enhancement_automation,
            commands::automation::get_enhancement_automation_status,
            commands::automation::start_craft_essence_enhancement_automation,
            commands::automation::stop_craft_essence_enhancement_automation,
            commands::automation::get_craft_essence_enhancement_automation_status,
            commands::automation::start_friend_point_summon_automation,
            commands::automation::stop_friend_point_summon_automation,
            commands::automation::get_friend_point_summon_automation_status,
            commands::debug::debug_capture,
            commands::debug::debug_stream_connect,
            commands::debug::debug_stream_disconnect,
            commands::debug::debug_stream_frame,
            commands::debug::debug_find_element,
            commands::debug::debug_find_element_by_name,
            commands::debug::debug_read_bond_level_up,
            commands::debug::debug_list_templates,
            commands::debug::debug_get_cv_config,
            commands::debug::debug_reload_sidecar,
            commands::debug::debug_shutdown,
            commands::debug::debug_get_runner_coordinates,
            commands::debug::debug_find_command_cards,
            commands::debug::debug_read_noble_phantasm_gauges,
            commands::debug::debug_read_noble_phantasm_gauges_live,
            commands::debug::debug_find_enhancement_servant,
            commands::debug::debug_find_attack_button,
            commands::debug::debug_read_battle_scene,
            commands::debug::debug_find_supports,
            commands::debug::debug_list_servant_assets,
            commands::debug::warm_sidecar,
            commands::catalog::get_servant_metadata,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests;
