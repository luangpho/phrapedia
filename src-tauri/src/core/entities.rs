//! Records: data/<kind>/<slug>.md (port of server/utils/entities.ts).
//! Reads prefer the export copy and fall back to the dataset; writes always go to the export side.

use super::error::{AppError, ErrorCode, Result};
use super::frontmatter::{self, Data};
use super::paths;
use super::slug::{is_valid_slug, short_id, slugify};
use super::types::{EntityKind, EntityRecord, EntityStatus, EntitySummary};
use super::Core;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const LIST_FIELDS: [&str; 3] = ["aliases", "sources", "tags"];

pub fn assert_kind(kind: &str) -> Result<EntityKind> {
    EntityKind::parse(kind).ok_or_else(|| AppError::invalid(format!("不明な種別です: {kind}")))
}

pub fn assert_slug(slug: &str) -> Result<String> {
    if !is_valid_slug(slug) {
        return Err(AppError::invalid(format!("slug が不正です: {slug}")));
    }
    Ok(slug.to_string())
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WriteOptions {
    pub must_not_exist: bool,
    pub must_exist: bool,
}

fn to_status(v: Option<&Value>) -> EntityStatus {
    match v.and_then(Value::as_str) {
        Some("verified") => EntityStatus::Verified,
        Some("reviewed") => EntityStatus::Reviewed,
        _ => EntityStatus::Draft,
    }
}

/// Hand-written frontmatter may use a scalar where a list is expected; normalise to a list of strings.
/// `None` means "drop the key" (null / empty string).
fn to_string_list(v: &Value) -> Option<Value> {
    let items: Vec<&Value> = match v {
        Value::Null => return None,
        Value::String(s) if s.is_empty() => return None,
        Value::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    let list: Vec<Value> = items
        .into_iter()
        .filter(|x| !x.is_null() && x.as_str() != Some(""))
        .map(|x| match x {
            Value::String(s) => Value::String(s.clone()),
            other => Value::String(other.to_string()),
        })
        .collect();
    Some(Value::Array(list))
}

fn normalise_lists(fm: &mut Data) {
    for key in LIST_FIELDS {
        if let Some(v) = fm.get(key).cloned() {
            match to_string_list(&v) {
                Some(n) => {
                    fm.insert(key.to_string(), n);
                }
                None => {
                    fm.shift_remove(key);
                }
            }
        }
    }
}

fn title_from_body(body: &str) -> Option<String> {
    body.lines().find_map(|l| l.strip_prefix("# ").map(str::trim).filter(|s| !s.is_empty()).map(str::to_string))
}

fn name_of(data: &Data, body: &str, slug: &str) -> String {
    match data.get("name").and_then(Value::as_str) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => title_from_body(body).unwrap_or_else(|| slug.to_string()),
    }
}

fn iso(t: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(t).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Display path relative to the dataset root; absolute when the data dir lives outside it.
fn rel_path(dataset_root: &Path, data_root: &Path, kind: EntityKind, slug: &str) -> String {
    let base = if paths::is_inside(dataset_root, data_root) {
        let r = paths::to_posix(data_root.strip_prefix(dataset_root).unwrap_or(Path::new("")));
        if r.is_empty() { ".".to_string() } else { r }
    } else {
        paths::to_posix(data_root)
    };
    format!("{base}/{}/{slug}.md", kind.as_str())
}

fn str_field(fm: &Data, key: &str) -> Option<String> {
    fm.get(key).and_then(Value::as_str).map(str::to_string)
}

fn to_record(dataset_root: &Path, data_root: &Path, kind: EntityKind, slug: &str, raw: &str, mtime: Option<SystemTime>) -> Result<EntityRecord> {
    let parsed = frontmatter::parse(raw)?;
    let mut fm = parsed.data;
    fm.insert("slug".into(), json!(slug));
    fm.insert("kind".into(), json!(kind.as_str()));
    let name = name_of(&fm, &parsed.body, slug);
    fm.insert("name".into(), json!(name));
    normalise_lists(&mut fm);
    Ok(EntityRecord {
        kind,
        slug: slug.to_string(),
        name,
        name_th: str_field(&fm, "name_th"),
        name_en: str_field(&fm, "name_en"),
        status: to_status(fm.get("status")),
        updated_at: str_field(&fm, "updated_at").or_else(|| mtime.map(iso)),
        path: rel_path(dataset_root, data_root, kind, slug),
        frontmatter: fm,
        body: parsed.body,
        raw: raw.to_string(),
    })
}

/// Name for sorting: case-insensitive, otherwise code-point order (no ICU collation).
fn sort_key(name: &str) -> String {
    name.to_lowercase()
}

impl Core {
    /// Directory records of `kind` are read from. Created best-effort (the dataset may be read-only).
    fn kind_dir(&self, kind: EntityKind) -> Result<PathBuf> {
        let dir = self.data_dir()?.join(kind.as_str());
        let _ = fs::create_dir_all(&dir);
        Ok(dir)
    }

    fn write_kind_dir(&self, kind: EntityKind) -> Result<PathBuf> {
        let dir = self.export_data_dir()?.join(kind.as_str());
        fs::create_dir_all(&dir).map_err(|e| AppError::io("書き出し先フォルダを作れません", e))?;
        Ok(dir)
    }

    fn entity_write_path(&self, kind: EntityKind, slug: &str) -> Result<PathBuf> {
        Ok(self.write_kind_dir(kind)?.join(format!("{slug}.md")))
    }

    /// Read path: the export copy when it exists (it is the newer one), else the dataset copy.
    fn entity_path(&self, kind: EntityKind, slug: &str) -> Result<PathBuf> {
        let read = self.kind_dir(kind)?.join(format!("{slug}.md"));
        if self.same_roots() {
            return Ok(read);
        }
        let written = self.export_data_dir()?.join(kind.as_str()).join(format!("{slug}.md"));
        Ok(if written.is_file() { written } else { read })
    }

    pub fn list_entities(&self, kind: EntityKind) -> Result<Vec<EntitySummary>> {
        let dataset = self.dataset_root()?;
        let data_root = self.data_dir()?;
        let mut by_slug: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut add_dir = |dir: &Path| {
            let Ok(rd) = fs::read_dir(dir) else { return };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some(stem) = name.strip_suffix(".md") {
                    if !name.starts_with('.') && name.to_lowercase() != "readme.md" {
                        by_slug.insert(stem.to_string(), e.path());
                    }
                }
            }
        };
        add_dir(&self.kind_dir(kind)?);
        if !self.same_roots() {
            add_dir(&self.export_data_dir()?.join(kind.as_str()));
        }
        let mut out = vec![];
        for (slug, full) in by_slug {
            let summary = (|| -> Result<EntitySummary> {
                let raw = fs::read_to_string(&full)?;
                let mtime = fs::metadata(&full).and_then(|m| m.modified()).ok();
                let r = to_record(&dataset, &data_root, kind, &slug, &raw, mtime)?;
                Ok(EntitySummary { kind, slug: slug.clone(), name: r.name, name_th: r.name_th, name_en: r.name_en, status: r.status, updated_at: r.updated_at })
            })();
            out.push(summary.unwrap_or(EntitySummary { kind, slug: slug.clone(), name: slug, name_th: None, name_en: None, status: EntityStatus::Draft, updated_at: None }));
        }
        out.sort_by_cached_key(|e| sort_key(&e.name));
        Ok(out)
    }

    pub fn get_entity(&self, kind: EntityKind, slug: &str) -> Result<Option<EntityRecord>> {
        let full = self.entity_path(kind, slug)?;
        let raw = match fs::read_to_string(&full) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(AppError::io("レコードを読めません", e)),
        };
        let mtime = fs::metadata(&full).and_then(|m| m.modified()).ok();
        Ok(Some(to_record(&self.dataset_root()?, &self.data_dir()?, kind, slug, &raw, mtime)?))
    }

    pub fn entity_exists(&self, kind: EntityKind, slug: &str) -> Result<bool> {
        Ok(self.entity_path(kind, slug)?.is_file())
    }

    /// Slug for a new record: frontmatter slug > name_en > name > name_th > random id; made unique with -2, -3, ...
    pub fn derive_slug(&self, kind: EntityKind, fm: &Data) -> Result<String> {
        let mut base = String::new();
        for key in ["slug", "name_en", "name", "name_th"] {
            if let Some(v) = fm.get(key).and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                let s = slugify(v);
                if is_valid_slug(&s) {
                    base = s;
                    break;
                }
            }
        }
        if base.is_empty() {
            let k = kind.as_str();
            base = format!("{}-{}", &k[..k.len() - 1], short_id(6));
        }
        let mut slug = base.clone();
        let mut n = 2;
        while self.entity_exists(kind, &slug)? {
            slug = format!("{base}-{n}");
            n += 1;
        }
        Ok(slug)
    }

    /// Normalise and write a record. slug / kind are forced to match the path; created_at / updated_at are maintained.
    pub fn write_entity(&self, kind: EntityKind, slug: &str, raw: &str, opts: WriteOptions) -> Result<EntityRecord> {
        assert_slug(slug)?;
        let exists = self.entity_exists(kind, slug)?;
        if opts.must_not_exist && exists {
            return Err(AppError::conflict(format!("{}/{slug} は既に存在します", kind.as_str())));
        }
        if opts.must_exist && !exists {
            return Err(AppError::not_found(format!("{}/{slug} が見つかりません", kind.as_str())));
        }
        let parsed = frontmatter::parse(raw)?;
        let now = now_iso();
        let previous = if exists { self.get_entity(kind, slug)? } else { None };
        let mut fm = parsed.data;
        fm.insert("slug".into(), json!(slug));
        fm.insert("kind".into(), json!(kind.as_str()));
        let name = name_of(&fm, &parsed.body, slug);
        fm.insert("name".into(), json!(name));
        let status = match to_status(fm.get("status")) {
            EntityStatus::Draft => "draft",
            EntityStatus::Reviewed => "reviewed",
            EntityStatus::Verified => "verified",
        };
        fm.insert("status".into(), json!(status));
        let created = fm
            .get("created_at")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| previous.as_ref().and_then(|p| str_field(&p.frontmatter, "created_at")))
            .unwrap_or_else(|| now.clone());
        fm.insert("created_at".into(), json!(created));
        fm.insert("updated_at".into(), json!(now));
        normalise_lists(&mut fm);
        let content = frontmatter::stringify(&fm, &parsed.body);
        let full = self.entity_write_path(kind, slug)?;
        fs::write(&full, &content).map_err(|e| AppError::io("レコードを書き込めません", e))?;
        to_record(&self.dataset_root()?, &self.data_dir()?, kind, slug, &content, Some(SystemTime::now()))
    }

    /// Delete the export copy. Files that exist only on the (read-only) dataset side are never touched.
    pub fn delete_entity(&self, kind: EntityKind, slug: &str) -> Result<()> {
        assert_slug(slug)?;
        let full = self.entity_write_path(kind, slug)?;
        match fs::remove_file(&full) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::new(ErrorCode::NotFound, format!("{}/{slug} が見つかりません", kind.as_str()))),
            Err(e) => Err(AppError::io("レコードを削除できません", e)),
        }
    }
}

/// Starter markdown for a new record.
pub fn entity_template(kind: EntityKind, name: &str) -> String {
    let mut fm = Data::new();
    fm.insert("slug".into(), json!(""));
    fm.insert("kind".into(), json!(kind.as_str()));
    fm.insert("name".into(), json!(name));
    fm.insert("name_th".into(), json!(""));
    fm.insert("name_en".into(), json!(""));
    fm.insert("status".into(), json!("draft"));
    fm.insert("sources".into(), json!([]));
    fm.insert("tags".into(), json!([]));
    let (extra, body): (&[&str], String) = match kind {
        EntityKind::Monks => (
            &["temple", "province", "born", "died"],
            format!(
                "# {}\n\n## 概要\n\n## 生涯\n\n## 所属寺院\n\n## 代表的なプラクルアン\n\n## 出典\n- 一次資料: \n- Web（未確認）: \n",
                if name.is_empty() { "（高僧名）" } else { name }
            ),
        ),
        EntityKind::Temples => (
            &["province", "founded"],
            format!(
                "# {}\n\n## 概要\n\n## 歴史\n\n## 関連する高僧\n\n## 所在地\n\n## 出典\n- 一次資料: \n- Web（未確認）: \n",
                if name.is_empty() { "（寺院名）" } else { name }
            ),
        ),
        EntityKind::Amulets => (&[], format!("# {}\n\n## 概要\n\n## 出典\n", if name.is_empty() { "（プラクルアン名）" } else { name })),
    };
    for k in extra {
        fm.insert((*k).into(), json!(""));
    }
    frontmatter::stringify(&fm, &body)
}

#[cfg(test)]
mod tests {
    use super::super::testutil::core_with_dataset;
    use super::*;

    fn raw(name: &str) -> String {
        format!("---\nname: {name}\nname_en: Luang Pho Test\naliases: 別名\ncustom_key: keep-me\n---\n\n# {name}\n\n本文\n")
    }

    #[test]
    fn create_get_update_delete() {
        let (_t, core) = core_with_dataset();
        let slug = core.derive_slug(EntityKind::Monks, &frontmatter::parse(&raw("テスト")).unwrap().data).unwrap();
        assert_eq!(slug, "luang-pho-test");
        let rec = core.write_entity(EntityKind::Monks, &slug, &raw("テスト"), WriteOptions { must_not_exist: true, ..Default::default() }).unwrap();
        assert_eq!(rec.frontmatter["slug"], "luang-pho-test");
        assert_eq!(rec.frontmatter["kind"], "monks");
        assert_eq!(rec.frontmatter["status"], "draft");
        assert_eq!(rec.frontmatter["aliases"], json!(["別名"]), "scalar list fields become lists");
        assert_eq!(rec.frontmatter["custom_key"], "keep-me", "unknown keys are preserved");
        assert_eq!(rec.path, "data/monks/luang-pho-test.md");
        let created = rec.frontmatter["created_at"].as_str().unwrap().to_string();

        let got = core.get_entity(EntityKind::Monks, &slug).unwrap().unwrap();
        assert_eq!(got.raw, rec.raw);
        assert!(got.raw.starts_with("---\nname: テスト\n"), "key order of the source is kept: {}", got.raw);

        let upd = core.write_entity(EntityKind::Monks, &slug, &got.raw.replace("本文", "更新"), WriteOptions { must_exist: true, ..Default::default() }).unwrap();
        assert!(upd.body.contains("更新"));
        assert_eq!(upd.frontmatter["created_at"], created.as_str(), "created_at survives updates");

        let list = core.list_entities(EntityKind::Monks).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "テスト");
        assert_eq!(list[0].name_en.as_deref(), Some("Luang Pho Test"));

        core.delete_entity(EntityKind::Monks, &slug).unwrap();
        assert!(core.get_entity(EntityKind::Monks, &slug).unwrap().is_none());
        assert_eq!(core.delete_entity(EntityKind::Monks, &slug).unwrap_err().code, ErrorCode::NotFound);
    }

    #[test]
    fn conflicts_and_missing() {
        let (_t, core) = core_with_dataset();
        core.write_entity(EntityKind::Temples, "wat-a", "---\nname: A\n---\nx\n", WriteOptions::default()).unwrap();
        let e = core.write_entity(EntityKind::Temples, "wat-a", "---\nname: A\n---\nx\n", WriteOptions { must_not_exist: true, ..Default::default() }).unwrap_err();
        assert_eq!(e.code, ErrorCode::Conflict);
        let e = core.write_entity(EntityKind::Temples, "nope", "---\nname: A\n---\nx\n", WriteOptions { must_exist: true, ..Default::default() }).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotFound);
        assert_eq!(core.write_entity(EntityKind::Temples, "../evil", "x", WriteOptions::default()).unwrap_err().code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn slug_is_made_unique() {
        let (_t, core) = core_with_dataset();
        let fm = frontmatter::parse("---\nname_en: Wat A\n---\n").unwrap().data;
        let s1 = core.derive_slug(EntityKind::Temples, &fm).unwrap();
        core.write_entity(EntityKind::Temples, &s1, "---\nname: A\n---\n", WriteOptions::default()).unwrap();
        assert_eq!(core.derive_slug(EntityKind::Temples, &fm).unwrap(), "wat-a-2");
        let none = core.derive_slug(EntityKind::Monks, &Data::new()).unwrap();
        assert!(none.starts_with("monk-") && none.len() == 11, "{none}");
    }

    #[test]
    fn name_falls_back_to_heading_then_slug() {
        let (_t, core) = core_with_dataset();
        let r = core.write_entity(EntityKind::Monks, "h", "---\nstatus: reviewed\n---\n\n# 見出し名\n", WriteOptions::default()).unwrap();
        assert_eq!(r.name, "見出し名");
        assert_eq!(r.status, EntityStatus::Reviewed);
        let r = core.write_entity(EntityKind::Monks, "plain", "本文だけ\n", WriteOptions::default()).unwrap();
        assert_eq!(r.name, "plain");
    }

    #[test]
    fn invalid_frontmatter_is_reported() {
        let (_t, core) = core_with_dataset();
        let e = core.write_entity(EntityKind::Monks, "bad", "---\nfoo: [x\n---\n", WriteOptions::default()).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidFrontmatter);
    }

    #[test]
    fn list_skips_readme_and_dotfiles_and_survives_broken_files() {
        let (_t, core) = core_with_dataset();
        let dir = core.dataset_root().unwrap().join("data/monks");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("README.md"), "# readme").unwrap();
        fs::write(dir.join(".hidden.md"), "x").unwrap();
        fs::write(dir.join("broken.md"), "---\nfoo: [x\n---\n").unwrap();
        fs::write(dir.join("ok.md"), "---\nname: OK\n---\n").unwrap();
        let list = core.list_entities(EntityKind::Monks).unwrap();
        let slugs: Vec<_> = list.iter().map(|e| e.slug.as_str()).collect();
        assert_eq!(slugs, ["broken", "ok"], "sorted by name: broken (slug fallback) < ok");
    }

    #[test]
    fn export_side_wins_and_dataset_stays_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let (ds, ex) = (tmp.path().join("ds"), tmp.path().join("ex"));
        fs::create_dir_all(ds.join("data/monks")).unwrap();
        fs::create_dir_all(&ex).unwrap();
        fs::write(ds.join("data/monks/a.md"), "---\nname: 元\n---\nold\n").unwrap();
        fs::write(ds.join("data/monks/b.md"), "---\nname: B\n---\nb\n").unwrap();
        let core = Core::new(super::super::AppDirs { config_dir: tmp.path().join("c"), data_dir: tmp.path().join("d") });
        assert!(core.save_path_settings(ds.to_str().unwrap(), ex.to_str().unwrap()).unwrap().ok);

        assert_eq!(core.get_entity(EntityKind::Monks, "a").unwrap().unwrap().name, "元", "falls back to the dataset");
        let before = fs::read_to_string(ds.join("data/monks/a.md")).unwrap();
        core.write_entity(EntityKind::Monks, "a", "---\nname: 新\n---\nnew\n", WriteOptions { must_exist: true, ..Default::default() }).unwrap();
        assert_eq!(fs::read_to_string(ds.join("data/monks/a.md")).unwrap(), before, "dataset file is never modified");
        assert!(ex.join("data/monks/a.md").is_file());
        assert_eq!(core.get_entity(EntityKind::Monks, "a").unwrap().unwrap().name, "新");
        let names: Vec<_> = core.list_entities(EntityKind::Monks).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["B", "新"], "merged list, export wins on the same slug");
        // deleting something that exists only in the dataset is refused (read-only side)
        assert_eq!(core.delete_entity(EntityKind::Monks, "b").unwrap_err().code, ErrorCode::NotFound);
        assert!(ds.join("data/monks/b.md").is_file());
    }

    #[test]
    fn templates() {
        let m = entity_template(EntityKind::Monks, "");
        let p = frontmatter::parse(&m).unwrap();
        assert_eq!(p.data["kind"], "monks");
        assert_eq!(p.data["slug"], "");
        assert!(p.data.contains_key("temple") && p.data.contains_key("died"));
        assert!(p.body.contains("# （高僧名）") && p.body.contains("## 代表的なプラクルアン"));
        let t = entity_template(EntityKind::Temples, "ワット・A");
        assert!(t.contains("name: ワット・A\n") && t.contains("# ワット・A\n") && t.contains("founded: \"\""));
        let a = entity_template(EntityKind::Amulets, "");
        assert!(a.contains("# （プラクルアン名）"));
        assert_eq!(frontmatter::stringify(&frontmatter::parse(&m).unwrap().data, &frontmatter::parse(&m).unwrap().body), m);
    }

    /// Repo records, parsed and re-serialised without any change, must be byte-identical (no git diff on re-save).
    #[test]
    fn bundled_records_round_trip_without_diff() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data");
        let mut checked = 0;
        for kind in ["monks", "temples", "amulets"] {
            let Ok(rd) = fs::read_dir(root.join(kind)) else { continue };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if !name.ends_with(".md") || name.to_lowercase() == "readme.md" {
                    continue;
                }
                let raw = fs::read_to_string(e.path()).unwrap();
                let p = frontmatter::parse(&raw).unwrap();
                assert_eq!(frontmatter::stringify(&p.data, &p.body), raw, "{} changes on re-serialisation", name);
                checked += 1;
            }
        }
        assert!(checked >= 4, "expected the bundled records, found {checked}");
    }

    /// Opening and saving a bundled record changes only `updated_at` (the app stamps it on every save).
    #[test]
    fn saving_a_bundled_record_changes_only_updated_at() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data");
        let (_t, core) = core_with_dataset();
        let dst = core.dataset_root().unwrap().join("data");
        let mut checked = 0;
        for kind in [EntityKind::Monks, EntityKind::Temples] {
            fs::create_dir_all(dst.join(kind.as_str())).unwrap();
            for e in fs::read_dir(src.join(kind.as_str())).unwrap().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if !name.ends_with(".md") || name.to_lowercase() == "readme.md" {
                    continue;
                }
                let slug = name.trim_end_matches(".md").to_string();
                let original = fs::read_to_string(e.path()).unwrap();
                fs::write(dst.join(kind.as_str()).join(&name), &original).unwrap();
                let rec = core.get_entity(kind, &slug).unwrap().unwrap();
                let saved = core.write_entity(kind, &slug, &rec.raw, WriteOptions { must_exist: true, ..Default::default() }).unwrap();
                let a: Vec<&str> = original.lines().collect();
                let b: Vec<&str> = saved.raw.lines().collect();
                assert_eq!(a.len(), b.len(), "{name}");
                let diff: Vec<_> = a.iter().zip(&b).filter(|(x, y)| x != y).collect();
                assert_eq!(diff.len(), 1, "{name}: {diff:?}");
                assert!(diff[0].0.starts_with("updated_at:"), "{name}: {diff:?}");
                checked += 1;
            }
        }
        assert!(checked >= 4);
    }
}
