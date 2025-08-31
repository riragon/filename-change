use druid::im::Vector;
use druid::{Data, Lens};

/// 各ファイルの情報（元のパスと新ファイル名）
#[derive(Clone, Data, Lens)]
pub struct FileEntry {
    pub original_path: String,
    pub new_name: String,
    // ハイライト用の情報
    pub search_pattern: String,
    pub replace_pattern: String,
    pub case_sensitive: bool,
}

/// アプリ全体の状態
#[derive(Clone, Data, Lens)]
pub struct AppState {
    pub selected_dir: String,
    pub files: Vector<FileEntry>,
    pub preview_files: Vector<FileEntry>, // 変更前と変更後が異なるファイル
    pub search_pattern: String,
    pub replace_pattern: String,
    pub exclude_pattern: String,
    pub case_sensitive: bool,
    pub include_subdirectories: bool,
    pub auto_number_on_conflict: bool,
    pub status_message: String,
    pub conversion_in_progress: bool,
    pub conversion_total: usize,
    pub conversion_done: usize,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            selected_dir: "".to_string(),
            files: Vector::new(),
            preview_files: Vector::new(),
            search_pattern: "".to_string(),
            replace_pattern: "".to_string(),
            exclude_pattern: "".to_string(),
            case_sensitive: false,
            include_subdirectories: false,
            auto_number_on_conflict: false,
            status_message: "準備完了".to_string(),
            conversion_in_progress: false,
            conversion_total: 0,
            conversion_done: 0,
        }
    }
}


