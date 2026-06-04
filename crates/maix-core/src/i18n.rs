//! Internationalization (i18n) — locale-aware string translation.
//!
//! Supports zh-CN, en-US, ja-JP locales with embedded translation strings.

use std::collections::HashMap;
use std::sync::RwLock;

/// Supported locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Locale {
    ZhCn,
    EnUs,
    JaJp,
}

impl Locale {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::EnUs => "en-US",
            Self::JaJp => "ja-JP",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "zh-CN" | "zh" | "chinese" => Some(Self::ZhCn),
            "en-US" | "en" | "english" => Some(Self::EnUs),
            "ja-JP" | "ja" | "japanese" => Some(Self::JaJp),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::ZhCn => "简体中文",
            Self::EnUs => "English",
            Self::JaJp => "日本語",
        }
    }
}

impl Default for Locale {
    fn default() -> Self {
        // Detect from system locale, fallback to en-US
        if let Ok(lang) = std::env::var("LANG") {
            if lang.starts_with("zh") {
                return Self::ZhCn;
            }
            if lang.starts_with("ja") {
                return Self::JaJp;
            }
        }
        if let Ok(lang) = std::env::var("LC_ALL") {
            if lang.starts_with("zh") {
                return Self::ZhCn;
            }
            if lang.starts_with("ja") {
                return Self::JaJp;
            }
        }
        Self::EnUs
    }
}

/// Translation store — maps locale → key → translated string.
pub struct I18n {
    locale: Locale,
    translations: HashMap<Locale, HashMap<String, String>>,
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        let mut i18n = Self {
            locale,
            translations: HashMap::new(),
        };
        i18n.load_builtins();
        i18n
    }

    /// Get the current locale.
    pub fn locale(&self) -> Locale {
        self.locale
    }

    /// Set the current locale.
    pub fn set_locale(&mut self, locale: Locale) {
        self.locale = locale;
    }

    /// Translate a key to the current locale.
    pub fn t(&self, key: &str) -> String {
        self.translations
            .get(&self.locale)
            .and_then(|t| t.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// Translate with named argument substitution.
    pub fn t_fmt(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut text = self.t(key);
        for (k, v) in args {
            text = text.replace(&format!("{{{}}}", k), v);
        }
        text
    }

    /// Load all built-in translations.
    fn load_builtins(&mut self) {
        // English
        let en = self.translations.entry(Locale::EnUs).or_default();
        for (k, v) in en_translations() {
            en.insert(k.to_string(), v.to_string());
        }

        // Chinese
        let zh = self.translations.entry(Locale::ZhCn).or_default();
        for (k, v) in zh_translations() {
            zh.insert(k.to_string(), v.to_string());
        }

        // Japanese
        let ja = self.translations.entry(Locale::JaJp).or_default();
        for (k, v) in ja_translations() {
            ja.insert(k.to_string(), v.to_string());
        }
    }
}

/// Global i18n instance.
static I18N: RwLock<Option<I18n>> = RwLock::new(None);

/// Initialize the global i18n instance.
pub fn init_i18n(locale: Locale) {
    let mut i18n = I18N.write().unwrap_or_else(|e| e.into_inner());
    *i18n = Some(I18n::new(locale));
}

/// Get the current locale.
pub fn current_locale() -> Locale {
    let i18n = I18N.read().unwrap_or_else(|e| e.into_inner());
    i18n.as_ref().map(|i| i.locale()).unwrap_or_default()
}

/// Set the current locale.
pub fn set_locale(locale: Locale) {
    let mut i18n = I18N.write().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut i) = *i18n {
        i.set_locale(locale);
    }
}

/// Translate a key using the global i18n instance.
pub fn t(key: &str) -> String {
    let i18n = I18N.read().unwrap_or_else(|e| e.into_inner());
    i18n.as_ref().map(|i| i.t(key)).unwrap_or_else(|| key.to_string())
}

/// Translate with named arguments using the global i18n instance.
pub fn t_fmt(key: &str, args: &[(&str, &str)]) -> String {
    let i18n = I18N.read().unwrap_or_else(|e| e.into_inner());
    i18n.as_ref().map(|i| i.t_fmt(key, args)).unwrap_or_else(|| key.to_string())
}

// ---------------------------------------------------------------------------
// Built-in translations
// ---------------------------------------------------------------------------

fn en_translations() -> Vec<(&'static str, &'static str)> {
    vec![
        // Status
        ("status.idle", "Ready"),
        ("status.thinking", "Thinking"),
        ("status.executing", "Executing tool"),
        ("status.waiting", "Waiting for approval"),
        ("status.responding", "Generating response"),
        // Commands
        ("commands.help", "Command list"),
        ("commands.quit", "Goodbye"),
        ("commands.clear", "Conversation cleared"),
        ("commands.mode_switch", "Switched to {mode}"),
        ("commands.lang_switch", "Language switched to {lang}"),
        // Tools
        ("tools.calling", "Calling tool: {name}"),
        ("tools.completed", "Tool completed"),
        ("tools.failed", "Tool failed: {error}"),
        ("tools.approve", "Approve this action?"),
        ("tools.denied", "Action denied"),
        // Errors
        ("errors.connection", "Failed to connect to server"),
        ("errors.timeout", "Request timed out"),
        ("errors.rate_limit", "Rate limited, retrying in {seconds}s"),
        ("errors.context_overflow", "Context too long, compacting..."),
        ("errors.unknown", "An error occurred: {error}"),
        // Session
        ("session.new", "New session started"),
        ("session.loaded", "Session loaded: {id}"),
        ("session.saved", "Session saved"),
        ("session.exported", "Session exported to {path}"),
    ]
}

fn zh_translations() -> Vec<(&'static str, &'static str)> {
    vec![
        // Status
        ("status.idle", "就绪"),
        ("status.thinking", "思考中"),
        ("status.executing", "执行工具中"),
        ("status.waiting", "等待审批"),
        ("status.responding", "生成回复中"),
        // Commands
        ("commands.help", "命令列表"),
        ("commands.quit", "再见"),
        ("commands.clear", "已清空对话"),
        ("commands.mode_switch", "已切换到{mode}"),
        ("commands.lang_switch", "语言已切换为{lang}"),
        // Tools
        ("tools.calling", "调用工具: {name}"),
        ("tools.completed", "工具执行完成"),
        ("tools.failed", "工具执行失败: {error}"),
        ("tools.approve", "是否批准此操作?"),
        ("tools.denied", "操作已拒绝"),
        // Errors
        ("errors.connection", "连接服务器失败"),
        ("errors.timeout", "请求超时"),
        ("errors.rate_limit", "请求过于频繁，{seconds}秒后重试"),
        ("errors.context_overflow", "上下文过长，正在压缩..."),
        ("errors.unknown", "发生错误: {error}"),
        // Session
        ("session.new", "新会话已开始"),
        ("session.loaded", "会话已加载: {id}"),
        ("session.saved", "会话已保存"),
        ("session.exported", "会话已导出到 {path}"),
    ]
}

fn ja_translations() -> Vec<(&'static str, &'static str)> {
    vec![
        // Status
        ("status.idle", "準備完了"),
        ("status.thinking", "思考中"),
        ("status.executing", "ツール実行中"),
        ("status.waiting", "承認待ち"),
        ("status.responding", "応答生成中"),
        // Commands
        ("commands.help", "コマンド一覧"),
        ("commands.quit", "さようなら"),
        ("commands.clear", "会話をクリアしました"),
        ("commands.mode_switch", "{mode}に切り替えました"),
        ("commands.lang_switch", "言語を{lang}に切り替えました"),
        // Tools
        ("tools.calling", "ツール呼び出し: {name}"),
        ("tools.completed", "ツール実行完了"),
        ("tools.failed", "ツール実行失敗: {error}"),
        ("tools.approve", "この操作を承認しますか?"),
        ("tools.denied", "操作が拒否されました"),
        // Errors
        ("errors.connection", "サーバーへの接続に失敗しました"),
        ("errors.timeout", "リクエストがタイムアウトしました"),
        ("errors.rate_limit", "レート制限中、{seconds}秒後に再試行"),
        ("errors.context_overflow", "コンテキストが長すぎます、圧縮中..."),
        ("errors.unknown", "エラーが発生しました: {error}"),
        // Session
        ("session.new", "新しいセッションを開始しました"),
        ("session.loaded", "セッションを読み込みました: {id}"),
        ("session.saved", "セッションを保存しました"),
        ("session.exported", "セッションを{path}にエクスポートしました"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_english_translations() {
        let i18n = I18n::new(Locale::EnUs);
        assert_eq!(i18n.t("status.idle"), "Ready");
        assert_eq!(i18n.t("commands.clear"), "Conversation cleared");
    }

    #[test]
    fn test_chinese_translations() {
        let i18n = I18n::new(Locale::ZhCn);
        assert_eq!(i18n.t("status.idle"), "就绪");
        assert_eq!(i18n.t("commands.clear"), "已清空对话");
    }

    #[test]
    fn test_japanese_translations() {
        let i18n = I18n::new(Locale::JaJp);
        assert_eq!(i18n.t("status.idle"), "準備完了");
        assert_eq!(i18n.t("commands.clear"), "会話をクリアしました");
    }

    #[test]
    fn test_fmt_with_args() {
        let i18n = I18n::new(Locale::EnUs);
        assert_eq!(i18n.t_fmt("commands.mode_switch", &[("mode", "fast")]), "Switched to fast");
        assert_eq!(i18n.t_fmt("tools.calling", &[("name", "grep")]), "Calling tool: grep");
    }

    #[test]
    fn test_missing_key_returns_key() {
        let i18n = I18n::new(Locale::EnUs);
        assert_eq!(i18n.t("nonexistent.key"), "nonexistent.key");
    }

    #[test]
    fn test_locale_from_code() {
        assert_eq!(Locale::from_code("zh-CN"), Some(Locale::ZhCn));
        assert_eq!(Locale::from_code("en"), Some(Locale::EnUs));
        assert_eq!(Locale::from_code("ja"), Some(Locale::JaJp));
        assert_eq!(Locale::from_code("fr"), None);
    }

    #[test]
    fn test_locale_code_roundtrip() {
        for locale in [Locale::ZhCn, Locale::EnUs, Locale::JaJp] {
            let code = locale.code();
            assert_eq!(Locale::from_code(code), Some(locale));
        }
    }

    #[test]
    fn test_set_locale() {
        let mut i18n = I18n::new(Locale::EnUs);
        assert_eq!(i18n.t("status.idle"), "Ready");
        i18n.set_locale(Locale::ZhCn);
        assert_eq!(i18n.t("status.idle"), "就绪");
    }
}
