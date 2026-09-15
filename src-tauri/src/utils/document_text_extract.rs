use regex::Regex;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

static XML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").unwrap());
static WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static XML_T_TEXT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<t[^>]*>(.*?)</t>").unwrap());

const TEXT_EXTS: &[&str] = &[
    "txt", "md", "csv", "log", "json", "xml", "yaml", "yml", "toml", "ini", "cfg", "conf", "py",
    "js", "ts", "jsx", "tsx", "java", "go", "rs", "c", "cpp", "h", "hpp", "cs", "php", "rb",
    "swift", "kt", "scala", "sql", "sh", "bat", "ps1", "lua", "html", "htm", "css", "scss", "less",
    "vue", "svelte", "r", "zig",
];

const MAX_CONTENT_BYTES: u64 = 2 * 1024 * 1024; // 2MB
const MAX_OFFICE_XML_BYTES: u64 = 8 * 1024 * 1024; // 单条 XML 解压上限，防 zip bomb

fn read_zip_entry_limited(
    archive: &mut zip::ZipArchive<fs::File>,
    entry_name: &str,
) -> Option<String> {
    let mut f = archive.by_name(entry_name).ok()?;
    let mut xml = String::new();
    // 限制读取量：by_name 后 f.size() 仍是声明值，按实际读取截断
    let mut buf = String::new();
    let mut total = 0u64;
    let mut chunk = [0u8; 8 * 1024];
    loop {
        let n = f.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > MAX_OFFICE_XML_BYTES {
            log::warn!("Office XML 条目过大已截断: {}", entry_name);
            break;
        }
        // 原始字节追加到临时 Vec 更安全，但这里用 lossy 读入 String 仅作搜索摘要
        buf.push_str(&String::from_utf8_lossy(&chunk[..n]));
    }
    xml.push_str(&buf);
    Some(xml)
}

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
    let Ok(meta) = fs::metadata(path) else {
        return String::new();
    };
    if meta.len() > MAX_CONTENT_BYTES {
        return String::new();
    }
    let Ok(bytes) = fs::read(path) else {
        return String::new();
    };
    decode_text_bytes(&bytes)
}

/// UTF-8 优先；失败则按 GB18030（兼容 GBK）解码，适配中文 Windows 常见编码
fn decode_text_bytes(bytes: &[u8]) -> String {
    // UTF-8 BOM
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let (cow, _, _) = encoding_rs::GB18030.decode(bytes);
    cow.into_owned()
}

fn extract_docx(path: &Path) -> String {
    extract_office_xml(path, "word/document.xml")
}

fn extract_xlsx(path: &Path) -> String {
    let Ok(file) = fs::File::open(path) else {
        return String::new();
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return String::new();
    };

    let mut shared_strings_xml = String::new();
    if let Some(s) = read_zip_entry_limited(&mut archive, "xl/sharedStrings.xml") {
        shared_strings_xml = s;
    }
    let shared: Vec<String> = XML_T_TEXT_RE
        .captures_iter(&shared_strings_xml)
        .filter_map(|cap| cap.get(1).map(|m| decode_xml_entities(m.as_str())))
        .collect();

    let mut text = String::new();
    for i in 1.. {
        let name = format!("xl/worksheets/sheet{}.xml", i);
        let Some(xml) = read_zip_entry_limited(&mut archive, &name) else {
            break;
        };
        text.push_str(&extract_xlsx_sheet_text(&xml, &shared));
        text.push(' ');
    }

    // 兜底：整表共享串仍拼上，保证全文检索可命中
    if !shared.is_empty() {
        for s in &shared {
            text.push(' ');
            text.push_str(s);
        }
    }

    collapse_ws(&text)
}

/// 从 worksheet XML 提取单元格文本：t="s" 时按索引查共享串，t="inlineStr" 取 <t>，其余取 <v>
fn extract_xlsx_sheet_text(xml: &str, shared: &[String]) -> String {
    static CELL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?s)<c\b[^>]*t="([^"]*)"[^>]*>(.*?)</c>"#).unwrap());
    static CELL_NO_T_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)<c\b[^>]*>(.*?)</c>").unwrap());
    static V_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<v>(.*?)</v>").unwrap());
    static T_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<t[^>]*>(.*?)</t>").unwrap());

    let mut out = String::new();
    for cap in CELL_RE.captures_iter(xml) {
        let cell_type = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let body = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        if cell_type == "s" {
            if let Some(v) = V_RE.captures(body).and_then(|c| c.get(1)) {
                if let Ok(idx) = v.as_str().trim().parse::<usize>() {
                    if let Some(s) = shared.get(idx) {
                        out.push_str(s);
                        out.push(' ');
                    }
                }
            }
        } else if cell_type == "inlineStr" {
            for t in T_RE.captures_iter(body) {
                if let Some(m) = t.get(1) {
                    out.push_str(&decode_xml_entities(m.as_str()));
                    out.push(' ');
                }
            }
        } else if let Some(v) = V_RE.captures(body).and_then(|c| c.get(1)) {
            let raw = decode_xml_entities(v.as_str());
            if !raw.trim().is_empty() {
                out.push_str(&raw);
                out.push(' ');
            }
        }
    }
    // 无 t 属性的单元格（数字等）
    for cap in CELL_NO_T_RE.captures_iter(xml) {
        let body = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if body.contains("t=\"") {
            continue;
        }
        if let Some(v) = V_RE.captures(body).and_then(|c| c.get(1)) {
            let raw = decode_xml_entities(v.as_str());
            if !raw.trim().is_empty() {
                out.push_str(&raw);
                out.push(' ');
            }
        }
    }
    out
}

fn decode_xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn extract_pptx(path: &Path) -> String {
    let Ok(file) = fs::File::open(path) else {
        return String::new();
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return String::new();
    };

    let mut text = String::new();
    for i in 1.. {
        let name = format!("ppt/slides/slide{}.xml", i);
        let Some(xml) = read_zip_entry_limited(&mut archive, &name) else {
            break;
        };
        text.push_str(&strip_xml(&xml));
    }

    collapse_ws(&text)
}

fn extract_office_xml(path: &Path, entry_name: &str) -> String {
    let Ok(file) = fs::File::open(path) else {
        return String::new();
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return String::new();
    };
    let Some(xml) = read_zip_entry_limited(&mut archive, entry_name) else {
        return String::new();
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_utf8() {
        let s = decode_text_bytes("你好世界".as_bytes());
        assert_eq!(s, "你好世界");
    }

    #[test]
    fn test_decode_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("hello".as_bytes());
        assert_eq!(decode_text_bytes(&bytes), "hello");
    }

    #[test]
    fn test_decode_gbk_chinese() {
        // "你好" in GBK
        let gbk: &[u8] = &[0xC4, 0xE3, 0xBA, 0xC3];
        assert_eq!(decode_text_bytes(gbk), "你好");
    }

    #[test]
    fn test_xlsx_shared_string_mapping() {
        let shared = vec!["项目".to_string(), "预算".to_string(), "OK".to_string()];
        let xml = r#"<worksheet>
            <c t="s"><v>0</v></c>
            <c t="s"><v>2</v></c>
            <c t="inlineStr"><is><t>行内</t></is></c>
            <c t="n"><v>42</v></c>
        </worksheet>"#;
        let text = extract_xlsx_sheet_text(xml, &shared);
        assert!(text.contains("项目"));
        assert!(text.contains("OK"));
        assert!(text.contains("行内"));
        assert!(text.contains("42"));
        assert!(!text.contains("预算"));
    }
}
