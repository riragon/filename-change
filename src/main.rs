use druid::widget::{Button, Checkbox, Flex, Label, List, Scroll, TextBox};
use druid::{
    AppLauncher, Data, Env, Event, EventCtx, Lens, LifeCycle, LifeCycleCtx, LayoutCtx, PaintCtx,
    Selector, Target, TextAlignment, UpdateCtx, Widget, WidgetExt, WindowDesc,
};
use druid::im::Vector;
use druid::kurbo::{Point, Rect, Size};
use druid::piet::{TextLayoutBuilder, TextLayout};
use druid::piet::Text as PietText;
use druid::piet::Color;
use druid::RenderContext;
use druid::widget::CrossAxisAlignment;

use walkdir::WalkDir;
use regex::{Regex, RegexBuilder};
use rayon::prelude::*;
use rayon::iter::ParallelBridge;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use rfd;

//
// カスタムコマンド（バックグラウンド処理からの進捗更新用）
//
const RENAMING_PROGRESS: Selector<usize> = Selector::new("renaming_progress");
const RENAMING_DONE: Selector<String> = Selector::new("renaming_done");

//
// 各ファイルの情報（元のパスと新ファイル名）
//
#[derive(Clone, Data, Lens)]
struct FileEntry {
    original_path: String,
    new_name: String,
}

//
// アプリ全体の状態
//
#[derive(Clone, Data, Lens)]
struct AppState {
    selected_dir: String,
    files: Vector<FileEntry>,
    preview_files: Vector<FileEntry>, // 変更前と変更後が異なるファイル
    search_pattern: String,
    replace_pattern: String,
    exclude_pattern: String,
    use_regex: bool,
    case_sensitive: bool,
    include_subdirectories: bool,
    show_preview: bool,
    status_message: String,
    conversion_in_progress: bool,
    conversion_total: usize,
    conversion_done: usize,
}

impl AppState {
    fn new() -> Self {
        Self {
            selected_dir: "".to_string(),
            files: Vector::new(),
            preview_files: Vector::new(),
            search_pattern: "".to_string(),
            replace_pattern: "".to_string(),
            exclude_pattern: "".to_string(),
            use_regex: false,
            case_sensitive: false,
            include_subdirectories: false,
            show_preview: false,
            status_message: "Ready".to_string(),
            conversion_in_progress: false,
            conversion_total: 0,
            conversion_done: 0,
        }
    }
}

///
/// 指定ディレクトリ（およびサブディレクトリも含む場合）のファイル一覧を読み込み、
/// 除外パターンに合致するファイルを除外した上で AppState の files に反映する。
///
fn load_files(data: &mut AppState) {
    let path = Path::new(&data.selected_dir);
    let mut files = Vector::new();
    if path.exists() && path.is_dir() {
        let walker = if data.include_subdirectories {
            WalkDir::new(path)
        } else {
            WalkDir::new(path).max_depth(1)
        };
        let exclude_filters: Vec<String> = data
            .exclude_pattern
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Some(file_name) = entry.path().file_name().and_then(|s| s.to_str()) {
                    let file_name_lower = file_name.to_lowercase();
                    if exclude_filters.iter().any(|pattern| file_name_lower.ends_with(pattern)) {
                        continue;
                    }
                    let original_path = entry.path().to_string_lossy().to_string();
                    let new_name = file_name.to_string();
                    files.push_back(FileEntry { original_path, new_name });
                }
            }
        }
        data.files = files;
        data.status_message = format!("Loaded {} files", data.files.len());
    } else {
        data.status_message = "Directory not found".to_string();
        data.files = Vector::new();
    }
}

///
/// プレビュー更新処理  
/// load_files() を呼び出して最新のファイル一覧を取得し、
/// 検索文字列（正規表現 or エスケープ済み文字列）により各ファイル名を置換、
/// 変更対象のファイル一覧（preview_files）を更新する。
///
fn update_preview(data: &mut AppState) {
    load_files(data);
    let search_pattern = data.search_pattern.clone();
    let replace_pattern = data.replace_pattern.clone();
    let use_regex = data.use_regex;
    let case_sensitive = data.case_sensitive;
    let re = if use_regex {
        Regex::new(&search_pattern).ok()
    } else {
        let escaped = regex::escape(&search_pattern);
        let mut builder = RegexBuilder::new(&escaped);
        builder.case_insensitive(!case_sensitive);
        builder.build().ok()
    };
    for file in data.files.iter_mut() {
        let path = Path::new(&file.original_path);
        let original_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if let Some(ref re) = re {
            file.new_name = re.replace_all(&original_name, replace_pattern.as_str()).to_string();
        } else {
            file.new_name = original_name;
        }
    }
    let mut preview = Vector::new();
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
    data.preview_files = preview;
    data.status_message = "Preview updated".to_string();
    data.show_preview = true;
}

///
/// リネーム処理  
/// 「ファイル名が実際に変更された」ファイルのみを対象に rename を実行する。
/// 並列処理（rayon）で実行し、進捗更新は RENAMING_PROGRESS、
/// 完了時は RENAMING_DONE で UI に通知する。
///
fn apply_changes(ctx: &mut EventCtx, data: &mut AppState) {
    if data.conversion_in_progress {
        return;
    }

    // 1) 実際にファイル名が変わるものだけを抽出する
    let changed_files: Vec<FileEntry> = data
        .files
        .iter()
        .cloned()
        .filter(|f| {
            let original_name = Path::new(&f.original_path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            original_name != f.new_name
        })
        .collect();

    let total_changed = changed_files.len();

    // 変更対象がゼロの場合はメッセージを表示して処理終了
    if total_changed == 0 {
        data.status_message = "No files need to be renamed.".to_string();
        return;
    }

    data.conversion_total = total_changed;
    data.conversion_done = 0;
    data.conversion_in_progress = true;

    let event_sink = ctx.get_external_handle();
    std::thread::spawn(move || {
        let counter = AtomicUsize::new(0);
        let results: Vec<Result<(), std::io::Error>> = changed_files
            .iter()
            .par_bridge()
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
        let msg = format!("Renamed {} files, {} errors", success_count, error_count);
        let _ = event_sink.submit_command(RENAMING_DONE, msg, Target::Global);
    });
}

///
/// Controller: 外部からのコマンド（進捗更新・完了通知）を Event::Command で受け取り、
/// AppState を更新する。
///
struct AppController;

impl<W: Widget<AppState>> druid::widget::Controller<AppState, W> for AppController {
    fn event(
        &mut self,
        child: &mut W,
        ctx: &mut EventCtx,
        event: &Event,
        data: &mut AppState,
        env: &Env,
    ) {
        if let Event::Command(cmd) = event {
            if let Some(&progress) = cmd.get(RENAMING_PROGRESS) {
                data.conversion_done = progress;
                ctx.request_update();
                ctx.set_handled();
                return;
            }
            if let Some(msg) = cmd.get(RENAMING_DONE) {
                data.status_message = msg.clone();
                data.conversion_in_progress = false;
                ctx.request_update();
                ctx.set_handled();
                return;
            }
        }
        child.event(ctx, event, data, env);
    }
}

///
/// シンプルな進捗バーウィジェット  
/// conversion_in_progress が true の場合、進捗割合に応じてバーを塗りつぶす。
///
struct ProgressBar;

impl Widget<AppState> for ProgressBar {
    fn event(&mut self, _ctx: &mut EventCtx, _event: &Event, _data: &mut AppState, _env: &Env) {}
    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &AppState,
        _env: &Env,
    ) {
    }
    fn update(&mut self, ctx: &mut UpdateCtx, old_data: &AppState, data: &AppState, _env: &Env) {
        if !old_data.same(data) {
            ctx.request_paint();
        }
    }
    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx,
        bc: &druid::BoxConstraints,
        _data: &AppState,
        _env: &Env,
    ) -> Size {
        let height = 20.0;
        let width = bc.max().width;
        Size::new(width, height)
    }
    fn paint(&mut self, ctx: &mut PaintCtx, data: &AppState, env: &Env) {
        if data.conversion_in_progress && data.conversion_total > 0 {
            let progress = data.conversion_done as f64 / data.conversion_total as f64;
            let rect = ctx.size().to_rect();
            let filled_rect = Rect::new(rect.x0, rect.y0, rect.x0 + rect.width() * progress, rect.y1);
            ctx.fill(rect, &env.get(druid::theme::BACKGROUND_LIGHT));
            ctx.fill(filled_rect, &Color::rgb8(0, 128, 0));
            let text = format!(
                "{:.0}% ({}/{})",
                progress * 100.0,
                data.conversion_done,
                data.conversion_total
            );
            let text_layout = ctx
                .text()
                .new_text_layout(text)
                .text_color(Color::WHITE)
                .build()
                .unwrap();
            let text_size = text_layout.size();
            let text_pos = Point::new(
                rect.center().x - text_size.width / 2.0,
                rect.center().y - text_size.height / 2.0,
            );
            ctx.draw_text(&text_layout, text_pos);
        }
    }
}

///
/// UI ツリーの構築  
/// 上段は、左側に各種設定（Directory/Search/Replace/Exclude）を、
/// 右側にチェックボックスオプション、Preview/Apply Changes ボタン、ステータス表示、プログレスバーを配置。
/// 下段は、左右 2 カラム（Original Files / Preview）を配置する。
///
fn build_ui() -> impl Widget<AppState> {
    const LABEL_WIDTH: f64 = 120.0;

    // 左上側: ディレクトリ／検索／置換／除外設定
    let directory_row = Flex::row()
        .with_child(Label::new("Directory:").fix_width(LABEL_WIDTH))
        .with_spacer(5.0)
        .with_flex_child(
            TextBox::new()
                .lens(AppState::selected_dir)
                .fix_height(30.0),
            1.0,
        )
        .with_spacer(5.0)
        .with_child(Button::new("Browse").on_click(|_ctx, data: &mut AppState, _env| {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                data.selected_dir = path.to_string_lossy().to_string();
                load_files(data);
            }
        }));

    let search_row = Flex::row()
        .with_child(Label::new("Search:").fix_width(LABEL_WIDTH))
        .with_spacer(5.0)
        .with_flex_child(
            TextBox::new()
                .lens(AppState::search_pattern)
                .fix_height(30.0),
            1.0,
        );

    let replace_row = Flex::row()
        .with_child(Label::new("Replace:").fix_width(LABEL_WIDTH))
        .with_spacer(5.0)
        .with_flex_child(
            TextBox::new()
                .lens(AppState::replace_pattern)
                .fix_height(30.0),
            1.0,
        );

    let exclude_row = Flex::row()
        .with_child(Label::new("Exclude:").fix_width(LABEL_WIDTH))
        .with_spacer(5.0)
        .with_flex_child(
            TextBox::new()
                .lens(AppState::exclude_pattern)
                .fix_height(30.0),
            1.0,
        );

    let left_col = Flex::column()
        .with_child(directory_row)
        .with_spacer(8.0)
        .with_child(search_row)
        .with_spacer(8.0)
        .with_child(replace_row)
        .with_spacer(8.0)
        .with_child(exclude_row);

    // 右上側: チェックボックスオプション、Preview/Apply Changes ボタン、ステータス表示、プログレスバー
    let checkbox_row = Flex::row()
        .with_child(Checkbox::new("Use Regex").lens(AppState::use_regex))
        .with_spacer(10.0)
        .with_child(Checkbox::new("Case Sensitive").lens(AppState::case_sensitive))
        .with_spacer(10.0)
        .with_child(Checkbox::new("Include Subdirectories").lens(AppState::include_subdirectories))
        .cross_axis_alignment(CrossAxisAlignment::Start);

    let button_row = Flex::row()
        .with_child(
            Button::new("Preview")
                .on_click(|_ctx, data: &mut AppState, _env| {
                    update_preview(data);
                })
                .fix_size(120.0, 40.0),
        )
        .with_spacer(10.0)
        .with_child(
            Button::new("Apply Changes")
                .on_click(|ctx, data: &mut AppState, _env| {
                    apply_changes(ctx, data);
                })
                .fix_size(120.0, 40.0),
        );

    let right_col = Flex::column()
        .with_child(checkbox_row)
        .with_spacer(20.0)
        .with_child(button_row)
        .with_spacer(10.0)
        .with_child(
            Label::new(|data: &String, _env: &Env| data.clone()).lens(AppState::status_message)
        )
        .with_spacer(10.0)
        .with_child(ProgressBar);

    let top_panel = Flex::column()
        .with_child(Label::new("Filename Changer").with_text_size(24.0))
        .with_spacer(10.0)
        .with_child(
            Flex::row()
                .with_flex_child(left_col, 1.0)
                .with_spacer(20.0)
                .with_flex_child(right_col, 1.0),
        );

    // 下段: Original Files / Preview のリスト
    let original_list = List::new(|| {
        Flex::column()
            .with_child(
                Label::new(|item: &FileEntry, _env: &Env| {
                    let path = Path::new(&item.original_path);
                    path.file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default()
                })
            )
            .with_child(
                Label::new(|item: &FileEntry, _env: &Env| item.original_path.clone())
                    .with_text_color(Color::grey(0.6))
                    .with_text_size(10.0),
            )
            .cross_axis_alignment(CrossAxisAlignment::Start)
    }).lens(AppState::files);

    let original_scroll = Scroll::new(original_list).vertical();
    let original_panel = Flex::column()
        .with_child(
            Label::new(|data: &AppState, _env: &Env| {
                format!("Original Files ({})", data.files.len())
            })
            .with_text_alignment(TextAlignment::Start)
        )
        .with_spacer(5.0)
        .with_flex_child(original_scroll, 1.0);

    let preview_list = List::new(|| {
        Flex::column()
            .with_child(Label::new(|item: &FileEntry, _env: &Env| item.new_name.clone()))
            .with_child(
                Label::new(|item: &FileEntry, _env: &Env| {
                    let path = Path::new(&item.original_path);
                    let original_name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    format!("Before: {}", original_name)
                })
                .with_text_color(Color::grey(0.6))
                .with_text_size(10.0),
            )
            .cross_axis_alignment(CrossAxisAlignment::Start)
    }).lens(AppState::preview_files);

    let preview_scroll = Scroll::new(preview_list).vertical();
    let preview_panel = Flex::column()
        .with_child(
            Label::new(|data: &AppState, _env: &Env| {
                format!("Preview ({})", data.preview_files.len())
            })
            .with_text_alignment(TextAlignment::Start)
        )
        .with_spacer(5.0)
        .with_flex_child(preview_scroll, 1.0);

    let main_panel = Flex::row()
        .with_flex_child(original_panel, 1.0)
        .with_spacer(10.0)
        .with_flex_child(preview_panel, 1.0);

    Flex::column()
        .with_child(top_panel)
        .with_spacer(10.0)
        .with_flex_child(main_panel, 1.0)
        .padding(10.0)
        .expand()
        .controller(AppController)
}

pub fn main() {
    let main_window = WindowDesc::new(build_ui())
        .title("Filename Changer")
        .window_size((900.0, 600.0));
    let initial_state = AppState::new();
    AppLauncher::with_window(main_window)
        .launch(initial_state)
        .expect("Failed to launch application");
}
