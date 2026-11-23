//! Editable Word reports built from bounded Markdown and verified sources.
use crate::workbench_plan::Source;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

pub(crate) const CREATE: &str = "create_word_report";

pub(crate) fn definition() -> serde_json::Value {
    serde_json::json!({"type":"function","function":{
        "name":CREATE,
        "description":"Prepare an editable Word report and open owner review. Use a new .docx filename and cite read_official_source results using [source_id].",
        "parameters":{"type":"object","required":["path","content"],"additionalProperties":false,"properties":{
            "path":{"type":"string","description":"New .docx filename relative to the pinned project"},
            "content":{"type":"string","description":"Report Markdown with one # title, ## sections, one plain paragraph per line and optional - bullets. Use [source_id] to cite. Omit inline formatting, links and a separate references section."}
        }}
    }})
}

#[derive(Serialize)]
pub(crate) struct Report {
    title: String,
    blocks: Vec<Block>,
    sources: Vec<Source>,
}

#[derive(Serialize)]
struct Block {
    style: &'static str,
    text: String,
}

impl Report {
    pub(crate) fn from_markdown(
        markdown: &str,
        sources: &BTreeMap<String, Source>,
    ) -> Result<Self, String> {
        if markdown.len() > 12_000
            || markdown.chars().any(|ch| {
                (ch.is_control() && !matches!(ch, '\r' | '\n' | '\t'))
                    || matches!(ch, '\u{fffe}' | '\u{ffff}')
            })
        {
            return Err("Use a plain report under 12,000 bytes.".into());
        }
        let mut lines = markdown
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty());
        let title = lines
            .next()
            .and_then(|line| line.strip_prefix("# "))
            .map(str::trim)
            .filter(|title| !title.is_empty() && title.chars().count() <= 120)
            .ok_or("Start the report with one short # title.")?;
        let mut cited = Vec::new();
        let mut used = Vec::new();
        let mut blocks = Vec::new();
        for line in lines {
            let (style, text) = if let Some(text) = line.strip_prefix("## ") {
                ("heading", text)
            } else if let Some(text) = line.strip_prefix("- ") {
                ("bullet", text)
            } else {
                ("paragraph", line)
            };
            if text.contains("://")
                || text.starts_with(['#', '|', char::from(96)])
                || text.contains("](")
            {
                return Err("Use short prose, headings, bullets, and [source_id] citations. PAIGE supplies the reference links.".into());
            }
            let mut rest = text;
            let mut formatted = String::new();
            while let Some((before, next)) = rest.split_once('[') {
                formatted.push_str(before);
                let (id, tail) = next
                    .split_once(']')
                    .ok_or("Close each [source_id] citation.")?;
                let source = sources
                    .get(id)
                    .ok_or("Use a source ID returned by read_official_source.")?;
                let index = match used.iter().position(|used| *used == id) {
                    Some(index) => index,
                    None => {
                        used.push(id);
                        cited.push(source.clone());
                        used.len() - 1
                    }
                };
                formatted.push_str(&format!("[{}]", index + 1));
                rest = tail;
            }
            formatted.push_str(rest);
            blocks.push(Block {
                style,
                text: formatted,
            });
        }
        if blocks.len() > 48
            || !blocks.iter().any(|block| block.style == "paragraph")
            || cited.is_empty()
        {
            return Err("Provide report paragraphs and cite at least one successfully read official source.".into());
        }
        Ok(Self {
            title: title.into(),
            blocks,
            sources: cited,
        })
    }

    pub(crate) fn to_docx(&self) -> Result<Vec<u8>, String> {
        let mut body = paragraph("Title", &self.title);
        for block in &self.blocks {
            body.push_str(&match block.style {
                "heading" => paragraph("Heading1", &block.text),
                "bullet" => paragraph("Bullet", &format!("•  {}", block.text)),
                _ => paragraph("Normal", &block.text),
            });
        }
        body.push_str(&paragraph("Heading1", "Sources"));
        let mut relations = String::from(STYLES_RELATIONSHIP);
        for (index, source) in self.sources.iter().enumerate() {
            let id = index + 1;
            body.push_str(&format!(
                r#"<w:p><w:pPr><w:pStyle w:val="Source"/></w:pPr><w:hyperlink r:id="source{id}"><w:r><w:rPr><w:color w:val="254E79"/><w:u w:val="single"/></w:rPr><w:t>{}</w:t></w:r></w:hyperlink></w:p>"#,
                xml(&format!("[{id}] {}", source.title)),
            ));
            relations.push_str(&format!(
                r#"<Relationship Id="source{id}" Type="{REL_TYPES}/hyperlink" Target="{}" TargetMode="External"/>"#,
                xml(&source.url),
            ));
        }
        let document = format!(
            r#"<w:document xmlns:w="{WORD}" xmlns:r="{REL_TYPES}"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1080" w:right="1260" w:bottom="1080" w:left="1260" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#,
        );
        let document_rels =
            format!(r#"<Relationships xmlns="{PACKAGE_RELS}">{relations}</Relationships>"#);
        let root_rels = format!(
            r#"<Relationships xmlns="{PACKAGE_RELS}"><Relationship Id="document" Type="{REL_TYPES}/officeDocument" Target="word/document.xml"/></Relationships>"#
        );
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, part) in [
            ("[Content_Types].xml", CONTENT_TYPES),
            ("_rels/.rels", &root_rels),
            ("word/document.xml", &document),
            ("word/styles.xml", STYLES),
            ("word/_rels/document.xml.rels", &document_rels),
        ] {
            archive
                .start_file(name, options)
                .map_err(|error| error.to_string())?;
            archive
                .write_all(part.as_bytes())
                .map_err(|error| error.to_string())?;
        }
        archive
            .finish()
            .map(|archive| archive.into_inner())
            .map_err(|error| error.to_string())
    }
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn paragraph(style: &str, text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        xml(text)
    )
}

const WORD: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const REL_TYPES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_RELS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CONTENT_TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;
const STYLES_RELATIONSHIP: &str = r#"<Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#;
const STYLES: &str = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:pPr><w:spacing w:after="150" w:line="275" w:lineRule="auto"/></w:pPr><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:color w:val="202529"/><w:sz w:val="23"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:after="300"/></w:pPr><w:rPr><w:b/><w:color w:val="000000"/><w:sz w:val="50"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="260" w:after="110"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:color w:val="000000"/><w:sz w:val="31"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Bullet"><w:name w:val="Report Bullet"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="100"/><w:ind w:left="280" w:hanging="220"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Source"><w:name w:val="Report Source"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="80"/></w:pPr><w:rPr><w:sz w:val="21"/></w:rPr></w:style>
</w:styles>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn builds_editable_word_package_and_rejects_ungrounded_citations() {
        let sources = BTreeMap::from([(
            "rust-tests".into(),
            Source {
                title: "Rust & tests".into(),
                url: "https://doc.rust-lang.org/book/".into(),
                sha256: "observed".into(),
            },
        )]);
        assert!(Report::from_markdown("# Test\nA body without citations.", &sources).is_err());
        assert!(Report::from_markdown("# Test\nA body [unread].", &sources).is_err());
        let report = Report::from_markdown(
            "# Test report\nRust uses <tests> [rust-tests].\n## Checks\n- Run tests [rust-tests].",
            &sources,
        )
        .unwrap();
        let docx = report.to_docx().unwrap();
        assert_eq!(docx, report.to_docx().unwrap());
        let mut zip = zip::ZipArchive::new(Cursor::new(docx)).unwrap();
        assert_eq!(zip.len(), 5);
        let mut text = String::new();
        zip.by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert!(text.contains(r#"<w:pStyle w:val="Title"/>"#));
        assert!(text.contains("Rust uses &lt;tests&gt; [1]."));
        assert!(text.contains("[1] Rust &amp; tests"));
    }
}
