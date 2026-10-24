//! Cleared literature (FORMAT.md §10): papers by id and PDF page, page text, page images, and the
//! four citation checks: clearance, source hash, page existence and quotation.
//!
//! Every file is read through the access policy. A matching quotation shows the words are on the
//! page, not that they support the claim; OCR'd page text and anything from a table or figure still
//! needs the page image. `image_command` runs a program named in plied.toml, so that file is part
//! of the trust boundary; what it produces is read through the policy like the rest.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use serde_json::Value;

use crate::access::Policy;
use crate::config::{Arg, Config, Literature as Settings, Pattern, render_template};
use crate::doc::{self, Map, Node, take};
use crate::error::{Fail, Res};
use crate::fsx;
use crate::textmatch;

pub struct Paper {
    pub id: String,
    pub file: String,
    pub sha256: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aspect {
    Clearance,
    Hash,
    Page,
    Quote,
    Image,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    Fail,
    Warn,
    Unchecked,
}

impl fmt::Display for Aspect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Aspect::Clearance => "clearance",
            Aspect::Hash => "hash",
            Aspect::Page => "page",
            Aspect::Quote => "quote",
            Aspect::Image => "image",
        })
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Status::Ok => "ok",
            Status::Fail => "fail",
            Status::Warn => "warn",
            Status::Unchecked => "unchecked",
        })
    }
}

pub struct Check {
    pub aspect: Aspect,
    pub status: Status,
    pub message: String,
}

fn check(aspect: Aspect, status: Status, message: impl Into<String>) -> Check {
    Check { aspect, status, message: message.into() }
}

pub struct Library<'a> {
    base: PathBuf,
    pub settings: Settings,
    policy: &'a Policy,
    runtime: PathBuf,
    pub papers: BTreeMap<String, Paper>,
    /// manifest entries that could not be used
    pub problems: Vec<String>,
    page_re: Regex,
}

/// Matches page text file names, capturing the page number.
fn page_regex(page_file: &str) -> Regex {
    let field = Regex::new(r"\{page(:[^}]*)?\}").expect("static pattern");
    let mut pat = String::from("^");
    let mut last = 0;
    for m in field.find_iter(page_file) {
        pat.push_str(&regex::escape(&page_file[last..m.start()]));
        pat.push_str("([0-9]+)");
        last = m.end();
    }
    pat.push_str(&regex::escape(&page_file[last..]));
    pat.push('$');
    Regex::new(&pat).expect("escaped pattern")
}

fn scalar_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

impl<'a> Library<'a> {
    pub fn new(cfg: &Config, policy: &'a Policy, runtime: &Path) -> Res<Library<'a>> {
        let Some(settings) = cfg.literature.clone() else { return Err(Fail::lit("no [literature] section in plied.toml")) };
        let manifest = cfg.base.join(&settings.manifest);
        if !manifest.exists() {
            return Err(Fail::lit(format!("literature manifest {} not found", settings.manifest)));
        }
        policy.check_readable(&manifest)?;
        let data: Value = serde_json::from_str(&fsx::read_text(&manifest)?).map_err(|e| Fail::new(format!("{}: {e}", settings.manifest)))?;
        let entries = match &data {
            Value::Object(o) => o.get(&settings.list_key).and_then(Value::as_array).cloned().unwrap_or_default(),
            Value::Array(a) => a.clone(),
            _ => vec![],
        };
        let id_from_file = settings.id_from_file.as_deref().and_then(|p| Pattern::new(p, "").ok());
        let (mut papers, mut problems) = (BTreeMap::new(), vec![]);
        for e in &entries {
            let Some(e) = e.as_object() else {
                problems.push(format!("manifest entry is not an object: {e}"));
                continue;
            };
            let file = e.get(&settings.file_field).and_then(scalar_text).unwrap_or_default();
            let id = settings
                .id_field
                .as_ref()
                .and_then(|k| e.get(k))
                .and_then(scalar_text)
                .or_else(|| id_from_file.as_ref().and_then(|r| r.group1(&file)));
            let Some(id) = id else {
                problems.push(format!("manifest entry without an id: {}", if file.is_empty() { Value::Object(e.clone()).to_string() } else { file }));
                continue;
            };
            if papers.contains_key(&id) {
                problems.push(format!("duplicate paper id {id}"));
                continue;
            }
            let sha256 = e.get(&settings.hash_field).and_then(scalar_text);
            papers.insert(id.clone(), Paper { id, file, sha256 });
        }
        let page_re = page_regex(&settings.page_file);
        Ok(Library { base: cfg.base.clone(), settings, policy, runtime: runtime.to_path_buf(), papers, problems, page_re })
    }

    // ------------------------------------------------------------------------------ locations

    pub fn paper(&self, id: &str) -> Res<&Paper> {
        self.papers.get(id).ok_or_else(|| Fail::lit(format!("{id} is not a cleared paper (not in {})", self.settings.manifest)))
    }

    fn template(&self, t: &str, p: &Paper, page: Option<i64>) -> String {
        let mut args = vec![("id", Arg::S(&p.id)), ("file", Arg::S(&p.file))];
        if let Some(n) = page {
            args.push(("page", Arg::I(n)));
        }
        render_template(t, &args).unwrap_or_default() // checked when plied.toml was loaded
    }

    pub fn pdf_path(&self, p: &Paper) -> Option<PathBuf> {
        let dir = self.settings.pdf_dir.as_ref()?;
        (!p.file.is_empty()).then(|| self.base.join(dir).join(&p.file))
    }

    pub fn text_dir(&self, p: &Paper) -> PathBuf {
        match &self.settings.text_dir {
            Some(t) => self.base.join(self.template(t, p, None)),
            None => self.runtime.join("papers").join(&p.id),
        }
    }

    pub fn page_path(&self, p: &Paper, page: i64) -> PathBuf {
        self.text_dir(p).join(self.template(&self.settings.page_file, p, Some(page)))
    }

    /// The pages with text, in order.
    pub fn pages(&self, p: &Paper) -> Vec<i64> {
        let d = self.text_dir(p);
        if !d.exists() || !self.policy.literature_readable(&d) {
            return vec![];
        }
        let mut out: Vec<i64> = std::fs::read_dir(&d)
            .map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| self.page_re.captures(&e.file_name().to_string_lossy()).and_then(|m| m[1].parse().ok())).collect())
            .unwrap_or_default();
        out.sort();
        out
    }

    pub fn page_text(&self, p: &Paper, page: i64) -> Res<Option<String>> {
        let path = self.page_path(p, page);
        if !path.exists() {
            return Ok(None);
        }
        if !self.policy.literature_readable(&path) {
            return Err(Fail::policy(format!("{} is not readable under the access policy", path.display())));
        }
        fsx::read_text(&path).map(Some)
    }

    pub fn is_ocr(&self, text: &str) -> bool {
        self.settings.ocr_marker.as_deref().is_some_and(|m| !m.is_empty() && text.contains(m))
    }

    // ----------------------------------------------------------------------------------- hash

    pub fn hash_status(&self, p: &Paper) -> Check {
        let Some(expected) = &p.sha256 else { return check(Aspect::Hash, Status::Unchecked, "the manifest gives no hash") };
        let Some(pdf) = self.pdf_path(p) else { return check(Aspect::Hash, Status::Unchecked, "no pdf_dir configured") };
        if !pdf.exists() {
            return check(Aspect::Hash, Status::Unchecked, format!("{} is not in this checkout", p.file));
        }
        if let Err(e) = self.policy.check_readable(&pdf) {
            return check(Aspect::Hash, Status::Fail, e.msg);
        }
        match self.pdf_digest(&pdf) {
            Err(e) => check(Aspect::Hash, Status::Fail, format!("{}: {e}", p.file)),
            Ok(d) if &d != expected => check(
                Aspect::Hash,
                Status::Fail,
                format!("{}: sha256 {}… differs from the manifest {}…", p.file, take(&d, 12), take(expected, 12)),
            ),
            Ok(_) => check(Aspect::Hash, Status::Ok, "matches the manifest"),
        }
    }

    /// The PDF's sha256, cached by path, size and modification time.
    fn pdf_digest(&self, pdf: &Path) -> std::io::Result<String> {
        let cache_path = self.runtime.join("hashcache.json");
        let mut cache: BTreeMap<String, String> =
            std::fs::read_to_string(&cache_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let md = std::fs::metadata(pdf)?;
        let real = fsx::resolve(pdf).display().to_string();
        let key = format!("{real}|{}|{}", md.len(), fsx::mtime_ns(&md));
        if let Some(d) = cache.get(&key) {
            return Ok(d.clone());
        }
        let digest = fsx::sha(&std::fs::read(pdf)?);
        cache.retain(|k, _| k.rsplitn(3, '|').nth(2) != Some(real.as_str())); // older versions of this file
        cache.insert(key, digest.clone());
        if let Ok(text) = serde_json::to_string(&cache) {
            let _ = fsx::atomic_write(&cache_path, text.as_bytes());
        }
        Ok(digest)
    }

    // ------------------------------------------------------------------------------- citation

    /// The five results for a citation, always all of them and in this order: clearance, hash, page,
    /// quote, image (FORMAT.md §10). A result that cannot be reached because an earlier one failed
    /// is `unchecked`, saying why.
    pub fn check_citation(&self, cite: &Map) -> Vec<Check> {
        let skipped = |a: Aspect, why: &str| check(a, Status::Unchecked, format!("not checked: {why}"));
        let id = doc::text(cite, "paper").unwrap_or_default();
        let figure = doc::str_of(cite, "kind").filter(|k| matches!(*k, "table" | "figure"));
        // whether the value was read from the page image is a claim the record makes, independent of the page text
        let image = match figure {
            None => check(Aspect::Image, Status::Unchecked, "not a table or figure value"),
            Some(k) if doc::str_of(cite, "image_checked") == Some("true") => check(Aspect::Image, Status::Ok, format!("the {k} value is recorded as read from the page image")),
            Some(k) => check(Aspect::Image, Status::Warn, format!("a {k} value: record image_checked: true once read from the page image")),
        };
        let p = match self.paper(&id) {
            Ok(p) => p,
            Err(e) => {
                let why = "the paper is not cleared";
                return vec![check(Aspect::Clearance, Status::Fail, e.msg), skipped(Aspect::Hash, why), skipped(Aspect::Page, why), skipped(Aspect::Quote, why), image];
            }
        };
        let mut out = vec![check(Aspect::Clearance, Status::Ok, format!("{id} is in the manifest")), self.hash_status(p)];
        let quote = doc::text(cite, "quote");
        let page = cite.get("page").and_then(Node::int);
        let pages = self.pages(p);
        let page_result = match (page, pages.first(), pages.last()) {
            (None, ..) => {
                let got = cite.get("page").map_or("nothing".into(), |v| format!("{v:?}"));
                Err(check(Aspect::Page, Status::Fail, format!("page must be a PDF page number, got {got}")))
            }
            (Some(_), None, _) | (Some(_), _, None) => {
                Err(check(Aspect::Page, Status::Unchecked, format!("no page text for {id} (run `plied lit extract {id}`)")))
            }
            (Some(n), Some(first), Some(last)) if !pages.contains(&n) => {
                Err(check(Aspect::Page, Status::Fail, format!("{id} has no page {n} (pages {first}–{last})")))
            }
            (Some(n), ..) => Ok(n),
        };
        let page = match page_result {
            Ok(n) => {
                out.push(check(Aspect::Page, Status::Ok, format!("page {n} exists")));
                n
            }
            Err(c) => {
                let why = if c.status == Status::Fail { "the page check failed" } else { "there is no page text to match against" };
                out.extend([c, skipped(Aspect::Quote, why), image]);
                return out;
            }
        };
        out.push(match quote {
            // a value read from a table or figure may have no quotable text: the image result covers it
            None if figure.is_some() => check(Aspect::Quote, Status::Unchecked, "no quotation (a table or figure value); only the page is checked"),
            None => check(Aspect::Quote, Status::Warn, "no quotation, so nothing ties the citation to the page's text"),
            Some(q) => self.quote_check(p, page, &q),
        });
        out.push(image);
        out
    }

    fn quote_check(&self, p: &Paper, page: i64, quote: &str) -> Check {
        let text = match self.page_text(p, page) {
            Ok(t) => t.unwrap_or_default(),
            Err(e) => return check(Aspect::Quote, Status::Fail, e.msg),
        };
        let ocr = self.is_ocr(&text);
        match (textmatch::find_quote(quote, &text), ocr) {
            (true, true) => check(Aspect::Quote, Status::Warn, "matches OCR'd text: verify against the page image"),
            (true, false) => check(Aspect::Quote, Status::Ok, "found on the page"),
            (false, true) => check(Aspect::Quote, Status::Warn, "not found in OCR'd text: verify against the page image"),
            (false, false) => {
                let near = textmatch::closest(quote, &text, 60).map(|n| format!("; nearest: “{n}…”")).unwrap_or_default();
                check(Aspect::Quote, Status::Fail, format!("not found on the page{near}"))
            }
        }
    }

    // ---------------------------------------------------------------------- extraction, images

    /// Write page text with pdfium into the runtime cache (when no text_dir is configured).
    pub fn extract(&self, p: &Paper) -> Res<usize> {
        if self.settings.text_dir.is_some() {
            return Err(Fail::lit("page text comes from literature.text_dir; extract with the project's own tool"));
        }
        let pdf = self.pdf_path(p).filter(|x| x.exists()).ok_or_else(|| Fail::lit(format!("no PDF for {}", p.id)))?;
        self.policy.check_readable(&pdf)?;
        let pages = crate::pdf::page_texts(&pdf)?;
        let d = self.text_dir(p);
        std::fs::create_dir_all(&d)?;
        for (i, text) in pages.iter().enumerate() {
            std::fs::write(self.page_path(p, i as i64 + 1), text)?;
        }
        Ok(pages.len())
    }

    /// A rendering of the page: from `image_command`, or pdfium.
    pub fn image(&self, p: &Paper, page: i64) -> Res<PathBuf> {
        if let Some(cmd) = &self.settings.image_command {
            return self.image_from_command(cmd, p, page);
        }
        let pdf = self.pdf_path(p).filter(|x| x.exists()).ok_or_else(|| Fail::lit(format!("no PDF for {}", p.id)))?;
        self.policy.check_readable(&pdf)?;
        let out = self.runtime.join("paper-img").join(format!("{}-p{page:02}.png", p.id));
        crate::pdf::render_page(&pdf, page, &out)?;
        Ok(out)
    }

    fn image_from_command(&self, cmd: &[String], p: &Paper, page: i64) -> Res<PathBuf> {
        let args: Vec<String> = cmd.iter().map(|a| self.template(a, p, Some(page))).collect();
        let r = Command::new(&args[0])
            .args(&args[1..])
            .current_dir(&self.base)
            .output()
            .map_err(|e| Fail::lit(format!("image command could not run: {e}")))?;
        if !r.status.success() {
            let err = String::from_utf8_lossy(&r.stderr);
            let err = err.trim();
            let tail = &err[err.char_indices().rev().nth(399).map_or(0, |(i, _)| i)..];
            return Err(Fail::lit(format!("image command failed: {tail}")));
        }
        let mut candidates = vec![];
        if let Some(ip) = &self.settings.image_path {
            candidates.push(self.base.join(self.template(ip, p, Some(page))));
        }
        let stdout = String::from_utf8_lossy(&r.stdout);
        if let Some(last) = stdout.trim().lines().last().map(str::trim).filter(|l| !l.is_empty()) {
            candidates.push(self.base.join(last));
        }
        let out = candidates
            .into_iter()
            .find(|c| c.exists())
            .ok_or_else(|| Fail::lit("image command ran but its output was not found (set literature.image_path)"))?;
        self.policy.check_readable(&out)?;
        Ok(out)
    }
}
