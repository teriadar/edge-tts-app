use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use serde::{Deserialize, Serialize};

pub mod metadata_fetcher;
pub use metadata_fetcher::{fetch_book_metadata_online, OnlineBookMetadata};

use crate::chunker::{ChapterData, ParagraphData, ParagraphKind};
use crate::tts::exceptions::{EdgeTtsError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpubBookData {
    pub title: String,
    pub author: String,
    pub language: String,
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub published_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub isbn: Option<String>,
    pub cover_href: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cover_image_path: Option<String>,
    #[serde(skip)]
    pub cover_bytes: Option<Vec<u8>>,
    pub chapters: Vec<ChapterData>,
}

#[derive(Debug, Clone, Default)]
pub struct OpfPackage {
    pub book_title: Option<String>,
    pub book_author: Option<String>,
    pub book_language: Option<String>,
    pub book_description: Option<String>,
    pub book_publisher: Option<String>,
    pub book_published_date: Option<String>,
    pub book_isbn: Option<String>,
    pub cover_id: Option<String>,
    pub cover_href: Option<String>,

    pub manifest: HashMap<String, String>,

    pub spine: Vec<String>,
}

fn get_attr_value(e: &BytesStart<'_>, attr_name: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == attr_name || attr.key.local_name().as_ref() == attr_name {
            if let Ok(val) = attr.unescape_value() {
                return Some(val.into_owned());
            }
        }
    }
    None
}

pub fn parse_container_xml(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                if e.local_name().as_ref() == b"rootfile" {
                    if let Some(full_path) = get_attr_value(e, b"full-path") {
                        return Some(full_path);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    None
}

fn resolve_zip_path(base_dir: &str, href: &str) -> String {
    let clean_href = href.split('#').next().unwrap_or(href);
    let combined = if base_dir.is_empty() {
        clean_href.to_string()
    } else {
        format!("{}/{}", base_dir.trim_end_matches('/'), clean_href)
    };

    let mut parts = Vec::new();
    for segment in combined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

pub fn parse_opf_document(opf_xml: &str, opf_dir: &str) -> OpfPackage {
    let mut reader = Reader::from_str(opf_xml);
    reader.config_mut().trim_text(true);

    let mut pkg = OpfPackage::default();
    let mut active_tag: Option<Vec<u8>> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let local = e.local_name().as_ref().to_vec();
                match local.as_slice() {
                    b"title" | b"creator" | b"language" | b"description" | b"publisher"
                    | b"date" | b"identifier" => {
                        active_tag = Some(local);
                    }
                    b"meta" => {
                        if get_attr_value(e, b"name").as_deref() == Some("cover") {
                            if let Some(content_id) = get_attr_value(e, b"content") {
                                pkg.cover_id = Some(content_id);
                            }
                        }
                    }
                    b"item" => {
                        if let (Some(id), Some(href)) =
                            (get_attr_value(e, b"id"), get_attr_value(e, b"href"))
                        {
                            let resolved = resolve_zip_path(opf_dir, &href);
                            if let Some(props) = get_attr_value(e, b"properties") {
                                if props.split_whitespace().any(|p| p == "cover-image") {
                                    pkg.cover_href = Some(resolved.clone());
                                }
                            }
                            pkg.manifest.insert(id, resolved);
                        }
                    }
                    b"itemref" => {
                        if let Some(idref) = get_attr_value(e, b"idref") {
                            pkg.spine.push(idref);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"meta" => {
                        if get_attr_value(e, b"name").as_deref() == Some("cover") {
                            if let Some(content_id) = get_attr_value(e, b"content") {
                                pkg.cover_id = Some(content_id);
                            }
                        }
                    }
                    b"item" => {
                        if let (Some(id), Some(href)) =
                            (get_attr_value(e, b"id"), get_attr_value(e, b"href"))
                        {
                            let resolved = resolve_zip_path(opf_dir, &href);
                            if let Some(props) = get_attr_value(e, b"properties") {
                                if props.split_whitespace().any(|p| p == "cover-image") {
                                    pkg.cover_href = Some(resolved.clone());
                                }
                            }
                            pkg.manifest.insert(id, resolved);
                        }
                    }
                    b"itemref" => {
                        if let Some(idref) = get_attr_value(e, b"idref") {
                            pkg.spine.push(idref);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(_)) => {
                active_tag = None;
            }
            Ok(Event::Text(t)) => {
                if let Some(ref tag) = active_tag {
                    if let Ok(decoded) = t.unescape() {
                        let trimmed = decoded.trim();
                        if !trimmed.is_empty() {
                            match tag.as_slice() {
                                b"title" if pkg.book_title.is_none() => {
                                    pkg.book_title = Some(trimmed.to_string());
                                }
                                b"creator" if pkg.book_author.is_none() => {
                                    pkg.book_author = Some(trimmed.to_string());
                                }
                                b"language" if pkg.book_language.is_none() => {
                                    pkg.book_language = Some(trimmed.to_string());
                                }
                                b"description" if pkg.book_description.is_none() => {
                                    pkg.book_description =
                                        Some(metadata_fetcher::strip_html_tags(trimmed));
                                }
                                b"publisher" if pkg.book_publisher.is_none() => {
                                    pkg.book_publisher = Some(trimmed.to_string());
                                }
                                b"date" if pkg.book_published_date.is_none() => {
                                    pkg.book_published_date =
                                        Some(trimmed.chars().take(10).collect());
                                }
                                b"identifier" if pkg.book_isbn.is_none() => {
                                    if let Some(isbn) = metadata_fetcher::extract_clean_isbn(trimmed)
                                    {
                                        pkg.book_isbn = Some(isbn);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    if pkg.cover_href.is_none() {
        if let Some(ref cid) = pkg.cover_id {
            if let Some(resolved) = pkg.manifest.get(cid) {
                pkg.cover_href = Some(resolved.clone());
            }
        }
    }

    pkg
}

fn is_ignored_tag(local: &[u8]) -> bool {
    matches!(local, b"script" | b"style" | b"head" | b"svg" | b"nav" | b"sup")
}

fn is_ignored_noteref_anchor(e: &BytesStart<'_>) -> bool {
    if e.local_name().as_ref() != b"a" {
        return false;
    }
    for attr_name in [b"epub:type".as_slice(), b"type".as_slice(), b"role".as_slice(), b"class".as_slice()] {
        if let Some(val) = get_attr_value(e, attr_name) {
            let lower = val.to_lowercase();
            if lower.contains("noteref")
                || lower.contains("footnote-ref")
                || lower.contains("footnoteref")
                || lower.contains("bibref")
            {
                return true;
            }
        }
    }
    false
}

fn is_block_tag(local: &[u8]) -> bool {
    matches!(
        local,
        b"p"
            | b"div"
            | b"li"
            | b"h1"
            | b"h2"
            | b"h3"
            | b"h4"
            | b"h5"
            | b"h6"
            | b"blockquote"
            | b"aside"
            | b"section"
            | b"header"
    )
}

fn classify_block_kind(e: &BytesStart<'_>, inherited: ParagraphKind) -> ParagraphKind {

    let local = e.local_name();
    let local_bytes = local.as_ref();

    for attr_name in [
        b"epub:type".as_slice(),
        b"type".as_slice(),
        b"role".as_slice(),
        b"class".as_slice(),
        b"id".as_slice(),
    ] {
        if let Some(val) = get_attr_value(e, attr_name) {
            let lower = val.to_lowercase();
            if lower.contains("footnote")
                || lower.contains("endnote")
                || lower.contains("rearnote")
                || lower.starts_with("fn")
                || lower.starts_with("note")
            {
                return ParagraphKind::Footnote;
            }
            if lower.contains("subtitle")
                || lower.contains("subhead")
                || lower.contains("bridgehead")
            {
                return ParagraphKind::Subtitle;
            }
            if lower == "title"
                || lower.contains("doc-title")
                || lower.contains("chapter-title")
                || lower.contains("book-title")
            {
                return ParagraphKind::Title;
            }
            if lower.contains("epigraph")
                || lower.contains("blockquote")
                || lower.contains("citation")
                || lower.contains("quote")
            {
                return ParagraphKind::Quote;
            }
        }
    }

    match local_bytes {
        b"h1" => ParagraphKind::Title,
        b"h2" | b"h3" | b"h4" | b"h5" | b"h6" => ParagraphKind::Subtitle,
        b"blockquote" => ParagraphKind::Quote,
        b"aside" => ParagraphKind::Footnote,
        _ => inherited,
    }
}

fn flush_paragraph(
    current_paragraph: &mut String,
    kind: ParagraphKind,
    detected_title: &mut Option<String>,
    next_para_id: &mut u32,
    paragraphs: &mut Vec<ParagraphData>,
) {
    let trimmed = current_paragraph.trim();
    if !trimmed.is_empty() && trimmed.chars().any(|c| !c.is_whitespace()) {
        if matches!(kind, ParagraphKind::Title | ParagraphKind::Subtitle) && detected_title.is_none() {
            *detected_title = Some(trimmed.to_string());
        }
        paragraphs.push(ParagraphData {
            id: *next_para_id,
            text: trimmed.to_string(),
            kind,
            chapter_offset: 0,
            words: Vec::new(),
        });
        *next_para_id += 1;
    }
    current_paragraph.clear();
}

pub fn parse_xhtml_chapter(fallback_title: &str, xhtml: &str) -> ChapterData {
    let mut reader = Reader::from_str(xhtml);
    reader.config_mut().trim_text(true);

    let mut detected_title: Option<String> = None;
    let mut paragraphs = Vec::new();
    let mut current_paragraph = String::new();
    let mut ignored_depth = 0usize;
    let mut ignored_noteref_depth = 0usize;
    let mut block_kind_stack: Vec<ParagraphKind> = Vec::new();
    let mut in_head_title = false;
    let mut next_para_id = 0u32;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let local = e.local_name();
                let local_bytes = local.as_ref();
                if local_bytes == b"title" {
                    in_head_title = true;
                } else if is_ignored_tag(local_bytes) {
                    ignored_depth += 1;
                } else if ignored_depth == 0 {
                    if is_ignored_noteref_anchor(&e) || ignored_noteref_depth > 0 && local_bytes == b"a" {
                        ignored_noteref_depth += 1;
                    } else if local_bytes == b"br" {
                        let active_kind = block_kind_stack.last().copied().unwrap_or(ParagraphKind::Body);
                        flush_paragraph(
                            &mut current_paragraph,
                            active_kind,
                            &mut detected_title,
                            &mut next_para_id,
                            &mut paragraphs,
                        );
                    } else if is_block_tag(local_bytes) {
                        let inherited = block_kind_stack.last().copied().unwrap_or(ParagraphKind::Body);
                        if !current_paragraph.trim().is_empty() {
                            flush_paragraph(
                                &mut current_paragraph,
                                inherited,
                                &mut detected_title,
                                &mut next_para_id,
                                &mut paragraphs,
                            );
                        }
                        let new_kind = classify_block_kind(&e, inherited);
                        block_kind_stack.push(new_kind);
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                if ignored_depth == 0 && ignored_noteref_depth == 0 && e.local_name().as_ref() == b"br" {
                    let active_kind = block_kind_stack.last().copied().unwrap_or(ParagraphKind::Body);
                    flush_paragraph(
                        &mut current_paragraph,
                        active_kind,
                        &mut detected_title,
                        &mut next_para_id,
                        &mut paragraphs,
                    );
                }
            }
            Ok(Event::End(e)) => {
                let local = e.local_name();
                let local_bytes = local.as_ref();
                if local_bytes == b"title" {
                    in_head_title = false;
                } else if is_ignored_tag(local_bytes) {
                    ignored_depth = ignored_depth.saturating_sub(1);
                } else if ignored_depth == 0 {
                    if local_bytes == b"a" && ignored_noteref_depth > 0 {
                        ignored_noteref_depth = ignored_noteref_depth.saturating_sub(1);
                    } else if is_block_tag(local_bytes) {
                        let ended_kind = block_kind_stack.pop().unwrap_or(ParagraphKind::Body);
                        flush_paragraph(
                            &mut current_paragraph,
                            ended_kind,
                            &mut detected_title,
                            &mut next_para_id,
                            &mut paragraphs,
                        );
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if in_head_title && detected_title.is_none() {
                    if let Ok(decoded) = t.unescape() {
                        let trimmed = decoded.trim();
                        if !trimmed.is_empty() {
                            detected_title = Some(trimmed.to_string());
                        }
                    }
                } else if ignored_depth == 0 && ignored_noteref_depth == 0 && !block_kind_stack.is_empty() {
                    if let Ok(decoded) = t.unescape() {
                        let trimmed = decoded.trim();
                        if !trimmed.is_empty() {
                            if !current_paragraph.is_empty() && !current_paragraph.ends_with(' ') {
                                current_paragraph.push(' ');
                            }
                            current_paragraph.push_str(trimmed);
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    let mut chapter_offset = 0u32;
    for para in &mut paragraphs {
        para.chapter_offset = chapter_offset;
        chapter_offset += (para.text.chars().count() as u32) + 1;
    }

    ChapterData {
        index: 0,
        title: detected_title.unwrap_or_else(|| fallback_title.to_string()),
        href: String::new(),
        paragraphs,
    }
}

fn read_zip_entry_string<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry_name: &str,
) -> Option<String> {
    let mut file = archive.by_name(entry_name).ok()?;
    let mut content = String::new();
    file.read_to_string(&mut content).ok()?;
    Some(content)
}

fn read_zip_entry_bytes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry_name: &str,
) -> Option<Vec<u8>> {
    let mut file = archive.by_name(entry_name).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

pub fn parse_epub_book(
    path: impl AsRef<Path>,
    cover_output_dir: Option<&Path>,
) -> Result<EpubBookData> {
    let path_ref = path.as_ref();
    let file = File::open(path_ref)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| EdgeTtsError::InvalidConfig(format!("Invalid EPUB zip archive: {}", e)))?;

    let fallback_book_title = path_ref
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Imported EPUB")
        .to_string();

    let mut book_title: Option<String> = None;
    let mut book_author: Option<String> = None;
    let mut book_language: Option<String> = None;
    let mut book_description: Option<String> = None;
    let mut book_publisher: Option<String> = None;
    let mut book_published_date: Option<String> = None;
    let mut book_isbn: Option<String> = None;
    let mut cover_href: Option<String> = None;
    let mut cover_bytes: Option<Vec<u8>> = None;
    let mut cover_image_path: Option<String> = None;
    let mut chapters = Vec::new();

    if let Some(container_xml) = read_zip_entry_string(&mut archive, "META-INF/container.xml") {
        if let Some(opf_path) = parse_container_xml(&container_xml) {
            if let Some(opf_xml) = read_zip_entry_string(&mut archive, &opf_path) {
                let opf_dir = opf_path
                    .rsplit_once('/')
                    .map(|(dir, _)| dir)
                    .unwrap_or("");
                let pkg = parse_opf_document(&opf_xml, opf_dir);

                book_title = pkg.book_title;
                book_author = pkg.book_author;
                book_language = pkg.book_language;
                book_description = pkg.book_description;
                book_publisher = pkg.book_publisher;
                book_published_date = pkg.book_published_date;
                book_isbn = pkg.book_isbn;
                cover_href = pkg.cover_href.clone();

                for (idx, idref) in pkg.spine.iter().enumerate() {
                    if let Some(chapter_zip_path) = pkg.manifest.get(idref) {
                        if let Some(xhtml) = read_zip_entry_string(&mut archive, chapter_zip_path) {
                            let fallback = format!("Chapter {}", idx + 1);
                            let mut chapter = parse_xhtml_chapter(&fallback, &xhtml);
                            if !chapter.paragraphs.is_empty() {
                                chapter.index = chapters.len() as u32;
                                chapter.href = chapter_zip_path.clone();
                                chapters.push(chapter);
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(ref c_href) = cover_href {
        if let Some(bytes) = read_zip_entry_bytes(&mut archive, c_href) {
            if !bytes.is_empty() {
                if let Some(out_dir) = cover_output_dir {
                    let _ = std::fs::create_dir_all(out_dir);
                    let ext = Path::new(c_href)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("jpg");
                    let hash_key = crate::tts::cache::sha256_hex(
                        path_ref.to_string_lossy().as_bytes(),
                    );
                    let cover_file = out_dir.join(format!("cover_{}.{}", &hash_key[..16], ext));
                    if std::fs::write(&cover_file, &bytes).is_ok() {
                        cover_image_path = Some(cover_file.to_string_lossy().into_owned());
                    }
                }
                cover_bytes = Some(bytes);
            }
        }
    }

    if chapters.is_empty() {
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| EdgeTtsError::InvalidConfig(e.to_string()))?;
            let name = entry
                .name()
                .map_err(|e| EdgeTtsError::InvalidConfig(e.to_string()))?
                .into_owned();
            if name.ends_with(".xhtml") || name.ends_with(".html") || name.ends_with(".htm") {
                let mut content = String::new();
                if entry.read_to_string(&mut content).is_ok() {
                    let mut chapter = parse_xhtml_chapter(&name, &content);
                    if !chapter.paragraphs.is_empty() {
                        chapter.index = chapters.len() as u32;
                        chapter.href = name;
                        chapters.push(chapter);
                    }
                }
            }
        }
    }

    let resolved_title = book_title
        .or_else(|| chapters.first().map(|c| c.title.clone()))
        .unwrap_or(fallback_book_title);

    Ok(EpubBookData {
        title: resolved_title,
        author: book_author.unwrap_or_else(|| "Unknown Author".to_string()),
        language: book_language.unwrap_or_else(|| "en".to_string()),
        description: book_description,
        publisher: book_publisher,
        published_date: book_published_date,
        isbn: book_isbn,
        cover_href,
        cover_image_path,
        cover_bytes,
        chapters,
    })
}

pub fn parse_epub_file(path: impl AsRef<Path>) -> Result<Vec<ChapterData>> {
    Ok(parse_epub_book(path, None)?.chapters)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_container_and_opf() {
        let container = r##"
            <?xml version="1.0"?>
            <container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
                <rootfiles>
                    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
                </rootfiles>
            </container>
        "##;
        let opf_path = parse_container_xml(container).unwrap();
        assert_eq!(opf_path, "OEBPS/content.opf");

        let opf = r##"
            <package xmlns="http://www.idpf.org/2007/opf" version="3.0">
                <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                    <dc:title>Il Nome della Rosa</dc:title>
                    <dc:creator>Umberto Eco</dc:creator>
                    <dc:language>it</dc:language>
                    <dc:description>Un giallo storico medievale.</dc:description>
                </metadata>
                <manifest>
                    <item id="cover" href="Images/cover.jpg" media-type="image/jpeg" properties="cover-image"/>
                    <item id="ch1" href="Text/cap1.xhtml" media-type="application/xhtml+xml"/>
                    <item id="ch2" href="Text/cap2.xhtml" media-type="application/xhtml+xml"/>
                </manifest>
                <spine>
                    <itemref idref="ch1"/>
                    <itemref idref="ch2"/>
                </spine>
            </package>
        "##;
        let pkg = parse_opf_document(opf, "OEBPS");
        assert_eq!(pkg.book_title.as_deref(), Some("Il Nome della Rosa"));
        assert_eq!(pkg.book_author.as_deref(), Some("Umberto Eco"));
        assert_eq!(pkg.book_language.as_deref(), Some("it"));
        assert_eq!(
            pkg.book_description.as_deref(),
            Some("Un giallo storico medievale.")
        );
        assert_eq!(pkg.cover_href.as_deref(), Some("OEBPS/Images/cover.jpg"));
        assert_eq!(pkg.spine, vec!["ch1", "ch2"]);
        assert_eq!(
            pkg.manifest.get("ch1").map(|s| s.as_str()),
            Some("OEBPS/Text/cap1.xhtml")
        );
    }

    #[test]
    fn test_parse_xhtml_chapter_extracts_title_and_ignores_script() {
        let xhtml = r##"
            <html>
                <head><title>Capitolo Primo</title><script>alert(1);</script></head>
                <body>
                    <h1>Prologo</h1>
                    <p>Era una bella giornata. Il Dott. Bianchi leggeva un libro.</p>
                </body>
            </html>
        "##;
        let chapter = parse_xhtml_chapter("Fallback", xhtml);
        assert_eq!(chapter.title, "Capitolo Primo");
        assert_eq!(chapter.paragraphs.len(), 2);
        assert_eq!(chapter.paragraphs[0].text, "Prologo");
        assert!(chapter.paragraphs[1].text.starts_with("Era una bella giornata."));
    }

    #[test]
    fn test_parse_xhtml_filters_noteref_and_classifies_footnotes() {
        let xhtml = r##"
            <html xmlns:epub="http://www.idpf.org/2007/ops">
                <body>
                    <h2>Chapter One</h2>
                    <p>Main narrative sentence<a epub:type="noteref" href="#fn1">[1]</a> continues smoothly.</p>
                    <aside epub:type="footnote" id="fn1"><p>1. Historical footnote details.</p></aside>
                </body>
            </html>
        "##;
        let chapter = parse_xhtml_chapter("Chapter One", xhtml);
        assert_eq!(chapter.paragraphs.len(), 3);
        assert_eq!(chapter.paragraphs[0].kind, ParagraphKind::Subtitle);
        assert_eq!(chapter.paragraphs[1].kind, ParagraphKind::Body);
        assert_eq!(
            chapter.paragraphs[1].text,
            "Main narrative sentence continues smoothly."
        );
        assert_eq!(chapter.paragraphs[2].kind, ParagraphKind::Footnote);
    }

    #[test]
    fn test_parse_paradise_lost_epub_end_to_end() {
        let epub_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/paradise_lost.epub");
        let book = parse_epub_book(epub_path, None).expect("paradise_lost.epub must parse");
        assert!(book.title.to_lowercase().contains("paradise lost"));
        assert!(book.author.to_lowercase().contains("milton"));
        assert!(!book.chapters.is_empty());

        let total_paragraphs: usize = book.chapters.iter().map(|c| c.paragraphs.len()).sum();
        assert!(total_paragraphs > 50);

        for chapter in &book.chapters {
            for para in &chapter.paragraphs {
                assert!(!para.text.trim().is_empty());
                let chars: Vec<char> = para.text.chars().collect();
                let mut prev_end = 0u32;
                for word in &para.words {
                    assert!(word.char_start >= prev_end);
                    assert!(word.char_end > word.char_start);
                    let slice: String = chars[word.char_start as usize..word.char_end as usize]
                        .iter()
                        .collect();
                    assert_eq!(slice, word.text);
                    prev_end = word.char_end;
                }
            }
        }
    }
}
