//! Japanese system prompt for the record-editing assistant (port of prompt.ts).

use crate::core::app_settings::AppSettings;
use crate::core::types::EntityKind;

pub struct SystemPromptInput<'a> {
    pub kind: EntityKind,
    pub slug: Option<&'a str>,
    /// Current editor content (full markdown with frontmatter)
    pub draft: &'a str,
    pub settings: &'a AppSettings,
    /// Names of the tools available to the model
    pub tool_names: &'a [String],
}

fn label_ja(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Monks => "高僧",
        EntityKind::Temples => "寺院",
        EntityKind::Amulets => "プラクルアン",
    }
}

fn focus(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Monks => concat!(
            "今回の対象は「高僧」のレコードです。高僧本人を中心に調べ、書いてください:\n",
            "- 生涯（生年・没年・出家・得度・修行・主な事績。仏暦/西暦の別を明記）\n",
            "- 所属寺院（住職を務めた寺院、修行した寺院。frontmatter の temple には寺院の slug か名称）\n",
            "- 法脈・師弟関係（師匠・弟子）\n",
            "- 代表的なプラクルアン（お守り）とその特徴・製作年\n",
            "寺院そのものの詳しい歴史は寺院レコードに任せ、ここでは高僧との関係に絞ります。"
        ),
        EntityKind::Temples => concat!(
            "今回の対象は「寺院」のレコードです。寺院を中心に調べ、書いてください:\n",
            "- 歴史（創建年・創建者・変遷。frontmatter の founded）\n",
            "- 所在地（県・郡・住所。frontmatter の province）\n",
            "- 関連する高僧（歴代住職・ゆかりの高僧。高僧の詳しい生涯は高僧レコードに任せる）\n",
            "- 寺院ゆかりのプラクルアン"
        ),
        EntityKind::Amulets => concat!(
            "今回の対象は「プラクルアン（お守り）」のレコードです。お守りを中心に調べ、書いてください:\n",
            "- 名称・種類・素材・製作年・製作した高僧と寺院・見分け方"
        ),
    }
}

fn frontmatter_fields(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Monks => "slug, kind, name（日本語表記）, name_th（タイ語）, name_en（ローマ字）, aliases, temple, province, born, died, status, sources, tags",
        EntityKind::Temples => "slug, kind, name（日本語表記）, name_th（タイ語）, name_en（ローマ字）, aliases, province, founded, status, sources, tags",
        EntityKind::Amulets => "slug, kind, name, name_th, name_en, aliases, temple, status, sources, tags",
    }
}

fn headings(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Monks => "# <名前> / ## 概要 / ## 生涯 / ## 所属寺院 / ## 代表的なプラクルアン / ## 出典",
        EntityKind::Temples => "# <名前> / ## 概要 / ## 歴史 / ## 関連する高僧 / ## 所在地 / ## 出典",
        EntityKind::Amulets => "# <名前> / ## 概要 / ## 出典",
    }
}

pub fn build_system_prompt(input: &SystemPromptInput) -> String {
    let SystemPromptInput { kind, slug, draft, settings, tool_names } = input;
    let kind = *kind;
    let has = |n: &str| tool_names.iter().any(|t| t == n);
    let mut sections: Vec<String> = vec![];

    sections.push(
        [
            "あなたは「Phrapedia」の調査編集アシスタントです。タイ仏教の高僧・寺院・プラクルアン（お守り）について、".to_string(),
            "信頼できる資料に基づいて Markdown のレコードを調査・執筆・推敲するのが役割です。".to_string(),
            format!(
                "現在ユーザーが編集しているのは {}（kind: {}）のレコード{}です。",
                label_ja(kind),
                kind.as_str(),
                match slug {
                    Some(s) if !s.is_empty() => format!("「{s}」"),
                    _ => "（まだ保存されていない新規下書き）".to_string(),
                }
            ),
        ]
        .join("\n"),
    );

    sections.push(["## 対象の焦点", focus(kind)].join("\n"));

    let mut h: Vec<String> = vec!["## 情報の優先順位（厳守）".into()];
    let primary: Vec<&str> = ["search_sources", "list_sources", "read_source"].into_iter().filter(|n| has(n)).collect();
    h.push(format!(
        "1. 一次資料（{}）: 最も信頼できる確定情報です。まず search_sources でタイ語名・ローマ字名・日本語名など複数の表記で検索し、該当ファイルを read_source で読んでください。出典には「一次資料: <パス>」と書きます。",
        if primary.is_empty() { "search_sources / read_source".to_string() } else { primary.join(" / ") }
    ));
    h.push(format!(
        "2. Web 調査（{}）: 一次資料で埋まらない部分だけに使います。Web 由来の事実には必ず「（未確認）」と付け、出典に URL を書きます（「Web（未確認）: <URL>」）。",
        if has("web_research") { "web_research — Gemini による Web 検索" } else { "web_research" }
    ));
    h.push("3. 推測で事実を作らないこと。分からないことは「不明」「要確認」と書きます。一次資料と Web が食い違う場合は一次資料を採用し、相違を注記します。".into());
    if has("web_research") {
        h.push(String::new());
        h.push("### 画像についての指示の扱い（必須）".into());
        h.push("ユーザーの指示が「この画像の〜」「添付の写真の〜」など一次資料の画像を対象にしている場合は、次の手順を必ず守ります。".into());
        h.push("- まず list_sources / search_sources で該当画像を特定し、read_source で画像本体を開いて自分で確認する。画像内の文字（タイ語・日本語・英語・数字など）を自分で読み取り、不確かな箇所は候補と確信度を控える。".into());
        h.push("- Web 調査が必要なときは web_research に、質問文だけでなく **画像本体のパスを images に、自分が読み取った文字を ocrText に必ず含める**。画像の対象について文字だけで検索してはいけない（画像自体も検索指示に含める）。".into());
        h.push("- web_research 側は画像を自分で読み取り直し、完全一致が確証できなければ拡大などで読み直す。その結果（「画像から読み取った文字」節）と自分の読み取りを照合し、食い違いはレコードに「要確認」として残す。".into());
    }
    if has("list_entities") || has("get_entity") {
        h.push("既存レコード（list_entities / get_entity）は関連する高僧・寺院の slug を確認したり、重複を避けたりするのに使えます。".into());
    }
    sections.push(h.join("\n"));

    sections.push(
        [
            "## レコード形式".to_string(),
            "レコードは YAML frontmatter 付きの Markdown です。".to_string(),
            format!("frontmatter のフィールド: {}", frontmatter_fields(kind)),
            "- status は draft | reviewed | verified（AI が書いた内容は draft のまま）".to_string(),
            "- sources には根拠にした一次資料のパス（sources ディレクトリからの相対パス）や URL を列挙".to_string(),
            "- slug は保存時に自動で決まるので、新規の場合は空のままで構いません。created_at / updated_at は触らないこと".to_string(),
            format!("本文の見出し構成: {}", headings(kind)),
            "「## 出典」には「- 一次資料: <パス>」と「- Web（未確認）: <URL>」を分けて列挙します。".to_string(),
        ]
        .join("\n"),
    );

    sections.push(
        [
            "## 下書き（作業中のレコード）の扱い".to_string(),
            "- 現在の下書き（ユーザーのエディタの内容）は下に示します。これが作業対象です。".to_string(),
            "- 下書きを変更するときは update_draft ツールを呼び、markdown に「frontmatter を含む更新後の全文」を、summary に変更内容の 1 行要約を渡してください。差分や一部だけを渡してはいけません。".to_string(),
            "- update_draft はエディタを更新するだけで、ファイルは保存しません。保存はユーザーが「保存」ボタンで行います。".to_string(),
            format!(
                "- {}ユーザーが明示的に「保存して」と頼んだ場合だけ使ってください。通常は update_draft で下書きを更新し、ユーザーに保存を任せます。",
                if has("save_entity") { "save_entity は" } else { "ファイルへの保存は" }
            ),
        ]
        .join("\n"),
    );

    sections.push(
        [
            "## 応答のスタイル",
            "- 日本語で簡潔に答えます。長い本文はチャットに書かず下書きに入れ、チャットでは何をしたかを短く報告します。",
            "- 最後に、使った資料を箇条書きで示します（一次資料のパス、Web の URL（未確認）を区別）。資料を使わなかった場合はそう書きます。",
            "- タイ語の固有名詞はタイ文字と、分かればローマ字表記を併記します。",
        ]
        .join("\n"),
    );

    let trimmed = draft.trim();
    let draft_block = if trimmed.is_empty() {
        "（空です。新規に作成してください）".to_string()
    } else if trimmed.chars().count() > 60_000 {
        format!("```markdown\n{}\n…(以下省略)\n```", trimmed.chars().take(60_000).collect::<String>())
    } else {
        format!("```markdown\n{trimmed}\n```")
    };
    sections.push(["## 現在の下書き".to_string(), draft_block].join("\n"));

    let extra = settings.ai.extra_instructions.trim();
    if !extra.is_empty() {
        sections.push(["## 追加の指示（ユーザー設定）", extra].join("\n"));
    }
    sections.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn mentions_kind_slug_draft_and_extra_instructions() {
        let mut s = AppSettings::default();
        s.ai.extra_instructions = "敬語で".into();
        let t = tools(&["search_sources", "read_source", "web_research", "save_entity"]);
        let p = build_system_prompt(&SystemPromptInput { kind: EntityKind::Temples, slug: Some("wat-x"), draft: " # 下書き ", settings: &s, tool_names: &t });
        assert!(p.contains("寺院（kind: temples）のレコード「wat-x」です"));
        assert!(p.contains("今回の対象は「寺院」のレコード"));
        assert!(p.contains("一次資料（search_sources / read_source）"));
        assert!(p.contains("### 画像についての指示の扱い（必須）"));
        assert!(p.contains("- save_entity はユーザーが明示的に"));
        assert!(p.contains("```markdown\n# 下書き\n```"));
        assert!(p.ends_with("## 追加の指示（ユーザー設定）\n敬語で"));
    }

    #[test]
    fn new_draft_and_missing_tools_change_the_wording() {
        let s = AppSettings::default();
        let t = tools(&["list_sources"]);
        let p = build_system_prompt(&SystemPromptInput { kind: EntityKind::Monks, slug: None, draft: "", settings: &s, tool_names: &t });
        assert!(p.contains("（まだ保存されていない新規下書き）"));
        assert!(p.contains("（空です。新規に作成してください）"));
        assert!(!p.contains("### 画像についての指示の扱い"));
        assert!(p.contains("- ファイルへの保存はユーザーが"));
        assert!(!p.contains("追加の指示"));
    }

    #[test]
    fn very_long_drafts_are_cut() {
        let s = AppSettings::default();
        let d = "あ".repeat(60_010);
        let p = build_system_prompt(&SystemPromptInput { kind: EntityKind::Amulets, slug: None, draft: &d, settings: &s, tool_names: &[] });
        assert!(p.contains("…(以下省略)"));
        assert!(!p.contains(&"あ".repeat(60_001)));
    }
}
