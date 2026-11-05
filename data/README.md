# data/ — Phrapedia のレコード（正本）

このフォルダの Markdown ファイルが Phrapedia の **正本データ** です。アプリ・MCP・手編集のどれで書いても同じファイルが更新され、Git 同期ではこのフォルダだけがコミットされます。

## 配置

```
data/<kind>/<slug>.md
```

| kind | 内容 | MVP |
|---|---|---|
| `monks` | 高僧 | 対応 |
| `temples` | 寺院 | 対応 |
| `amulets` | プラクルアン（お守り） | 将来対応 |

`slug` はファイル名（拡張子なし）。英字小文字とハイフン推奨（例: `somdet-to`）。`README.md` はレコードとして扱われません。

## 1 ファイルの形

```markdown
---
slug: somdet-to            # ファイル名と一致（保存時に自動で揃えられる）
kind: monks                # フォルダ名と一致
name: ソムデット・トー       # 表示名（日本語）
name_th: สมเด็จพระพุฒาจารย์ (โต พฺรหฺมรํสี)
name_en: Somdet Phra Phutthachan (To)
aliases: [ルアンポー・トー]
temple: wat-rakhang        # 高僧のみ: 所属寺院の slug
province: バンコク
born: "1788"               # 年は文字列で（引用符推奨）
died: "1872"
founded: ...               # 寺院のみ
status: draft              # draft | reviewed | verified
sources: [notes/somdet-to.md]   # 一次資料フォルダからの相対パス、または URL
tags: [ソムデット]
created_at: ...            # 自動管理
updated_at: ...            # 自動管理
---

# 表示名
## 概要 / ## 生涯 / ## 所属寺院 / ## 代表的なプラクルアン / ## 出典   （高僧）
## 概要 / ## 歴史 / ## 関連する高僧 / ## 所在地 / ## 出典           （寺院）
```

- レコード間リンクは `[ワット・ラカン](/temples/wat-rakhang)` のように `/<kind>/<slug>` で書きます。
- 未知の frontmatter キーは保持されます。
- 不確かな事実には「要確認」、Web 由来の情報には「未確認」と明記し、`## 出典` で一次資料と Web を分けて書きます。
- 同梱のサンプル（ソムデット・トー、ルアンプー・トゥアット、ワット・ラカン、ワット・チャーンハイ）は `status: draft` の下書きです。
