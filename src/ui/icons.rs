use eframe::egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use std::path::Path;

use crate::state::{FileEntry, FileEntryKind};

pub const ICON_SIZE: f32 = 16.0;

pub fn paint_entry_icon(painter: &Painter, rect: Rect, entry: &FileEntry) {
    match entry.kind {
        FileEntryKind::Directory => paint_folder(painter, rect, Color32::from_rgb(220, 170, 40)),
        FileEntryKind::Parent => paint_folder(painter, rect, Color32::from_rgb(170, 130, 30)),
        FileEntryKind::Drive => paint_drive(painter, rect),
        FileEntryKind::Archive => paint_file(painter, rect, Color32::from_rgb(190, 130, 70)),
        FileEntryKind::FtpItem => paint_file(painter, rect, Color32::from_rgb(80, 170, 200)),
        FileEntryKind::Unknown => paint_file(painter, rect, Color32::from_rgb(110, 110, 110)),
        FileEntryKind::File => paint_file(painter, rect, file_color(&entry.virtual_path)),
    }
}

fn file_color(path: &str) -> Color32 {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "tiff" | "tif"
            | "ico") => Color32::from_rgb(50, 185, 175),
        Some("mp3" | "flac" | "ogg" | "wav" | "aac") => Color32::from_rgb(160, 80, 200),
        Some("mp4" | "mkv" | "avi" | "mov" | "wmv") => Color32::from_rgb(80, 90, 210),
        Some("exe" | "dll" | "so" | "sys") => Color32::from_rgb(210, 65, 55),
        Some("pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt") => {
            Color32::from_rgb(60, 120, 215)
        }
        Some(
            "rs" | "py" | "js" | "ts" | "c" | "cpp" | "cxx" | "cc" | "h" | "hpp" | "sh"
            | "bash" | "ps1" | "bat" | "cmd" | "lua",
        ) => Color32::from_rgb(75, 185, 85),
        Some("json" | "xml" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "csv") => {
            Color32::from_rgb(185, 145, 55)
        }
        Some("txt" | "md" | "log") => Color32::from_rgb(160, 160, 165),
        Some("ttf" | "otf" | "woff" | "woff2") => Color32::from_rgb(210, 75, 165),
        _ => Color32::from_rgb(130, 130, 135),
    }
}

fn paint_folder(painter: &Painter, rect: Rect, color: Color32) {
    let w = rect.width();
    let h = rect.height();
    let min = rect.min;

    let tab_w = w * 0.45;
    let tab_h = h * 0.22;
    let tab = Rect::from_min_size(min, Vec2::new(tab_w, tab_h + 1.0));
    painter.rect_filled(tab, CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, color);

    let body_min = Pos2::new(min.x, min.y + tab_h);
    let body = Rect::from_min_size(body_min, Vec2::new(w, h - tab_h));
    painter.rect_filled(body, CornerRadius { nw: 0, ne: 2, sw: 2, se: 2 }, color);
}

fn paint_file(painter: &Painter, rect: Rect, color: Color32) {
    let w = rect.width();
    let fold = (w * 0.28).max(3.0).min(5.0);
    let min = rect.min;

    // Body: full rect in the file color
    painter.rect_filled(rect, CornerRadius::ZERO, color);

    // Fold: darker triangle at top-right corner
    let darker = Color32::from_rgb(
        color.r().saturating_sub(50),
        color.g().saturating_sub(50),
        color.b().saturating_sub(50),
    );
    let tr = rect.right_top();
    let fold_pts = vec![
        Pos2::new(tr.x - fold, tr.y),
        tr,
        Pos2::new(tr.x, tr.y + fold),
    ];
    painter.add(Shape::convex_polygon(fold_pts, darker, Stroke::NONE));

    // Small white highlight line along the fold crease
    painter.line_segment(
        [Pos2::new(tr.x - fold, tr.y), Pos2::new(min.x, tr.y)],
        Stroke::new(0.5, Color32::from_white_alpha(40)),
    );
}

fn paint_drive(painter: &Painter, rect: Rect) {
    let color = Color32::from_rgb(150, 152, 160);
    let w = rect.width();
    let h = rect.height();
    let min = rect.min;

    let disk = Rect::from_min_size(
        Pos2::new(min.x, min.y + h * 0.2),
        Vec2::new(w, h * 0.6),
    );
    painter.rect_filled(disk, CornerRadius::same(3), color);

    // LED dot
    painter.circle_filled(
        Pos2::new(min.x + w - w * 0.22, min.y + h * 0.5),
        h * 0.09,
        Color32::from_rgb(90, 220, 100),
    );
}
