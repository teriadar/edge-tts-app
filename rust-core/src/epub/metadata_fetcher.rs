use std::path::Path;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::tts::cache::sha256_hex;
use crate::tts::exceptions::{EdgeTtsError, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineBookMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub published_date: Option<String>,
    pub language: Option<String>,
    pub isbn: Option<String>,
    pub categories: Vec<String>,
    pub page_count: Option<u32>,
    pub cover_url: Option<String>,
    pub cover_image_path: Option<String>,
    pub source_provider: String,
}

fn percent_encode_query(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len() * 2);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{:02X}", b)),
        }
    }
    encoded
}

fn decode_html_entity(entity: &str) -> Option<String> {
    if let Some(hex) = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"))
    {
        let code = u32::from_str_radix(hex, 16).ok()?;
        let ch = if code == 0xA0 {
            ' '
        } else {
            char::from_u32(code)?
        };
        return Some(ch.to_string());
    }
    if let Some(dec) = entity.strip_prefix('#') {
        let code = dec.parse::<u32>().ok()?;
        let ch = if code == 160 {
            ' '
        } else {
            char::from_u32(code)?
        };
        return Some(ch.to_string());
    }
    let named = match entity {
        "nbsp" | "NBSP" => " ",
        "quot" | "QUOT" => "\"",
        "apos" => "'",
        "amp" | "AMP" => "&",
        "lt" | "LT" => "<",
        "gt" | "GT" => ">",
        "ndash" => "–",
        "mdash" => "—",
        "laquo" => "«",
        "raquo" => "»",
        "ldquo" => "“",
        "rdquo" => "”",
        "lsquo" => "‘",
        "rsquo" => "’",
        "hellip" => "…",
        "egrave" => "è",
        "eacute" => "é",
        "agrave" => "à",
        "igrave" => "ì",
        "ograve" => "ò",
        "ugrave" => "ù",
        "Egrave" => "È",
        _ => return None,
    };
    Some(named.to_string())
}

fn decode_all_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp_idx) = rest.find('&') {
        out.push_str(&rest[..amp_idx]);
        let after_amp = &rest[amp_idx + 1..];
        if let Some(semi_idx) = after_amp.find(';') {
            let candidate = &after_amp[..semi_idx];
            if candidate.len() <= 10 && !candidate.contains(char::is_whitespace) {
                if let Some(decoded) = decode_html_entity(candidate) {
                    out.push_str(&decoded);
                    rest = &after_amp[semi_idx + 1..];
                    continue;
                }
            }
        }
        out.push('&');
        rest = after_amp;
    }
    out.push_str(rest);
    out.replace('\u{00A0}', " ")
}

pub fn strip_html_tags(raw: &str) -> String {

    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '<' {
            let mut tag_content = String::new();
            for tc in chars.by_ref() {
                if tc == '>' {
                    break;
                }
                tag_content.push(tc);
            }
            let tag_lower = tag_content
                .trim()
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches('/')
                .to_ascii_lowercase();
            match tag_lower.as_str() {
                "br" => out.push('\n'),
                "p" | "div" | "section" | "blockquote" | "h1" | "h2" | "h3" | "h4" => {
                    out.push_str("\n\n");
                }
                "li" => {
                    if !tag_content.trim_start().starts_with('/') {
                        out.push_str("\n• ");
                    } else {
                        out.push('\n');
                    }
                }
                _ => {}
            }
        } else {
            out.push(ch);
        }
    }

    let unescaped = decode_all_html_entities(&out);

    let mut spaced = String::with_capacity(unescaped.len() + 16);
    let mut char_iter = unescaped.chars().peekable();
    let mut prev_was_alpha = false;
    while let Some(c) = char_iter.next() {
        spaced.push(c);
        if (c == '.' || c == '!' || c == '?') && prev_was_alpha {
            if let Some(&next_c) = char_iter.peek() {
                if next_c.is_uppercase() {
                    spaced.push(' ');
                }
            }
        }
        prev_was_alpha = c.is_alphabetic() && c.is_lowercase();
    }

    let mut cleaned_lines: Vec<String> = Vec::new();
    let mut last_was_empty = true;
    for line in spaced.lines() {
        let collapsed_words = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed_words.is_empty() {
            if !last_was_empty {
                cleaned_lines.push(String::new());
                last_was_empty = true;
            }
        } else {
            cleaned_lines.push(collapsed_words);
            last_was_empty = false;
        }
    }

    cleaned_lines.join("\n").trim().to_string()
}

pub fn extract_clean_isbn(raw: &str) -> Option<String> {
    let stripped = raw
        .trim()
        .trim_start_matches("urn:isbn:")
        .trim_start_matches("isbn:")
        .trim_start_matches("ISBN:");
    let digits: String = stripped
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
        .collect();
    if digits.len() == 13 || digits.len() == 10 {
        Some(digits.to_uppercase())
    } else {
        None
    }
}

pub fn upgrade_google_cover_url(raw_url: &str) -> String {
    let https = if let Some(rest) = raw_url.strip_prefix("http://") {
        format!("https://{}", rest)
    } else {
        raw_url.to_string()
    };
    https.replace("&edge=curl", "").replace("zoom=1", "zoom=2")
}

pub fn upgrade_apple_cover_url(raw_url: &str) -> String {
    raw_url
        .replace("100x100bb", "600x600bb")
        .replace("60x60bb", "600x600bb")
}

#[derive(Debug, Deserialize)]
struct GoogleVolumesResponse {
    items: Option<Vec<GoogleVolumeItem>>,
}

#[derive(Debug, Deserialize)]
struct GoogleVolumeItem {
    #[serde(rename = "volumeInfo")]
    volume_info: Option<GoogleVolumeInfo>,
}

#[derive(Debug, Deserialize)]
struct GoogleVolumeInfo {
    title: Option<String>,
    authors: Option<Vec<String>>,
    publisher: Option<String>,
    #[serde(rename = "publishedDate")]
    published_date: Option<String>,
    description: Option<String>,
    #[serde(rename = "pageCount")]
    page_count: Option<u32>,
    categories: Option<Vec<String>>,
    language: Option<String>,
    #[serde(rename = "industryIdentifiers")]
    industry_identifiers: Option<Vec<GoogleIdentifier>>,
    #[serde(rename = "imageLinks")]
    image_links: Option<GoogleImageLinks>,
}

#[derive(Debug, Deserialize)]
struct GoogleIdentifier {
    #[serde(rename = "type")]
    id_type: Option<String>,
    identifier: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleImageLinks {
    #[serde(rename = "extraLarge")]
    extra_large: Option<String>,
    large: Option<String>,
    medium: Option<String>,
    small: Option<String>,
    thumbnail: Option<String>,
    #[serde(rename = "smallThumbnail")]
    small_thumbnail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AppleSearchResponse {
    results: Option<Vec<AppleBookResult>>,
}

#[derive(Debug, Deserialize)]
struct AppleBookResult {
    #[serde(rename = "trackName")]
    track_name: Option<String>,
    #[serde(rename = "artistName")]
    artist_name: Option<String>,
    description: Option<String>,
    #[serde(rename = "releaseDate")]
    release_date: Option<String>,
    genres: Option<Vec<String>>,
    #[serde(rename = "artworkUrl100")]
    artwork_url_100: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenLibrarySearchResponse {
    docs: Option<Vec<OpenLibraryDoc>>,
}

#[derive(Debug, Deserialize)]
struct OpenLibraryDoc {
    title: Option<String>,
    author_name: Option<Vec<String>>,
    first_publish_year: Option<i32>,
    publisher: Option<Vec<String>>,
    isbn: Option<Vec<String>>,
    subject: Option<Vec<String>>,
    language: Option<Vec<String>>,
    number_of_pages_median: Option<u32>,
    cover_i: Option<i64>,
}

async fn query_google_books(
    client: &Client,
    title: &str,
    author: Option<&str>,
    isbn: Option<&str>,
) -> Option<OnlineBookMetadata> {
    let clean_isbn = isbn.and_then(extract_clean_isbn);

    let mut query = format!("intitle:{}", title.trim());
    if let Some(a) = author
        .map(str::trim)
        .filter(|a| !a.is_empty() && !a.eq_ignore_ascii_case("Unknown Author"))
    {
        query.push_str(&format!(" inauthor:{}", a));
    }
    if query == "intitle:" {
        if let Some(ref code) = clean_isbn {
            query = format!("isbn:{}", code);
        }
    }

    let url = format!(
        "https://www.googleapis.com/books/v1/volumes?q={}&maxResults=5&printType=books",
        percent_encode_query(&query)
    );

    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let body: GoogleVolumesResponse = resp.json().await.ok()?;
    let volumes: Vec<GoogleVolumeInfo> = body
        .items?
        .into_iter()
        .filter_map(|i| i.volume_info)
        .collect();
    if volumes.is_empty() {
        return None;
    }

    let primary_idx = volumes
        .iter()
        .position(|v| v.image_links.is_some())
        .unwrap_or(0);
    let first = &volumes[primary_idx];

    let resolved_isbn = volumes
        .iter()
        .find_map(|v| {
            v.industry_identifiers.as_ref().and_then(|ids| {
                ids.iter()
                    .find(|id| id.id_type.as_deref() == Some("ISBN_13"))
                    .or_else(|| ids.first())
                    .and_then(|id| id.identifier.clone())
            })
        })
        .or(clean_isbn);

    let raw_cover = volumes.iter().find_map(|v| {
        v.image_links.as_ref().and_then(|links| {
            links
                .extra_large
                .clone()
                .or_else(|| links.large.clone())
                .or_else(|| links.medium.clone())
                .or_else(|| links.small.clone())
                .or_else(|| links.thumbnail.clone())
                .or_else(|| links.small_thumbnail.clone())
        })
    });

    let cover_url = raw_cover.map(|u| upgrade_google_cover_url(&u));
    let description = volumes.iter().find_map(|v| {
        v.description
            .as_ref()
            .map(|d| strip_html_tags(d))
            .filter(|d| !d.is_empty())
    });

    let publisher = volumes.iter().find_map(|v| v.publisher.clone());
    let published_date = volumes.iter().find_map(|v| v.published_date.clone());
    let page_count = volumes.iter().find_map(|v| v.page_count);
    let categories = volumes
        .iter()
        .find_map(|v| v.categories.clone().filter(|c| !c.is_empty()))
        .unwrap_or_default();

    Some(OnlineBookMetadata {
        title: first.title.clone(),
        author: first.authors.as_ref().and_then(|a| {
            let joined = a.join(", ");
            if joined.is_empty() {
                None
            } else {
                Some(joined)
            }
        }),
        description,
        publisher,
        published_date,
        language: first.language.clone(),
        isbn: resolved_isbn,
        categories,
        page_count,
        cover_url,
        cover_image_path: None,
        source_provider: "Google Books".to_string(),
    })
}

async fn query_apple_books(
    client: &Client,
    title: &str,
    author: Option<&str>,
) -> Option<OnlineBookMetadata> {
    let mut term = title.trim().to_string();
    if let Some(a) = author
        .map(str::trim)
        .filter(|a| !a.is_empty() && !a.eq_ignore_ascii_case("Unknown Author"))
    {
        term.push(' ');
        term.push_str(a);
    }

    let url = format!(
        "https://itunes.apple.com/search?term={}&media=ebook&limit=3",
        percent_encode_query(&term)
    );

    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let body: AppleSearchResponse = resp.json().await.ok()?;
    let first = body.results?.into_iter().next()?;

    let cover_url = first.artwork_url_100.map(|u| upgrade_apple_cover_url(&u));
    let description = first
        .description
        .map(|d| strip_html_tags(&d))
        .filter(|d| !d.is_empty());
    let published_date = first
        .release_date
        .map(|d| d.chars().take(10).collect::<String>());

    Some(OnlineBookMetadata {
        title: first.track_name,
        author: first.artist_name,
        description,
        publisher: None,
        published_date,
        language: None,
        isbn: None,
        categories: first.genres.unwrap_or_default(),
        page_count: None,
        cover_url,
        cover_image_path: None,
        source_provider: "Apple Books".to_string(),
    })
}

async fn query_open_library(
    client: &Client,
    title: &str,
    author: Option<&str>,
    isbn: Option<&str>,
) -> Option<OnlineBookMetadata> {
    let clean_isbn = isbn.and_then(extract_clean_isbn);

    let url = if let Some(ref code) = clean_isbn {
        format!(
            "https://openlibrary.org/search.json?isbn={}&limit=2",
            percent_encode_query(code)
        )
    } else {
        let mut q = format!(
            "https://openlibrary.org/search.json?title={}&limit=3",
            percent_encode_query(title.trim())
        );
        if let Some(a) = author
            .map(str::trim)
            .filter(|a| !a.is_empty() && !a.eq_ignore_ascii_case("Unknown Author"))
        {
            q.push_str(&format!("&author={}", percent_encode_query(a)));
        }
        q
    };

    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let body: OpenLibrarySearchResponse = resp.json().await.ok()?;
    let doc = body.docs?.into_iter().next()?;

    let resolved_isbn = doc
        .isbn
        .as_ref()
        .and_then(|list| list.iter().find_map(|s| extract_clean_isbn(s)))
        .or(clean_isbn);

    let cover_url = if let Some(cover_id) = doc.cover_i {
        Some(format!(
            "https://covers.openlibrary.org/b/id/{}-L.jpg",
            cover_id
        ))
    } else {
        resolved_isbn.as_ref().map(|code| {
            format!(
                "https://covers.openlibrary.org/b/isbn/{}-L.jpg?default=false",
                code
            )
        })
    };

    Some(OnlineBookMetadata {
        title: doc.title,
        author: doc.author_name.and_then(|a| {
            let joined = a.join(", ");
            if joined.is_empty() {
                None
            } else {
                Some(joined)
            }
        }),
        description: None,
        publisher: doc.publisher.and_then(|p| p.into_iter().next()),
        published_date: doc.first_publish_year.map(|y| y.to_string()),
        language: doc.language.and_then(|l| l.into_iter().next()),
        isbn: resolved_isbn,
        categories: doc
            .subject
            .map(|s| s.into_iter().take(4).collect())
            .unwrap_or_default(),
        page_count: doc.number_of_pages_median,
        cover_url,
        cover_image_path: None,
        source_provider: "Open Library".to_string(),
    })
}

async fn download_cover_to_dir(
    client: &Client,
    cover_url: &str,
    key_seed: &str,
    out_dir: &Path,
) -> Option<String> {
    let resp = client.get(cover_url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;

    if bytes.len() < 512 {
        return None;
    }
    std::fs::create_dir_all(out_dir).ok()?;
    let hash_key = sha256_hex(key_seed.as_bytes());
    let file_path = out_dir.join(format!("cover_online_{}.jpg", &hash_key[..16]));
    std::fs::write(&file_path, &bytes).ok()?;
    Some(file_path.to_string_lossy().into_owned())
}

pub async fn fetch_book_metadata_online(
    title: &str,
    author: Option<&str>,
    isbn: Option<&str>,
    cover_output_dir: Option<&Path>,
) -> Result<OnlineBookMetadata> {
    let clean_title = title.trim();
    if clean_title.is_empty() && isbn.map(str::trim).unwrap_or("").is_empty() {
        return Err(EdgeTtsError::InvalidConfig(
            "Book title or ISBN required for online metadata lookup".to_string(),
        ));
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("PaginaReader/0.1.0 (Android; +https://github.com/teriadar/pagina_reader)")
        .build()?;

    let mut primary = query_google_books(&client, clean_title, author, isbn).await;
    if primary.is_none() && isbn.is_some() && !clean_title.is_empty() {
        primary = query_google_books(&client, clean_title, author, None).await;
    }

    let need_enrichment = primary
        .as_ref()
        .map(|m| m.cover_url.is_none() || m.description.is_none())
        .unwrap_or(true);

    if need_enrichment && !clean_title.is_empty() {
        if let Some(apple) = query_apple_books(&client, clean_title, author).await {
            if let Some(ref mut base) = primary {
                if base.cover_url.is_none() && apple.cover_url.is_some() {
                    base.cover_url = apple.cover_url;
                    base.source_provider = format!("{} + Apple Books", base.source_provider);
                }
                if base.description.is_none() && apple.description.is_some() {
                    base.description = apple.description;
                }
                if base.categories.is_empty() && !apple.categories.is_empty() {
                    base.categories = apple.categories;
                }
                if base.published_date.is_none() {
                    base.published_date = apple.published_date;
                }
            } else {
                primary = Some(apple);
            }
        }
    }

    let still_needs_cover = primary
        .as_ref()
        .map(|m| m.cover_url.is_none())
        .unwrap_or(true);

    if still_needs_cover {
        if let Some(ol) = query_open_library(&client, clean_title, author, isbn).await {
            if let Some(ref mut base) = primary {
                if base.cover_url.is_none() && ol.cover_url.is_some() {
                    base.cover_url = ol.cover_url;
                    base.source_provider = format!("{} + Open Library", base.source_provider);
                }
                if base.isbn.is_none() {
                    base.isbn = ol.isbn;
                }
                if base.publisher.is_none() {
                    base.publisher = ol.publisher;
                }
                if base.page_count.is_none() {
                    base.page_count = ol.page_count;
                }
            } else {
                primary = Some(ol);
            }
        }
    }

    let mut metadata = primary.ok_or_else(|| {
        EdgeTtsError::InvalidConfig(format!(
            "No online metadata found for '{}'",
            if clean_title.is_empty() {
                isbn.unwrap_or("unknown")
            } else {
                clean_title
            }
        ))
    })?;

    if let (Some(out_dir), Some(ref cover_url)) = (cover_output_dir, metadata.cover_url.clone()) {
        let seed = format!(
            "{}:{}:{}",
            metadata.title.as_deref().unwrap_or(clean_title),
            metadata.author.as_deref().unwrap_or(""),
            cover_url
        );
        if let Some(saved_path) = download_cover_to_dir(&client, cover_url, &seed, out_dir).await {
            metadata.cover_image_path = Some(saved_path);
        }
    }

    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_and_isbn_normalization() {
        let html = "<p>Un <b>grande</b> classico.<br/>Seconda riga &amp; note.</p>";
        assert_eq!(
            strip_html_tags(html),
            "Un grande classico.\nSeconda riga & note."
        );

        assert_eq!(
            extract_clean_isbn("urn:isbn:978-88-452-9261-3"),
            Some("9788845292613".to_string())
        );
        assert_eq!(
            extract_clean_isbn("0-306-40615-2"),
            Some("0306406152".to_string())
        );
        assert_eq!(extract_clean_isbn("uuid:12345-6789"), None);
    }

    #[test]
    fn test_cover_url_upgraders() {
        let g_url = "http://books.google.com/books/content?id=123&printsec=frontcover&img=1&zoom=1&edge=curl&source=gbs_api";
        let upgraded = upgrade_google_cover_url(g_url);
        assert!(upgraded.starts_with("https://"));
        assert!(upgraded.contains("zoom=2"));
        assert!(!upgraded.contains("&edge=curl"));

        let a_url = "https://is1-ssl.mzstatic.com/image/thumb/Publication/v4/100x100bb.jpg";
        assert!(upgrade_apple_cover_url(a_url).ends_with("600x600bb.jpg"));
    }
}
