use crate::state::{AppState, FileEntry};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::{Regex, RegexBuilder, NoExpand};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use walkdir::WalkDir;
use druid::im::Vector;
use tracing::debug;

/// 指定ディレクトリ（およびサブディレクトリも含む場合）のファイル一覧を読み込み、
/// 除外パターンに合致するファイルを除外した上で AppState の files に反映する。
pub fn load_files(data: &mut AppState) {
    let path = Path::new(&data.selected_dir);
    let mut files = Vector::new();
    if path.exists() && path.is_dir() {
        let walker = if data.include_subdirectories {
            WalkDir::new(path)
        } else {
            WalkDir::new(path).max_depth(1)
        };

        // Exclude: 3系統サポート（ケース非依存）
        let mut glob_builder = GlobSetBuilder::new();
        let mut regex_excludes: Vec<Regex> = Vec::new();
        let mut filename_substrings: Vec<String> = Vec::new();
        let mut path_substrings: Vec<String> = Vec::new();
        for raw in data
            .exclude_pattern
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let is_regex = raw.to_ascii_lowercase().starts_with("re:");
            if is_regex {
                let pat = &raw[3..];
                let mut rb = RegexBuilder::new(pat);
                rb.case_insensitive(true);
                match rb.build() {
                    Ok(re) => regex_excludes.push(re),
                    Err(_) => {
                        data.status_message = format!("Exclude regex error: {}", pat);
                        debug!(target: "exclude", err = %pat, "exclude_regex_error");
                    }
                }
                continue;
            }
            let has_glob_meta = raw.contains('*') || raw.contains('?') || raw.contains('[') || raw.contains('{');
            let has_sep = raw.contains('/') || raw.contains('\\');
            if has_glob_meta {
                match GlobBuilder::new(raw).case_insensitive(true).build() {
                    Ok(g) => {
                        glob_builder.add(g);
                    }
                    Err(_) => {
                        data.status_message = format!("Exclude glob error: {}", raw);
                        debug!(target: "exclude", err = %raw, "exclude_glob_error");
                    }
                }
            } else if has_sep {
                path_substrings.push(raw.to_ascii_lowercase());
            } else {
                filename_substrings.push(raw.to_ascii_lowercase());
            }
        }
        let glob_set: Option<GlobSet> = glob_builder.build().ok();
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let full_path = entry.path();
                if glob_set
                    .as_ref()
                    .map(|gs| gs.is_match(full_path))
                    .unwrap_or(false)
                {
                    debug!(target: "exclude", path = %full_path.display(), "excluded by glob");
                    continue;
                }
                let full_path_str = full_path.to_string_lossy();
                if regex_excludes.iter().any(|re| re.is_match(&full_path_str)) {
                    debug!(target: "exclude", path = %full_path.display(), "excluded by regex");
                    continue;
                }
                let path_lower = full_path_str.to_ascii_lowercase();
                let file_name_lower = full_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_ascii_lowercase())
                    .unwrap_or_default();
                if !filename_substrings.is_empty() {
                    if filename_substrings.iter().any(|tok| file_name_lower.contains(tok)) {
                        debug!(target: "exclude", path = %full_path.display(), reason = "filename_substring");
                        continue;
                    }
                }
                if !path_substrings.is_empty() {
                    if path_substrings.iter().any(|sub| path_lower.contains(sub)) {
                        debug!(target: "exclude", path = %full_path.display(), reason = "substring");
                        continue;
                    }
                }
                if let Some(file_name) = full_path.file_name().and_then(|s| s.to_str()) {
                    let original_path = full_path.to_string_lossy().to_string();
                    let new_name = file_name.to_string();
                    files.push_back(FileEntry {
                        original_path,
                        new_name,
                        search_pattern: data.search_pattern.clone(),
                        replace_pattern: data.replace_pattern.clone(),
                        case_sensitive: data.case_sensitive,
                    });
                }
            }
        }
        data.files = files;
        data.status_message = format!("ファイル {} 件を読み込み", data.files.len());
        debug!("loaded_files: {}", data.files.len());
    } else {
        data.status_message = "ディレクトリが見つかりません".to_string();
        data.files = Vector::new();
    }
}

/// プレビュー更新処理
pub fn update_preview(data: &mut AppState) {
    load_files(data);
    let search_pattern = data.search_pattern.clone();
    let replace_pattern = data.replace_pattern.clone();
    let case_sensitive = data.case_sensitive;
    let re = if search_pattern.is_empty() {
        None
    } else {
        let escaped = regex::escape(&search_pattern);
        let mut builder = RegexBuilder::new(&escaped);
        builder.case_insensitive(!case_sensitive);
        Some(builder.build().unwrap())
    };
    for file in data.files.iter_mut() {
        let path = Path::new(&file.original_path);
        let original_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if let Some(ref re) = re {
            let replaced = re
                .replace_all(&original_name, NoExpand(replace_pattern.as_str()))
                .to_string();
            debug!(orig = %original_name, new = %replaced, "preview_rename");
            file.new_name = replaced;
        } else {
            file.new_name = original_name;
        }
        file.search_pattern = search_pattern.clone();
        file.replace_pattern = replace_pattern.clone();
        file.case_sensitive = case_sensitive;
    }
    let mut preview = druid::im::Vector::new();
    for file in data.files.iter() {
        let path = Path::new(&file.original_path);
        let original_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if original_name != file.new_name {
            preview.push_back(file.clone());
        }
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut dup_count = 0usize;
    for f in preview.iter() {
        let parent = Path::new(&f.original_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let new_path = parent.join(&f.new_name);
        let key = new_path.to_string_lossy().to_string().to_ascii_lowercase();
        if !seen.insert(key) {
            dup_count += 1;
        }
    }

    let mut numbered_count = 0usize;
    if data.auto_number_on_conflict && !preview.is_empty() {
        let mut used_by_parent: HashMap<String, HashSet<String>> = HashMap::new();
        for f in data.files.iter() {
            let parent = Path::new(&f.original_path)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default();
            let parent_key = parent.to_string_lossy().to_string().to_ascii_lowercase();
            let orig_name_lower = Path::new(&f.original_path)
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            used_by_parent
                .entry(parent_key)
                .or_default()
                .insert(orig_name_lower);
        }

        for f in preview.iter_mut() {
            let parent = Path::new(&f.original_path)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default();
            let parent_key = parent.to_string_lossy().to_string().to_ascii_lowercase();
            let used = used_by_parent.entry(parent_key.clone()).or_default();

            let mut candidate = f.new_name.clone();
            let mut candidate_lower = candidate.to_ascii_lowercase();

            if used.contains(&candidate_lower) {
                let (base, ext) = match candidate.rsplit_once('.') {
                    Some((b, e)) => (b.to_string(), format!(".{}", e)),
                    None => (candidate.clone(), String::new()),
                };
                let mut n: usize = 2;
                loop {
                    let c = format!("{} ({}){}", base, n, ext);
                    let c_lower = c.to_ascii_lowercase();
                    if !used.contains(&c_lower) {
                        candidate = c;
                        candidate_lower = c_lower;
                        numbered_count += 1;
                        break;
                    }
                    n += 1;
                }
            }

            used.insert(candidate_lower);
            f.new_name = candidate;
        }
    }
    if !preview.is_empty() {
        let mut map_by_original: HashMap<String, String> = HashMap::new();
        for f in preview.iter() {
            map_by_original.insert(f.original_path.clone(), f.new_name.clone());
        }
        for f in data.files.iter_mut() {
            if let Some(new_name) = map_by_original.get(&f.original_path) {
                f.new_name = new_name.clone();
            }
        }
    }
    data.preview_files = preview;
    if data.auto_number_on_conflict {
        if numbered_count > 0 {
            data.status_message = format!(
                "プレビュー更新 (変更 {} 件, 連番付与 {} 件)",
                data.preview_files.len(),
                numbered_count
            );
        } else {
            data.status_message = format!("プレビュー更新 (変更 {} 件)", data.preview_files.len());
        }
    } else if dup_count > 0 {
        data.status_message = format!(
            "プレビュー更新 (変更 {} 件, 重複 {} 件)",
            data.preview_files.len(),
            dup_count
        );
    } else {
        data.status_message = format!("プレビュー更新 (変更 {} 件)", data.preview_files.len());
    }
}


