//! Tauri IPC commands for layout detection operations.

use crate::layout::{LayoutJob, LayoutQueue};
use tauri::State;

/// Submit a layout detection job for an asset.
///
/// Returns "queued" immediately — the frontend should listen to
/// `layout:complete` and `layout:error` events for the result.
#[tauri::command]
pub async fn extract_layout(
    asset_id: String,
    asset_path: String,
    layout_queue: State<'_, LayoutQueue>,
) -> Result<String, String> {
    layout_queue.submit(LayoutJob {
        asset_id,
        asset_path,
    })?;
    Ok("queued".to_string())
}