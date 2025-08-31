use crate::state::FileEntry;
use druid::kurbo::{Point, Rect, Size};
use druid::piet::{TextLayoutBuilder, TextLayout};
use druid::piet::Text as PietText;
use druid::piet::Color;
use druid::{Env, Event, EventCtx, LifeCycle, LifeCycleCtx, LayoutCtx, PaintCtx, UpdateCtx, Widget};
use druid::RenderContext;
use druid::Data;
use regex::RegexBuilder;
use std::path::Path;

/// ハイライト表示対応のカスタムラベルウィジェット
pub struct HighlightedLabel {
    pub is_replacement: bool, // true: 置換後のテキスト, false: 元のテキスト
}

impl HighlightedLabel {
    pub fn new(is_replacement: bool) -> Self {
        Self { is_replacement }
    }
}

impl Widget<FileEntry> for HighlightedLabel {
    fn event(&mut self, _ctx: &mut EventCtx, _event: &Event, _data: &mut FileEntry, _env: &Env) {}

    fn lifecycle(&mut self, _ctx: &mut LifeCycleCtx, _event: &LifeCycle, _data: &FileEntry, _env: &Env) {}

    fn update(&mut self, ctx: &mut UpdateCtx, old_data: &FileEntry, data: &FileEntry, _env: &Env) {
        if !old_data.same(data) {
            ctx.request_paint();
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &druid::BoxConstraints, data: &FileEntry, env: &Env) -> Size {
        let current_text = if self.is_replacement {
            data.new_name.clone()
        } else {
            let path = Path::new(&data.original_path);
            path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        };

        let text_layout = ctx
            .text()
            .new_text_layout(current_text)
            .font(druid::piet::FontFamily::SYSTEM_UI, env.get(druid::theme::TEXT_SIZE_NORMAL))
            .build()
            .unwrap();

        let size = text_layout.size();
        bc.constrain(size)
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &FileEntry, env: &Env) {
        let current_text = if self.is_replacement {
            data.new_name.clone()
        } else {
            let path = Path::new(&data.original_path);
            path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        };

        if data.search_pattern.is_empty() {
            let text_layout = ctx
                .text()
                .new_text_layout(current_text)
                .font(druid::piet::FontFamily::SYSTEM_UI, env.get(druid::theme::TEXT_SIZE_NORMAL))
                .text_color(env.get(druid::theme::TEXT_COLOR))
                .build()
                .unwrap();

            ctx.draw_text(&text_layout, Point::ORIGIN);
            return;
        }

        let (highlight_text, is_highlight) = if self.is_replacement {
            (data.replace_pattern.clone(), !data.replace_pattern.is_empty())
        } else {
            (data.search_pattern.clone(), true)
        };

        if !is_highlight || highlight_text.is_empty() {
            let text_layout = ctx
                .text()
                .new_text_layout(current_text)
                .font(druid::piet::FontFamily::SYSTEM_UI, env.get(druid::theme::TEXT_SIZE_NORMAL))
                .text_color(env.get(druid::theme::TEXT_COLOR))
                .build()
                .unwrap();

            ctx.draw_text(&text_layout, Point::ORIGIN);
            return;
        }

        let escaped = regex::escape(&highlight_text);
        let mut rb = RegexBuilder::new(&escaped);
        rb.case_insensitive(!data.case_sensitive);
        let re = match rb.build() {
            Ok(r) => r,
            Err(_) => {
                let text_layout = ctx
                    .text()
                    .new_text_layout(current_text)
                    .font(druid::piet::FontFamily::SYSTEM_UI, env.get(druid::theme::TEXT_SIZE_NORMAL))
                    .text_color(env.get(druid::theme::TEXT_COLOR))
                    .build()
                    .unwrap();
                ctx.draw_text(&text_layout, Point::ORIGIN);
                return;
            }
        };

        let mut current_x = 0.0;
        let mut last = 0usize;
        for m in re.find_iter(&current_text) {
            let start = m.start();
            let end = m.end();

            if start > last {
                let normal = &current_text[last..start];
                let normal_layout = ctx
                    .text()
                    .new_text_layout(normal.to_string())
                    .font(druid::piet::FontFamily::SYSTEM_UI, env.get(druid::theme::TEXT_SIZE_NORMAL))
                    .text_color(env.get(druid::theme::TEXT_COLOR))
                    .build()
                    .unwrap();
                ctx.draw_text(&normal_layout, Point::new(current_x, 0.0));
                current_x += normal_layout.size().width;
            }

            let seg = &current_text[start..end];
            let hl_layout = ctx
                .text()
                .new_text_layout(seg.to_string())
                .font(druid::piet::FontFamily::SYSTEM_UI, env.get(druid::theme::TEXT_SIZE_NORMAL))
                .text_color(Color::rgb8(0, 0, 0))
                .build()
                .unwrap();
            let hl_size = hl_layout.size();
            let rect = Rect::new(current_x, 0.0, current_x + hl_size.width, hl_size.height);
            ctx.fill(rect, &Color::rgb8(255, 255, 0));
            ctx.draw_text(&hl_layout, Point::new(current_x, 0.0));
            current_x += hl_size.width;

            last = end;
        }

        if last < current_text.len() {
            let tail = &current_text[last..];
            let tail_layout = ctx
                .text()
                .new_text_layout(tail.to_string())
                .font(druid::piet::FontFamily::SYSTEM_UI, env.get(druid::theme::TEXT_SIZE_NORMAL))
                .text_color(env.get(druid::theme::TEXT_COLOR))
                .build()
                .unwrap();
            ctx.draw_text(&tail_layout, Point::new(current_x, 0.0));
        }
    }
}

/// シンプルな進捗バーウィジェット
pub struct ProgressBar;

impl Widget<crate::state::AppState> for ProgressBar {
    fn event(&mut self, _ctx: &mut EventCtx, _event: &Event, _data: &mut crate::state::AppState, _env: &Env) {}
    fn lifecycle(&mut self, _ctx: &mut LifeCycleCtx, _event: &LifeCycle, _data: &crate::state::AppState, _env: &Env) {}
    fn update(&mut self, ctx: &mut UpdateCtx, old_data: &crate::state::AppState, data: &crate::state::AppState, _env: &Env) {
        if !old_data.same(data) {
            ctx.request_paint();
        }
    }
    fn layout(&mut self, _ctx: &mut LayoutCtx, bc: &druid::BoxConstraints, _data: &crate::state::AppState, _env: &Env) -> druid::kurbo::Size {
        let height = 20.0;
        let width = bc.max().width;
        druid::kurbo::Size::new(width, height)
    }
    fn paint(&mut self, ctx: &mut PaintCtx, data: &crate::state::AppState, env: &Env) {
        if data.conversion_in_progress && data.conversion_total > 0 {
            let progress = data.conversion_done as f64 / data.conversion_total as f64;
            let rect = ctx.size().to_rect();
            let filled_rect = Rect::new(rect.x0, rect.y0, rect.x0 + rect.width() * progress, rect.y1);
            ctx.fill(rect, &env.get(druid::theme::BACKGROUND_LIGHT));
            ctx.fill(filled_rect, &Color::rgb8(0, 128, 0));
            let text = format!("{:.0}% ({}/{})", progress * 100.0, data.conversion_done, data.conversion_total);
            let text_layout = ctx
                .text()
                .new_text_layout(text)
                .text_color(Color::WHITE)
                .build()
                .unwrap();
            let text_size = text_layout.size();
            let text_pos = Point::new(rect.center().x - text_size.width / 2.0, rect.center().y - text_size.height / 2.0);
            ctx.draw_text(&text_layout, text_pos);
        }
    }
}


