//! Feature-enabled extraction smoke test. Needs pdfium on the library search path.
#![cfg(feature = "pdf")]

mod common;
use common::*;

fn one_page_pdf() -> Vec<u8> {
    let stream = "BT /F1 12 Tf 72 720 Td (A generated page cache works.) Tj ET\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut pdf = "%PDF-1.4\n".to_string();
    let mut offsets = vec![];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
    }
    let xref = pdf.len();
    pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets { pdf.push_str(&format!("{offset:010} 00000 n \n")); }
    pdf.push_str(&format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"));
    pdf.into_bytes()
}

#[test]
fn extracting_a_real_pdf_supports_pages_citations_and_search() {
    let p = Proj::new();
    let pdf = one_page_pdf();
    std::fs::create_dir(p.path().join("papers")).unwrap();
    std::fs::write(p.path().join("papers/Example.pdf"), &pdf).unwrap();
    std::fs::write(p.path().join("literature.json"), serde_json::json!({"papers": [{"id": "Example", "file": "Example.pdf", "sha256": sha(&pdf)}]}).to_string()).unwrap();
    p.append_config("[literature]\nmanifest = \"literature.json\"\npdf_dir = \"papers\"\n");
    p.ok(&["lit", "extract", "Example"]);
    assert!(p.ok(&["lit", "page", "Example", "1"]).contains("A generated page cache works."));
    p.ok(&["new", "thread", "--title", "Citing"]);
    p.edit("plied/threads/T001-citing.md", "status: open", "status: open\ncites:\n  - paper: Example\n    page: 1\n    quote: A generated page cache works.");
    p.ok(&["lit", "check"]);
    p.ok(&["index"]);
    assert!(p.ok(&["search", "generated"]).contains("Example p1"));
}
