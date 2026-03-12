//! Time Travel Debugging (TTD) — record, seek, and replay execution timelines.

use crate::dto::*;
use crate::error::CmdResult;
use crate::state::AppState;
use tauri::State;

// ============================================================================
// Private helpers
// ============================================================================

fn snapshot_to_dto(s: &fission_dynamic::debug::ttd::ExecutionSnapshot) -> TtdSnapshotDto {
    TtdSnapshotDto {
        step: s.step_index,
        thread_id: s.thread_id,
        rip: format!("0x{:x}", s.registers.rip),
        rax: s.registers.rax,
        rbx: s.registers.rbx,
        rcx: s.registers.rcx,
        rdx: s.registers.rdx,
        rsp: s.registers.rsp,
        rbp: s.registers.rbp,
        rsi: s.registers.rsi,
        rdi: s.registers.rdi,
        rflags: s.registers.rflags,
    }
}

fn timeline_to_state_dto(tl: &fission_dynamic::debug::ttd::Timeline) -> TtdStateDto {
    let stats = tl.stats();
    let step_range = tl.step_range().map(|(a, b)| [a, b]);
    let current_step = tl.current_position();
    let current_snapshot = tl.current_snapshot().map(snapshot_to_dto);
    TtdStateDto {
        is_recording: tl.is_recording(),
        snapshot_count: stats.count as usize,
        step_range,
        current_step,
        current_snapshot,
    }
}

// ============================================================================
// Commands
// ============================================================================

/// Start TTD recording. While recording, every debugger `SingleStep` event is
/// captured automatically (Windows only; on other platforms the timeline simply
/// accumulates no snapshots).
#[tauri::command]
pub async fn ttd_start(state: State<'_, AppState>) -> CmdResult<TtdStateDto> {
    let mut tl = state.timeline.lock().await;
    tl.start_recording();
    Ok(timeline_to_state_dto(&tl))
}

/// Stop TTD recording and enter replay mode so the timeline can be seeked.
#[tauri::command]
pub async fn ttd_stop(state: State<'_, AppState>) -> CmdResult<TtdStateDto> {
    let mut tl = state.timeline.lock().await;
    tl.stop_recording();
    tl.enter_replay_mode();
    Ok(timeline_to_state_dto(&tl))
}

/// Return the current TTD timeline state without modifying it.
#[tauri::command]
pub async fn ttd_status(state: State<'_, AppState>) -> CmdResult<TtdStateDto> {
    let tl = state.timeline.lock().await;
    Ok(timeline_to_state_dto(&tl))
}

/// Seek to a specific step index. Returns the updated timeline state including
/// the register snapshot at that step (if found).
#[tauri::command]
pub async fn ttd_seek(step: u64, state: State<'_, AppState>) -> CmdResult<TtdStateDto> {
    let mut tl = state.timeline.lock().await;
    let _ = tl.seek_to(step);
    Ok(timeline_to_state_dto(&tl))
}

/// Step one position in the given `direction` (`"forward"` or `"rewind"`).
#[tauri::command]
pub async fn ttd_step(direction: String, state: State<'_, AppState>) -> CmdResult<TtdStateDto> {
    let mut tl = state.timeline.lock().await;
    if direction == "rewind" {
        let _ = tl.rewind(1);
    } else {
        let _ = tl.forward(1);
    }
    Ok(timeline_to_state_dto(&tl))
}
