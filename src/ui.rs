use crate::controller::AppController;
use crate::preview::{load_files, update_preview};
use crate::rename::apply_changes;
use crate::state::{AppState, FileEntry};
use crate::widgets::{HighlightedLabel, ProgressBar};
use druid::widget::{Button, Checkbox, Flex, Label, List, Scroll, TextBox};
use druid::widget::CrossAxisAlignment;
use druid::widget::LineBreaking;
use druid::{Env, TextAlignment, Widget, WidgetExt};
use druid::piet::Color;
use std::path::Path;

pub fn build_ui() -> impl Widget<AppState> {
    const LABEL_WIDTH: f64 = 120.0;

    // 左上側: ディレクトリ／検索／置換／除外設定
    let directory_row = Flex::row()
        .with_child(Label::new("フォルダ:").fix_width(LABEL_WIDTH))
        .with_spacer(5.0)
        .with_flex_child(TextBox::new().lens(AppState::selected_dir).fix_height(30.0), 1.0)
        .with_spacer(5.0)
        .with_child(Button::new("参照").on_click(|_ctx, data: &mut AppState, _env| {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                data.selected_dir = path.to_string_lossy().to_string();
                load_files(data);
            }
        }));

    let search_row = Flex::row()
        .with_child(Label::new("検索:").fix_width(LABEL_WIDTH))
        .with_spacer(5.0)
        .with_flex_child(TextBox::new().lens(AppState::search_pattern).fix_height(30.0), 1.0);

    let replace_row = Flex::row()
        .with_child(Label::new("置換:").fix_width(LABEL_WIDTH))
        .with_spacer(5.0)
        .with_flex_child(TextBox::new().lens(AppState::replace_pattern).fix_height(30.0), 1.0);

    let exclude_row = Flex::row()
        .with_child(Label::new("除外:").fix_width(LABEL_WIDTH))
        .with_spacer(5.0)
        .with_flex_child(TextBox::new().lens(AppState::exclude_pattern).fix_height(30.0), 1.0);

    let left_col = Flex::column()
        .with_child(directory_row)
        .with_spacer(8.0)
        .with_child(search_row)
        .with_spacer(8.0)
        .with_child(replace_row)
        .with_spacer(8.0)
        .with_child(exclude_row);

    let checkbox_row_top = Flex::row()
        .with_child(Checkbox::new("大文字小文字を区別").lens(AppState::case_sensitive))
        .with_spacer(10.0)
        .with_child(Checkbox::new("サブフォルダを含める").lens(AppState::include_subdirectories));

    let checkbox_row_bottom = Flex::row()
        .with_child(Checkbox::new("重複時に連番を付与").lens(AppState::auto_number_on_conflict));

    let checkbox_row = Flex::column()
        .with_child(checkbox_row_top)
        .with_spacer(6.0)
        .with_child(checkbox_row_bottom)
        .cross_axis_alignment(CrossAxisAlignment::Start);

    let button_row = Flex::row()
        .with_child(
            Button::new("プレビュー")
                .on_click(|_ctx, data: &mut AppState, _env| update_preview(data))
                .fix_size(120.0, 40.0),
        )
        .with_spacer(10.0)
        .with_child(
            Button::new("変更を適用")
                .on_click(|ctx, data: &mut AppState, _env| apply_changes(ctx, data))
                .fix_size(120.0, 40.0),
        );

    let right_col = Flex::column()
        .with_child(checkbox_row)
        .with_spacer(20.0)
        .with_child(button_row)
        .with_spacer(10.0)
        .with_child(Label::new(|data: &String, _env: &Env| data.clone()).lens(AppState::status_message))
        .with_spacer(10.0)
        .with_child(ProgressBar);

    let top_panel = Flex::column()
        .with_child(Label::new("ファイル名一括変更").with_text_size(24.0))
        .with_spacer(10.0)
        .with_child(Flex::row().with_flex_child(left_col, 1.0).with_spacer(20.0).with_flex_child(right_col, 1.0));

    let original_list = List::new(|| {
        Flex::column()
            .with_child(HighlightedLabel::new(false).expand_width())
            .with_child(
                Label::new(|item: &FileEntry, _env: &Env| item.original_path.clone())
                    .with_text_color(Color::grey(0.6))
                    .with_text_size(10.0)
                    .with_line_break_mode(LineBreaking::WordWrap)
                    .expand_width(),
            )
            .cross_axis_alignment(CrossAxisAlignment::Start)
    })
    .lens(AppState::files);

    let preview_list = List::new(|| {
        Flex::column()
            .with_child(HighlightedLabel::new(true).expand_width())
            .with_child(
                Label::new(|item: &FileEntry, _env: &Env| {
                    let path = Path::new(&item.original_path);
                    let original_name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    format!("変更前: {}", original_name)
                })
                .with_text_color(Color::grey(0.6))
                .with_text_size(10.0)
                .with_line_break_mode(LineBreaking::WordWrap)
                .expand_width(),
            )
            .cross_axis_alignment(CrossAxisAlignment::Start)
    })
    .lens(AppState::preview_files);

    let original_scroll = Scroll::new(original_list).vertical();
    let preview_scroll = Scroll::new(preview_list).vertical();

    let original_panel = Flex::column()
        .with_child(Label::new(|data: &AppState, _env: &Env| format!("元のファイル ({})", data.files.len())).with_text_alignment(TextAlignment::Start))
        .with_spacer(5.0)
        .with_flex_child(original_scroll, 1.0);

    let preview_panel = Flex::column()
        .with_child(Label::new(|data: &AppState, _env: &Env| format!("プレビュー ({})", data.preview_files.len())).with_text_alignment(TextAlignment::Start))
        .with_spacer(5.0)
        .with_flex_child(preview_scroll, 1.0);

    let main_panel = Flex::row()
        .with_flex_child(original_panel, 1.0)
        .with_spacer(10.0)
        .with_flex_child(preview_panel, 1.0);

    druid::widget::Either::new(|_data: &AppState, _env| true, Flex::column()
        .with_child(top_panel)
        .with_spacer(10.0)
        .with_flex_child(main_panel, 1.0)
        .padding(10.0)
        .expand()
        .controller(AppController), Flex::column())
}


