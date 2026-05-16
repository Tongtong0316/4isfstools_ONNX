use tauri::{AppHandle, Emitter};

use crate::{ACTIVE_JOB_TOKENS, CANCEL_FLAGS, SONGS};

pub(crate) fn emit_progress(
    app: &AppHandle,
    song_id: &str,
    stage: &str,
    progress: u32,
    message: &str,
    estimated_time: Option<u32>,
) {
    if stage != "cancelling" && stage != "cancelled" {
        let status = {
            let songs = SONGS.lock().unwrap();
            songs
                .as_ref()
                .and_then(|m| m.get(song_id))
                .map(|song| song.status.clone())
        };
        if check_cancel_flag(song_id)
            || status.as_deref() == Some("cancelled")
            || status.as_deref() == Some("cancelling")
        {
            return;
        }
    }
    let _ = app.emit(
        "processing-progress",
        serde_json::json!({
            "song_id": song_id,
            "stage": stage,
            "progress": progress,
            "message": message,
            "estimated_time": estimated_time
        }),
    );
}

pub(crate) fn emit_error(app: &AppHandle, song_id: &str, stage: &str, error: &str) {
    let status = {
        let songs = SONGS.lock().unwrap();
        songs
            .as_ref()
            .and_then(|m| m.get(song_id))
            .map(|song| song.status.clone())
    };
    if check_cancel_flag(song_id)
        || status.as_deref() == Some("cancelled")
        || status.as_deref() == Some("cancelling")
    {
        return;
    }
    let _ = app.emit(
        "processing-error",
        serde_json::json!({
            "song_id": song_id,
            "stage": stage,
            "error": error
        }),
    );
}

pub(crate) fn emit_progress_for_job(
    app: &AppHandle,
    song_id: &str,
    job_token: &str,
    stage: &str,
    progress: u32,
    message: &str,
    estimated_time: Option<u32>,
) {
    if is_active_job(song_id, job_token) {
        emit_progress(app, song_id, stage, progress, message, estimated_time);
    }
}

pub(crate) fn emit_error_for_job(
    app: &AppHandle,
    song_id: &str,
    job_token: &str,
    stage: &str,
    error: &str,
) {
    if is_active_job(song_id, job_token) {
        emit_error(app, song_id, stage, error);
    }
}

pub(crate) fn check_cancel_flag(song_id: &str) -> bool {
    let flags = CANCEL_FLAGS.lock().unwrap();
    flags
        .as_ref()
        .map(|f| f.get(song_id).copied().unwrap_or(false))
        .unwrap_or(false)
}

pub(crate) fn get_active_job_token(song_id: &str) -> Option<String> {
    let tokens = ACTIVE_JOB_TOKENS.lock().unwrap();
    tokens.as_ref().and_then(|m| m.get(song_id).cloned())
}

pub(crate) fn is_active_job(song_id: &str, job_token: &str) -> bool {
    get_active_job_token(song_id).as_deref() == Some(job_token)
}
