use druid::Selector;

// カスタムコマンド（バックグラウンド処理からの進捗更新用）
pub const RENAMING_PROGRESS: Selector<usize> = Selector::new("renaming_progress");
pub const RENAMING_DONE: Selector<String> = Selector::new("renaming_done");
pub const PREVIEW_REQUEST: Selector<()> = Selector::new("preview_request");


