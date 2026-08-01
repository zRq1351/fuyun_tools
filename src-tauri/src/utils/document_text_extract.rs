use regex::Regex;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

static XML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").unwrap());
static WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static XML_T_TEXT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<t[^>]*>(.*?)</t>").unwrap());

const TEXT_EXTS: &[&str] = &[
    "txt", "md", "csv", "log", "json", "xml", "yaml", "yml", "toml", "ini", "cfg", "conf",
    "py", "js", "ts", "jsx", "tsx", "java", "go", "rs", "c", "cpp", "h", "hpp", "cs",
    "php", "rb", "swift", "kt", "scala", "sql", "sh", "bat", "ps1", "lua",
    "html", "htm", "css", "scss", "less", "vue", "svelte", "r", "zig",
];

const MAX_CONTENT_BYTES: u64 = 2 * 1024 * 1024; // 2MB

pub fn extract_file_content(path: &Path, ext: &str) -> String {
    let ext_lower = ext.to_lowercase();
    if TEXT_EXTS.contains(&ext_lower.as_str()) {
        return extract_plain_text(path);
    }
    match ext_lower.as_str() {
        "docx" => extract_docx(path),
        "xlsx" => extract_xlsx(path),
        "pptx" => extract_pptx(path),
        "pdf" => extract_pdf(path),
        _ => String::new(),
    }
}

fn extract_plain_text(path: &Path) -> String {
    let Ok(meta) = fs::metadata(path) else { return String::new() };
    if meta.len() > MAX_CONTENT_BYTES { return String::new() }
    fs::read_to_string(path).unwrap_or_default()
}

fn extract_docx(path: &Path) -> String {
    extract_office_xml(path, "word/document.xml")
}

fn extract_xlsx(path: &Path) -> String {
    let Ok(file) = fs::File::open(path) else { return String::new() };
    let Ok(mut archive) = zip::ZipArchive::new(file) else { return String::new() };

    let mut shared_strings = String::new();
    if let Ok(mut f) = archive.by_name("xl/sharedStrings.xml") {
        let _ = f.read_to_string(&mut shared_strings);
    }

    let mut text = String::new();
    for i in 1.. {
        let name = format!("xl/worksheets/sheet{}.xml", i);
        let Ok(mut f) = archive.by_name(&name) else { break };
        let mut xml = String::new();
        let _ = f.read_to_string(&mut xml);
        text.push_str(&strip_xml(&xml));
    }

    // Replace shared string references with actual text
    if !shared_strings.is_empty() {
        let strings: Vec<&str> = XML_T_TEXT_RE
            .captures_iter(&shared_strings)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str()))
            .collect();
        // Re-add shared strings at the end for searchability
        for s in strings {
            text.push(' ');
            text.push_str(s);
        }
    }

    collapse_ws(&text)
}

fn extract_pptx(path: &Path) -> String {
    let Ok(file) = fs::File::open(path) else { return String::new() };
    let Ok(mut archive) = zip::ZipArchive::new(file) else { return String::new() };

    let mut text = String::new();
    for i in 1.. {
        let name = format!("ppt/slides/slide{}.xml", i);
        let Ok(mut f) = archive.by_name(&name) else { break };
        let mut xml = String::new();
        let _ = f.read_to_string(&mut xml);
        text.push_str(&strip_xml(&xml));
    }

    collapse_ws(&text)
}

fn extract_office_xml(path: &Path, entry_name: &str) -> String {
    let Ok(file) = fs::File::open(path) else { return String::new() };
    let Ok(mut archive) = zip::ZipArchive::new(file) else { return String::new() };
    let Ok(mut f) = archive.by_name(entry_name) else { return String::new() };
    let mut xml = String::new();
    let _ = f.read_to_string(&mut xml);
    collapse_ws(&strip_xml(&xml))
}

fn extract_pdf(path: &Path) -> String {
    pdf_extract::extract_text(path).unwrap_or_default()
}

fn strip_xml(xml: &str) -> String {
    XML_TAG_RE.replace_all(xml, " ").to_string()
}

fn collapse_ws(s: &str) -> String {
    WS_RE.replace_all(s.trim(), " ").to_string()
}
