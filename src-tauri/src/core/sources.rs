//! Primary sources (read-only): listing, reading, SQLite FTS5 index and search (port of server/utils/sources.ts).

use super::error::{AppError, Result};
use super::paths::{safe_join, to_posix};
use super::types::{ReindexResult, SourceContent, SourceFile, SourceFileKind, SourceSearchHit};
use super::Core;
use base64::Engine as _;
use rusqlite::params;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const IGNORED_DIRS: [&str; 5] = ["node_modules", ".git", ".phrapedia", ".nuxt", ".output"];
const MAX_INDEXED_CHARS: usize = 512 * 1024;

pub fn source_kind(ext: &str) -> SourceFileKind {
    match ext.to_lowercase().as_str() {
        ".md" | ".markdown" => SourceFileKind::Markdown,
        ".txt" | ".csv" | ".json" | ".yaml" | ".yml" => SourceFileKind::Text,
        ".png" | ".jpg" | ".jpeg" | ".gif" | ".webp" => SourceFileKind::Image,
        ".pdf" => SourceFileKind::Pdf,
        _ => SourceFileKind::Other,
    }
}

pub fn image_mime(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        ".png" => Some("image/png"),
        ".jpg" | ".jpeg" => Some("image/jpeg"),
        ".gif" => Some("image/gif"),
        ".webp" => Some("image/webp"),
        _ => None,
    }
}

fn ext_of(name: &str) -> String {
    // node's extname: last dot that is not the first char
    match name.rfind('.') {
        Some(i) if i > 0 => name[i..].to_string(),
        _ => String::new(),
    }
}

fn iso(t: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(t).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn walk(base_real: &Path, dir: &Path, base: &Path, out: &mut Vec<SourceFile>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || IGNORED_DIRS.contains(&name.as_str()) {
            continue;
        }
        let full = e.path();
        // std::fs::metadata follows symlinks; dangling links and links leaving the sources dir are skipped
        let Ok(meta) = fs::metadata(&full) else { continue };
        if e.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
            match full.canonicalize() {
                Ok(real) if real.starts_with(base_real) => {}
                _ => continue,
            }
        }
        if meta.is_dir() {
            walk(base_real, &full, base, out);
        } else if meta.is_file() {
            let ext = ext_of(&name);
            out.push(SourceFile {
                path: to_posix(full.strip_prefix(base).unwrap_or(&full)),
                name,
                kind: source_kind(&ext),
                ext,
                size: meta.len(),
                mtime: meta.modified().map(iso).unwrap_or_default(),
            });
        }
    }
}

fn excerpt(text: &str, at_byte: usize, radius: usize) -> String {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let at = chars.iter().position(|(b, _)| *b >= at_byte).unwrap_or(chars.len());
    let start = at.saturating_sub(radius);
    let end = (at + radius).min(chars.len());
    let mut s: String = chars[start..end].iter().map(|(_, c)| *c).collect();
    // collapse whitespace runs
    let mut collapsed = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.drain(..) {
        if c.is_whitespace() {
            if !prev_ws {
                collapsed.push(' ');
            }
            prev_ws = true;
        } else {
            collapsed.push(c);
            prev_ws = false;
        }
    }
    format!("{}{}{}", if start > 0 { "…" } else { "" }, collapsed, if end < chars.len() { "…" } else { "" })
}

impl Core {
    /// Recursively list files under the sources dir (relative POSIX paths, sorted).
    pub fn list_sources(&self, subdir: &str) -> Result<Vec<SourceFile>> {
        let base = self.sources_dir()?;
        let start = safe_join(&base, subdir)?;
        let mut out = vec![];
        if base.is_dir() {
            let base_real = base.canonicalize().unwrap_or_else(|_| base.clone());
            walk(&base_real, &start, &base, &mut out);
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// Absolute path of a source file (validated against the sources dir).
    pub fn source_absolute_path(&self, rel: &str) -> Result<(PathBuf, SourceFileKind, Option<&'static str>)> {
        let full = safe_join(&self.sources_dir()?, rel)?;
        let ext = ext_of(&full.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        let mime = image_mime(&ext).or(if ext.to_lowercase() == ".pdf" { Some("application/pdf") } else { None });
        Ok((full, source_kind(&ext), mime))
    }

    /// Raw bytes of a source file plus its mime type (for images / PDFs shown in the WebView).
    pub fn read_source_bytes(&self, rel: &str) -> Result<(Vec<u8>, Option<&'static str>)> {
        let (full, _, mime) = self.source_absolute_path(rel)?;
        let meta = fs::metadata(&full).map_err(|_| AppError::not_found(format!("一次資料が見つかりません: {rel}")))?;
        if !meta.is_file() {
            return Err(AppError::not_found(format!("ファイルではありません: {rel}")));
        }
        Ok((fs::read(&full).map_err(|e| AppError::io("一次資料を読めません", e))?, mime))
    }

    /// Text for markdown / text, base64 for images.
    pub fn read_source(&self, rel: &str, max_chars: Option<usize>) -> Result<SourceContent> {
        let (full, kind, mime) = self.source_absolute_path(rel)?;
        let meta = fs::metadata(&full).map_err(|_| AppError::not_found(format!("一次資料が見つかりません: {rel}")))?;
        if !meta.is_file() {
            return Err(AppError::invalid(format!("ファイルではありません: {rel}")));
        }
        let size = meta.len();
        let mut c = SourceContent { path: rel.to_string(), kind, text: None, base64: None, mime_type: None, size };
        match kind {
            SourceFileKind::Markdown | SourceFileKind::Text => {
                let mut text = String::from_utf8_lossy(&fs::read(&full).map_err(|e| AppError::io("一次資料を読めません", e))?).into_owned();
                if let Some(max) = max_chars {
                    let total = text.encode_utf16().count();
                    if total > max {
                        let cut: String = text.chars().take(max).collect();
                        text = format!("{cut}\n\n…(truncated, {total} chars total)");
                    }
                }
                c.text = Some(text);
            }
            SourceFileKind::Image => {
                c.base64 = Some(base64::engine::general_purpose::STANDARD.encode(fs::read(&full).map_err(|e| AppError::io("一次資料を読めません", e))?));
                c.mime_type = mime.map(str::to_string);
            }
            SourceFileKind::Pdf => c.mime_type = mime.map(str::to_string),
            SourceFileKind::Other => {}
        }
        Ok(c)
    }

    /// (Re)build the index of text sources. Incremental: unchanged files (same mtime + size) are skipped, deleted ones removed.
    pub fn reindex_sources(&self) -> Result<ReindexResult> {
        let base = self.sources_dir()?;
        let files = self.list_sources("")?;
        self.with_db(|c| {
            let mut known: HashMap<String, (i64, String)> = HashMap::new();
            {
                let mut st = c.prepare("SELECT path, size, mtime FROM source_index")?;
                for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))? {
                    let (p, s, m) = r?;
                    known.insert(p, (s, m));
                }
            }
            let tx = c.unchecked_transaction()?;
            let mut indexed = 0;
            let mut removed = 0;
            let mut seen = std::collections::HashSet::new();
            for f in &files {
                seen.insert(f.path.clone());
                if known.get(&f.path).is_some_and(|(s, m)| *s == f.size as i64 && *m == f.mtime) {
                    continue;
                }
                let content: Option<String> = if matches!(f.kind, SourceFileKind::Markdown | SourceFileKind::Text) {
                    fs::read(base.join(&f.path)).ok().map(|b| String::from_utf8_lossy(&b).chars().take(MAX_INDEXED_CHARS).collect())
                } else {
                    None
                };
                tx.execute(
                    "INSERT INTO source_index (path, kind, size, mtime, content, indexed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT (path) DO UPDATE SET kind = excluded.kind, size = excluded.size, mtime = excluded.mtime, content = excluded.content, indexed_at = excluded.indexed_at",
                    params![f.path, f.kind.as_str(), f.size as i64, f.mtime, content, super::entities::now_iso()],
                )?;
                indexed += 1;
            }
            for p in known.keys() {
                if !seen.contains(p) {
                    tx.execute("DELETE FROM source_index WHERE path = ?1", [p])?;
                    removed += 1;
                }
            }
            if indexed + removed > 0 {
                tx.execute("INSERT INTO source_fts(source_fts) VALUES ('rebuild')", [])?;
            }
            tx.commit()?;
            Ok(ReindexResult { indexed, removed, total: files.len() })
        })
    }

    /// Substring search (case-insensitive; Thai / Japanese have no word boundaries) over every whitespace-separated term;
    /// file names are matched too. Score = 2 per term found in content + 1 per term found in the path.
    /// Candidates come from the FTS5 trigram index when every term has >= 3 chars, otherwise from a table scan.
    pub fn search_sources(&self, query: &str, limit: usize) -> Result<Vec<SourceSearchHit>> {
        let terms: Vec<String> = query.split_whitespace().take(8).map(str::to_string).collect();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        self.reindex_sources()?;
        let lowered: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
        let use_fts = terms.iter().all(|t| t.chars().count() >= 3);
        let rows: Vec<(String, String, Option<String>)> = self.with_db(|c| {
            let map = |r: &rusqlite::Row<'_>| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?));
            if use_fts {
                let m = terms.iter().map(|t| format!("\"{}\"", t.replace('"', "\"\""))).collect::<Vec<_>>().join(" OR ");
                let mut sql = String::from(
                    "SELECT path, kind, content FROM source_index WHERE rowid IN (SELECT rowid FROM source_fts WHERE source_fts MATCH ?1) OR ",
                );
                sql.push_str(&(0..lowered.len()).map(|i| format!("instr(lower(path), ?{}) > 0", i + 2)).collect::<Vec<_>>().join(" OR "));
                let mut st = c.prepare(&sql)?;
                let mut args: Vec<&dyn rusqlite::ToSql> = vec![&m];
                args.extend(lowered.iter().map(|t| t as &dyn rusqlite::ToSql));
                let r = st.query_map(args.as_slice(), map)?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(r)
            } else {
                let mut st = c.prepare("SELECT path, kind, content FROM source_index")?;
                let r = st.query_map([], map)?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(r)
            }
        })?;
        let mut hits = vec![];
        for (path, kind, content) in rows {
            let content = content.unwrap_or_default();
            let lc = content.to_lowercase();
            let lp = path.to_lowercase();
            let mut score = 0u32;
            let mut first: Option<usize> = None;
            for t in &lowered {
                if let Some(i) = lc.find(t.as_str()) {
                    score += 2;
                    first = Some(first.map_or(i, |f| f.min(i)));
                }
                if lp.contains(t.as_str()) {
                    score += 1;
                }
            }
            if score == 0 {
                continue;
            }
            // `first` indexes the lower-cased text; map it back by char count so the excerpt comes from the original
            let snippet = match first {
                Some(i) => {
                    let char_idx = lc[..i].chars().count();
                    let byte = content.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(content.len());
                    excerpt(&content, byte, 160)
                }
                None => String::new(),
            };
            hits.push(SourceSearchHit { path, kind: SourceFileKind::parse(&kind), snippet, score });
        }
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
        hits.truncate(limit);
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::ErrorCode;
    use super::super::testutil::core_with_dataset;
    use super::*;

    fn put(core: &Core, rel: &str, content: &[u8]) {
        let p = core.dataset_root().unwrap().join("sources").join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn list_kinds_and_filters() {
        let (_t, core) = core_with_dataset();
        put(&core, "notes/b.md", "# b".as_bytes());
        put(&core, "images/a.JPG", b"\xff\xd8");
        put(&core, "x.pdf", b"%PDF");
        put(&core, "data.bin", b"1");
        put(&core, ".hidden/h.md", b"h");
        put(&core, "node_modules/m.md", b"m");
        let list = core.list_sources("").unwrap();
        let got: Vec<(&str, SourceFileKind)> = list.iter().map(|f| (f.path.as_str(), f.kind)).collect();
        assert_eq!(got, [("data.bin", SourceFileKind::Other), ("images/a.JPG", SourceFileKind::Image), ("notes/b.md", SourceFileKind::Markdown), ("x.pdf", SourceFileKind::Pdf)]);
        assert_eq!(list[1].ext, ".JPG");
        assert_eq!(list[2].name, "b.md");
        assert_eq!(core.list_sources("notes").unwrap().len(), 1);
        assert_eq!(core.list_sources("nope").unwrap().len(), 0);
    }

    #[test]
    fn path_traversal_is_rejected_everywhere() {
        let (_t, core) = core_with_dataset();
        put(&core, "a.md", b"a");
        for bad in ["../data/monks/x.md", "..", "a/../../b"] {
            assert_eq!(core.read_source(bad, None).unwrap_err().code, ErrorCode::InvalidArgument, "{bad}");
            assert_eq!(core.read_source_bytes(bad).unwrap_err().code, ErrorCode::InvalidArgument);
            assert_eq!(core.list_sources(bad).unwrap_err().code, ErrorCode::InvalidArgument);
        }
        assert_eq!(core.read_source("missing.md", None).unwrap_err().code, ErrorCode::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_leaving_the_sources_dir_are_hidden_and_unreadable() {
        let (t, core) = core_with_dataset();
        put(&core, "ok.md", b"ok");
        let outside = t.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.md"), "secret").unwrap();
        let src = core.dataset_root().unwrap().join("sources");
        std::os::unix::fs::symlink(&outside, src.join("link")).unwrap();
        std::os::unix::fs::symlink(src.join("ok.md"), src.join("alias.md")).unwrap();
        let paths: Vec<_> = core.list_sources("").unwrap().into_iter().map(|f| f.path).collect();
        assert_eq!(paths, ["alias.md", "ok.md"]);
        assert!(core.read_source("link/secret.md", None).is_err());
        assert!(core.read_source("alias.md", None).is_ok());
    }

    #[test]
    fn read_text_truncate_image_and_pdf() {
        let (_t, core) = core_with_dataset();
        put(&core, "t.md", "あいうえおかきくけこ".as_bytes());
        put(&core, "i.png", b"\x89PNG");
        put(&core, "d.pdf", b"%PDF");
        let t = core.read_source("t.md", Some(5)).unwrap();
        assert_eq!(t.text.unwrap(), "あいうえお\n\n…(truncated, 10 chars total)");
        let full = core.read_source("t.md", None).unwrap();
        assert_eq!(full.text.unwrap(), "あいうえおかきくけこ");
        let i = core.read_source("i.png", None).unwrap();
        assert_eq!(i.mime_type.as_deref(), Some("image/png"));
        assert_eq!(i.base64.unwrap(), "iVBORw==");
        let p = core.read_source("d.pdf", None).unwrap();
        assert!(p.text.is_none() && p.base64.is_none() && p.mime_type.as_deref() == Some("application/pdf"));
        let (bytes, mime) = core.read_source_bytes("i.png").unwrap();
        assert_eq!((bytes.as_slice(), mime), (&b"\x89PNG"[..], Some("image/png")));
        assert_eq!(core.read_source("notes", None).unwrap_err().code, ErrorCode::NotFound);
    }

    #[test]
    fn search_thai_japanese_and_english() {
        let (_t, core) = core_with_dataset();
        put(&core, "notes/somdet-to.md", "# ソムデット・トー\nวัดระฆังโฆสิตาราม ที่กรุงเทพ\nBorn 1788 at Phra Nakhon".as_bytes());
        put(&core, "notes/other.md", "ワット・パクナム の記録".as_bytes());
        put(&core, "img/x.png", b"\x89PNG");
        let r = core.search_sources("วัดระฆัง", 20).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, "notes/somdet-to.md");
        assert!(r[0].snippet.contains("วัดระฆัง"));
        assert_eq!(core.search_sources("ソムデット", 20).unwrap()[0].path, "notes/somdet-to.md");
        assert_eq!(core.search_sources("PHRA nakhon", 20).unwrap().len(), 1, "case-insensitive; each term counted");
        // short terms (< 3 chars) take the scan path
        assert_eq!(core.search_sources("ト一", 20).unwrap().len(), 0);
        assert_eq!(core.search_sources("トー", 20).unwrap().len(), 1);
        // file-name match without content match
        let r = core.search_sources("somdet", 20).unwrap();
        assert_eq!(r[0].path, "notes/somdet-to.md");
        assert_eq!(r[0].score, 1, "path match only (the content holds the katakana form)");
        assert!(core.search_sources("   ", 20).unwrap().is_empty());
        assert!(core.search_sources("zzzzzz", 20).unwrap().is_empty());
        // images are indexed by path only
        assert_eq!(core.search_sources("img", 20).unwrap()[0].kind, SourceFileKind::Image);
    }

    #[test]
    fn ranking_limit_and_special_chars() {
        let (_t, core) = core_with_dataset();
        put(&core, "a.md", b"alpha beta");
        put(&core, "b.md", b"alpha");
        put(&core, "c.md", b"alpha beta gamma");
        let r = core.search_sources("alpha beta", 20).unwrap();
        let paths: Vec<_> = r.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, ["a.md", "c.md", "b.md"], "score desc, then path");
        assert_eq!(core.search_sources("alpha", 2).unwrap().len(), 2);
        // FTS syntax characters in a term must not break the query
        put(&core, "q.md", b"say \"hi\" OR NOT (x)");
        assert_eq!(core.search_sources("\"hi\"", 20).unwrap().len(), 1);
        assert_eq!(core.search_sources("OR NOT", 20).unwrap().len(), 1);
    }

    #[test]
    fn reindex_is_incremental_and_tracks_deletes() {
        let (_t, core) = core_with_dataset();
        put(&core, "a.md", b"first");
        put(&core, "b.md", b"second");
        let r = core.reindex_sources().unwrap();
        assert_eq!((r.indexed, r.removed, r.total), (2, 0, 2));
        let r = core.reindex_sources().unwrap();
        assert_eq!((r.indexed, r.removed, r.total), (0, 0, 2), "unchanged files are skipped");
        fs::remove_file(core.dataset_root().unwrap().join("sources/b.md")).unwrap();
        put(&core, "a.md", b"changed text now");
        let r = core.reindex_sources().unwrap();
        assert_eq!((r.indexed, r.removed, r.total), (1, 1, 1));
        assert!(core.search_sources("second", 5).unwrap().is_empty());
        assert_eq!(core.search_sources("changed", 5).unwrap().len(), 1);
        assert!(core.search_sources("first", 5).unwrap().is_empty(), "stale content is gone from the FTS index");
    }
}
