use crate::state::{AppState, FileEntry};
use crate::events::{RENAMING_DONE, RENAMING_PROGRESS};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use druid::{EventCtx, Target};
use tracing::error;

/// リネーム処理
pub fn apply_changes(ctx: &mut EventCtx, data: &mut AppState) {
    if data.conversion_in_progress {
        return;
    }

    // 実際にファイル名が変わるものだけを抽出
    let changed_files: Vec<FileEntry> = data
        .files
        .iter()
        .cloned()
        .filter(|f| {
            let original_path = Path::new(&f.original_path);
            if !original_path.exists() {
                // 一度適用済みなどで元パスがすでに存在しないエントリはスキップ
                return false;
            }
            let original_name = original_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            original_name != f.new_name
        })
        .collect();

    let total_changed = changed_files.len();
    if total_changed == 0 {
        data.status_message = "変更対象のファイルはありません。".to_string();
        return;
    }

    // 衝突検出
    let mut new_path_to_sources: HashMap<String, Vec<String>> = HashMap::new();
    let mut existing_conflicts: Vec<String> = Vec::new();
    for f in &changed_files {
        let original_path = Path::new(&f.original_path);
        let new_path_buf = original_path.with_file_name(&f.new_name);
        let new_path_norm = new_path_buf.to_string_lossy().to_string().to_ascii_lowercase();
        new_path_to_sources
            .entry(new_path_norm.clone())
            .or_default()
            .push(f.original_path.clone());
        if new_path_buf.exists() {
            let orig_norm = original_path
                .to_string_lossy()
                .to_string()
                .to_ascii_lowercase();
            if new_path_norm != orig_norm {
                existing_conflicts.push(new_path_buf.to_string_lossy().to_string());
            }
        }
    }
    let duplicates: Vec<(String, Vec<String>)> = new_path_to_sources
        .into_iter()
        .filter_map(|(k, v)| if v.len() > 1 { Some((k, v)) } else { None })
        .collect();
    if !duplicates.is_empty() || !existing_conflicts.is_empty() {
        let dup_count = duplicates.len();
        let exist_count = existing_conflicts.len();
        error!(?duplicates, ?existing_conflicts, "collision_detected");
        data.status_message = format!(
            "衝突を検出: 新名の重複 {} 件、既存ファイルとの衝突 {} 件",
            dup_count, exist_count
        );
        return;
    }

    data.conversion_total = total_changed;
    data.conversion_done = 0;
    data.conversion_in_progress = true;

    let event_sink = ctx.get_external_handle();
    std::thread::spawn(move || {
        let counter = AtomicUsize::new(0);
        let results: Vec<Result<(), std::io::Error>> = changed_files
            .par_iter()
            .map(|file| {
                let original_path = Path::new(&file.original_path);
                let new_path = original_path.with_file_name(&file.new_name);
                let result = std::fs::rename(&original_path, &new_path);
                let done_count = counter.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = event_sink.submit_command(RENAMING_PROGRESS, done_count, Target::Global);
                result
            })
            .collect();

        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let error_count = results.len() - success_count;
        let msg = format!("リネーム {} 件、エラー {} 件", success_count, error_count);
        let _ = event_sink.submit_command(RENAMING_DONE, msg, Target::Global);
    });
}


