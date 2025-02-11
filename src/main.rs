use eframe::egui;
use regex::{Regex, RegexBuilder};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
// 並列処理用に rayon の機能をインポート
use rayon::prelude::*;
use rayon::iter::ParallelBridge;
use rayon::ThreadPoolBuilder;

use num_cpus;

use font_kit::family_name::FamilyName;
use font_kit::properties::Properties;
use font_kit::source::SystemSource;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Filename Changer",
        options,
        Box::new(|cc| Box::new(FilenameChangerApp::new(cc))),
    )
}

#[derive(Clone)]
struct FileEntry {
    original_path: PathBuf,
    new_name: String,
}

struct FilenameChangerApp {
    selected_dir: String,
    files: Vec<FileEntry>,
    search_pattern: String,
    replace_pattern: String,
    // 除外フィルター：カンマ区切りで複数指定可能（例: ".xml, .tmp"）
    exclude_pattern: String,
    use_regex: bool,
    case_sensitive: bool,
    include_subdirectories: bool,
    show_preview: bool,
    status_message: String,
    // 進捗表示関連
    conversion_in_progress: bool,
    conversion_total: usize,
    conversion_done: Arc<AtomicUsize>,
    rename_result_receiver: Option<mpsc::Receiver<String>>,
}

impl FilenameChangerApp {
    /// CreationContext を受け取り、起動時にフォント設定などを行う
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        set_custom_fonts(&cc.egui_ctx);

        Self {
            selected_dir: String::new(),
            files: Vec::new(),
            search_pattern: String::new(),
            replace_pattern: String::new(),
            exclude_pattern: String::new(),
            use_regex: false,
            case_sensitive: false,
            include_subdirectories: false,
            show_preview: false,
            status_message: String::from("Ready"),
            conversion_in_progress: false,
            conversion_total: 0,
            conversion_done: Arc::new(AtomicUsize::new(0)),
            rename_result_receiver: None,
        }
    }
}

impl eframe::App for FilenameChangerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 変換処理の完了をチェック
        if let Some(rx) = &self.rename_result_receiver {
            if let Ok(msg) = rx.try_recv() {
                self.status_message = msg;
                self.conversion_in_progress = false;
                self.rename_result_receiver = None;
                self.load_files(); // リネーム後、最新のファイル一覧を取得
            }
        }

        // --- トップパネル: 操作メニュー ---
        egui::TopBottomPanel::top("top_menu_bar").show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.heading("Filename Changer");
                ui.add_space(10.0);

                // Directory 入力欄と Browse ボタン（横一列）
                ui.horizontal(|ui| {
                    ui.label("Directory:");
                    ui.text_edit_singleline(&mut self.selected_dir);
                    if ui.button("Browse").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.selected_dir = path.to_string_lossy().to_string();
                            self.load_files();
                        }
                    }
                });
                ui.add_space(10.0);

                // Search 入力欄（横一列）
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.search_pattern);
                });
                ui.add_space(10.0);

                // Replace 入力欄（横一列）
                ui.horizontal(|ui| {
                    ui.label("Replace:");
                    ui.text_edit_singleline(&mut self.replace_pattern);
                });
                ui.add_space(10.0);

                // Exclude 入力欄（横一列）
                ui.horizontal(|ui| {
                    ui.label("Exclude:");
                    ui.text_edit_singleline(&mut self.exclude_pattern);
                });
                ui.add_space(10.0);

                // チェックボックスオプション一覧（横一列）
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.use_regex, "Use Regex");
                    ui.checkbox(&mut self.case_sensitive, "Case Sensitive");
                    ui.checkbox(&mut self.include_subdirectories, "Include files in subdirectories");
                });
                ui.add_space(20.0);

                // ボタン群：左右入れ替えた状態（左に Preview、右に Apply Changes）
                ui.horizontal(|ui| {
                    // 余白を追加して右寄せ
                    ui.add_space(ui.available_width() - 200.0 * 2.0 - 10.0);
                    // Preview ボタン：背景灰色、文字黒
                    if ui.add_sized(
                        [200.0, 50.0],
                        egui::Button::new(egui::RichText::new("Preview").color(egui::Color32::BLACK))
                            .fill(egui::Color32::from_rgb(128, 128, 128)),
                    )
                    .clicked()
                    {
                        self.update_preview();
                    }
                    // Apply Changes ボタン：背景青色、文字白
                    if ui.add_sized(
                        [200.0, 50.0],
                        egui::Button::new(egui::RichText::new("Apply Changes").color(egui::Color32::WHITE))
                            .fill(egui::Color32::from_rgb(0, 0, 255)),
                    )
                    .clicked()
                    {
                        self.apply_changes();
                    }
                });
                ui.add_space(10.0);

                // ステータス表示
                ui.label(&self.status_message);

                // プログレスバー（リネーム処理中のみ）
                if self.conversion_in_progress {
                    let done = self.conversion_done.load(Ordering::Relaxed) as f32;
                    let total = self.conversion_total as f32;
                    let progress = if total > 0.0 { done / total } else { 0.0 };
                    ui.add(egui::ProgressBar::new(progress).text(format!(
                        "{:.0}% ({}/{})",
                        progress * 100.0,
                        done as usize,
                        self.conversion_total
                    )));
                }
            });
        });

        // --- 中央パネル: 2カラム表示 (Original Files / Preview) ---
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |cols| {
                // 左カラム: Original Files（ファイル数付き、ファイル名のみ表示、ツールチップ付き）
                cols[0].heading(format!("Original Files ({})", self.files.len()));
                egui::ScrollArea::vertical()
                    .id_source("original_files_scroll")
                    .show(&mut cols[0], |ui| {
                        for file in &self.files {
                            if let Some(file_name) = file.original_path.file_name() {
                                ui.add(egui::Label::new(file_name.to_string_lossy()))
                                    .on_hover_ui(|ui| {
                                        ui.label(file.original_path.to_string_lossy());
                                    });
                            }
                        }
                    });

                // 右カラム: Preview（対象ファイル数付き、横幅拡大）
                cols[1].set_min_width(400.0);
                let preview_count = self
                    .files
                    .iter()
                    .filter(|file| {
                        let original_name = file.original_path.file_name().unwrap().to_string_lossy();
                        original_name != file.new_name
                    })
                    .count();
                cols[1].heading(format!("Preview ({})", preview_count));
                egui::ScrollArea::vertical()
                    .id_source("preview_files_scroll")
                    .show(&mut cols[1], |ui| {
                        let mut any_changed = false;
                        for file in &self.files {
                            let original_name = file.original_path.file_name().unwrap().to_string_lossy().to_string();
                            if original_name != file.new_name {
                                any_changed = true;
                                // Preview一覧では変換後のファイル名のみ表示し、ホバー時に変換前後をツールチップで表示
                                ui.add(egui::Label::new(file.new_name.clone()))
                                    .on_hover_ui(|ui| {
                                        ui.label(format!("Before: {}\nAfter: {}", original_name, file.new_name));
                                    });
                            }
                        }
                        if !any_changed {
                            ui.label("No files will be changed.");
                        }
                    });
            });
        });

        // 強制再描画（プログレスバー等の動的更新のため）
        ctx.request_repaint();
    }
}

impl FilenameChangerApp {
    // 指定フォルダー内（およびサブディレクトリも含む場合）からファイル一覧を取得  
    // 除外フィルターに該当するファイルは除外する
    fn load_files(&mut self) {
        let walker = if self.include_subdirectories {
            WalkDir::new(&self.selected_dir)
        } else {
            WalkDir::new(&self.selected_dir).max_depth(1)
        };

        let exclude_filters: Vec<String> = self
            .exclude_pattern
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        self.files = walker
            .into_iter()
            .par_bridge() // 並列処理で高速化
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
            .filter(|entry| {
                if let Some(file_name) = entry.path().file_name().and_then(|s| s.to_str()) {
                    let file_name_lower = file_name.to_lowercase();
                    // 除外フィルターのいずれかにマッチすれば除外
                    !exclude_filters.iter().any(|pattern| file_name_lower.ends_with(pattern))
                } else {
                    true
                }
            })
            .map(|entry| FileEntry {
                original_path: entry.path().to_path_buf(),
                new_name: entry.file_name().to_string_lossy().to_string(),
            })
            .collect();

        // ファイル名の昇順（a と b の順）にソート
        self.files.sort_by(|a, b| {
            a.original_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .cmp(&b.original_path.file_name().unwrap().to_string_lossy())
        });

        self.status_message = format!("Loaded {} files", self.files.len());
    }

    // プレビュー更新時、正規表現またはエスケープ済みパターンで置換  
    // ※最新の除外設定を反映するため load_files() を呼び出す
    fn update_preview(&mut self) {
        self.load_files();
        let search_pattern = self.search_pattern.clone();
        let replace_pattern = self.replace_pattern.clone();
        let use_regex = self.use_regex;
        let case_sensitive = self.case_sensitive;

        let re = if use_regex {
            Regex::new(&search_pattern).ok()
        } else {
            let escaped = regex::escape(&search_pattern);
            let mut builder_tmp = RegexBuilder::new(&escaped);
            let builder = builder_tmp.case_insensitive(!case_sensitive);
            builder.build().ok()
        };

        if let Some(re) = re {
            self.files.par_iter_mut().for_each(|file| {
                let original_name = file.original_path.file_name().unwrap().to_string_lossy();
                file.new_name = re.replace_all(&original_name, &replace_pattern).to_string();
            });
        } else {
            self.files.par_iter_mut().for_each(|file| {
                file.new_name = file.original_path.file_name().unwrap().to_string_lossy().to_string();
            });
        }
        self.status_message = "Preview updated".to_string();
        self.show_preview = true;
    }

    // リネーム処理を並列実行（システムの最大論理CPU数を自動取得）
    fn apply_changes(&mut self) {
        if self.conversion_in_progress {
            return;
        }
        self.conversion_total = self.files.len();
        self.conversion_done.store(0, Ordering::Relaxed);
        self.conversion_in_progress = true;

        let (tx, rx) = mpsc::channel();
        self.rename_result_receiver = Some(rx);

        let files_clone = self.files.clone();
        let conversion_done_clone = self.conversion_done.clone();

        std::thread::spawn(move || {
            let num_threads = num_cpus::get();
            let pool = ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build()
                .unwrap();
            let results: Vec<Result<(), std::io::Error>> = pool.install(|| {
                files_clone
                    .par_iter()
                    .map(|file| {
                        let new_path = file.original_path.with_file_name(&file.new_name);
                        let result = std::fs::rename(&file.original_path, &new_path);
                        conversion_done_clone.fetch_add(1, Ordering::Relaxed);
                        result
                    })
                    .collect()
            });

            let success_count = results.iter().filter(|r| r.is_ok()).count();
            let error_count = results.len() - success_count;
            let msg = format!("Renamed {} files, {} errors", success_count, error_count);
            let _ = tx.send(msg);
        });
    }
}

fn set_custom_fonts(ctx: &egui::Context) {
    use eframe::egui::{FontData, FontDefinitions, FontFamily};

    let mut fonts = FontDefinitions::default();
    let source = SystemSource::new();
    let handle_result = source.select_best_match(
        &[FamilyName::Title("MS PGothic".to_owned())],
        &Properties::new(),
    );
    match handle_result {
        Ok(handle) => {
            match handle.load() {
                Ok(font) => {
                    if let Some(font_data_bytes) = font.copy_font_data() {
                        fonts.font_data.insert(
                            "my_japanese_font".to_owned(),
                            FontData::from_owned((*font_data_bytes).to_vec()),
                        );
                        fonts.families
                            .entry(FontFamily::Proportional)
                            .or_default()
                            .insert(0, "my_japanese_font".to_owned());
                        fonts.families
                            .entry(FontFamily::Monospace)
                            .or_default()
                            .insert(0, "my_japanese_font".to_owned());
                    }
                }
                Err(e) => {
                    eprintln!("Failed to load MS PGothic font: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Could not find 'MS PGothic' in system fonts: {:?}", e);
        }
    }
    ctx.set_fonts(fonts);
}
