use crate::events::{PREVIEW_REQUEST, RENAMING_DONE, RENAMING_PROGRESS};
use rfd::{MessageButtons, MessageDialog, MessageLevel};
use crate::preview::update_preview;
use crate::state::AppState;
use druid::{Env, Event, EventCtx, UpdateCtx, Widget};

pub struct AppController;

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
            if cmd.is(PREVIEW_REQUEST) {
                update_preview(data);
                ctx.set_handled();
                return;
            }
            if let Some(&progress) = cmd.get(RENAMING_PROGRESS) {
                data.conversion_done = progress;
                ctx.request_update();
                ctx.set_handled();
                return;
            }
            if let Some(msg) = cmd.get(RENAMING_DONE) {
                data.status_message = msg.clone();
                data.conversion_in_progress = false;
                // リネーム適用後にファイル一覧/プレビューを最新化
                ctx.submit_command(PREVIEW_REQUEST.with(()));
                // 完了ダイアログを表示
                let message = msg.clone();
                std::thread::spawn(move || {
                    MessageDialog::new()
                        .set_title("変更の適用が完了しました")
                        .set_description(&message)
                        .set_buttons(MessageButtons::Ok)
                        .set_level(MessageLevel::Info)
                        .show();
                });
                ctx.request_update();
                ctx.set_handled();
                return;
            }
        }
        child.event(ctx, event, data, env);
    }

    fn update(
        &mut self,
        child: &mut W,
        ctx: &mut UpdateCtx,
        old_data: &AppState,
        data: &AppState,
        env: &Env,
    ) {
        let checkbox_changed =
            old_data.case_sensitive != data.case_sensitive ||
            old_data.include_subdirectories != data.include_subdirectories;
        if checkbox_changed {
            ctx.submit_command(PREVIEW_REQUEST.with(()));
        }
        child.update(ctx, old_data, data, env);
    }
}


