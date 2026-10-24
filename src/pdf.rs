//! PDF page text and page images through pdfium (feature `pdf`, FORMAT.md §10).
//! Without the feature, extraction and rendering fail with a clear message;
//! page text supplied through `literature.text_dir` works either way.

use std::path::{Path, PathBuf};

use crate::error::{Fail, Res};

#[cfg(not(feature = "pdf"))]
const NO_PDF: &str = "this plied was built without PDF support (build with --features pdf)";

#[cfg(not(feature = "pdf"))]
pub fn page_texts(_pdf: &Path) -> Res<Vec<String>> {
    Err(Fail::lit(NO_PDF))
}

#[cfg(not(feature = "pdf"))]
pub fn render_page(_pdf: &Path, _page: i64, _out: &Path) -> Res<PathBuf> {
    Err(Fail::lit(NO_PDF))
}

#[cfg(feature = "pdf")]
pub fn page_texts(pdf: &Path) -> Res<Vec<String>> {
    let pdfium = bind()?;
    let doc = pdfium.load_pdf_from_file(pdf, None).map_err(|e| Fail::lit(format!("{}: {e}", pdf.display())))?;
    let mut out = Vec::new();
    for page in doc.pages().iter() {
        let text = page.text().map_err(|e| Fail::lit(e.to_string()))?;
        out.push(text.all());
    }
    Ok(out)
}

#[cfg(feature = "pdf")]
pub fn render_page(pdf: &Path, page: i64, out: &Path) -> Res<PathBuf> {
    use pdfium_render::prelude::*;
    if page < 1 {
        return Err(Fail::lit(format!("page {page} does not exist")));
    }
    let pdfium = bind()?;
    let doc = pdfium.load_pdf_from_file(pdf, None).map_err(|e| Fail::lit(format!("{}: {e}", pdf.display())))?;
    let pages = doc.pages();
    let idx = (page - 1) as i32;
    if idx >= pages.len() as i32 {
        return Err(Fail::lit(format!("page {page} does not exist ({} pages)", pages.len())));
    }
    let pg = pages.get(idx as u16).map_err(|e| Fail::lit(e.to_string()))?;
    let img = pg.render_with_config(&PdfRenderConfig::new().set_target_width(1700)).map_err(|e| Fail::lit(e.to_string()))?.as_image();
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d)?;
    }
    // image crate with only the png feature: write PNG bytes directly
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut cur = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cur, image::ImageFormat::Png).map_err(|e| Fail::lit(e.to_string()))?;
    }
    std::fs::write(out, buf)?;
    Ok(out.to_path_buf())
}

#[cfg(feature = "pdf")]
fn bind() -> Res<pdfium_render::prelude::Pdfium> {
    use pdfium_render::prelude::*;
    Pdfium::bind_to_system_library()
        .or_else(|_| {
            // fall back to a library next to the executable (common pdfium layout)
            let exe = std::env::current_exe().unwrap_or_default();
            let dir = exe.parent().unwrap_or(Path::new(".")).to_path_buf();
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&dir))
        })
        .map(Pdfium::new)
        .map_err(|e| Fail::lit(format!("could not load the pdfium library: {e}")))
}
