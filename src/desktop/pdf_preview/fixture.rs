//! Tiny owned PDF fixture: three distinct pages, no external files or network.
use crate::state::{Overlay, ShellState, ViewerVm};
use super_platinum_core::{MediaAssetId, MediaAssetKind};
pub fn document() -> Vec<u8> {
    document_pages(3)
}

fn document_pages(page_count: usize) -> Vec<u8> {
    let pages = [
        ("First page - Super Platinum", "0.12 0.36 0.75", "420 540"),
        ("Second page - Landscape", "0.15 0.55 0.3", "540 420"),
        (
            "Third page - Navigation verified",
            "0.65 0.2 0.2",
            "420 540",
        ),
    ];
    let kids = (0..page_count)
        .map(|i| format!("{} 0 R", 4 + i * 2))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects=vec!["<< /Type /Catalog /Pages 2 0 R /OpenAction << /S /JavaScript /JS (this.previewExecuted = true) >> >>".to_string(),format!("<< /Type /Pages /Kids [{kids}] /Count {page_count} >>"),"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into()];
    for index in 0..page_count {
        let (title, color, size) = pages[index % pages.len()];
        let height: i32 = size.split_whitespace().nth(1).unwrap().parse().unwrap();
        let title_y = height - 50;
        let block_y = height - 180;
        let content = format!(
            "q {color} rg 30 {block_y} 320 60 re f Q\nBT /F1 18 Tf 30 {title_y} Td ({title}) Tj 0 -32 Td /F1 11 Tf (Selectable text and local PDF rendering.) Tj 0 -20 Td (Build [ready] a+b. Build [ready] a+b.) Tj ET\n"
        );
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {size}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",5+index*2));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ));
    }
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
            offsets.len()
        )
        .as_bytes(),
    );
    bytes
}
pub fn apply(shell: &mut ShellState, name: &str) {
    let id = MediaAssetId::new(MediaAssetKind::Attachment);
    shell.media.insert(
        id.clone(),
        "application/pdf",
        if name == "pdf-preview-error" {
            b"not a PDF".to_vec()
        } else if name == "pdf-preview-search-long" {
            document_pages(120)
        } else {
            document()
        },
    );
    shell.viewer = Some(ViewerVm {
        file: None,
        id,
        name: "Project brief.pdf".into(),
        download_name: "Project brief.pdf".into(),
        mime: "application/pdf".into(),
    });
    shell.overlay = Some(Overlay::Viewer);
}
