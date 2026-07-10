use eframe::egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use std::path::Path;

use crate::state::{FileEntry, FileEntryKind};

pub const ICON_SIZE: f32 = 16.0;

pub fn paint_entry_icon(painter: &Painter, rect: Rect, entry: &FileEntry) {
    match entry.kind {
        FileEntryKind::Directory => paint_folder(painter, rect, Color32::from_rgb(220, 170, 40)),
        FileEntryKind::Parent => paint_folder(painter, rect, Color32::from_rgb(170, 130, 30)),
        FileEntryKind::Drive => paint_drive(painter, rect),
        FileEntryKind::Archive => paint_archive(painter, rect),
        FileEntryKind::FtpItem => paint_file(painter, rect, Color32::from_rgb(80, 170, 200)),
        FileEntryKind::Unknown => paint_file(painter, rect, Color32::from_rgb(110, 110, 110)),
        FileEntryKind::File => paint_file_by_ext(painter, rect, &entry.virtual_path),
    }
}

fn paint_file_by_ext(painter: &Painter, rect: Rect, path: &str) {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "tiff" | "tif" | "ico") => {
            paint_image_file(painter, rect)
        }
        Some("mp4" | "mkv" | "avi" | "mov" | "wmv") => paint_video(painter, rect),
        Some("cbz" | "cbr" | "cb7") => paint_comic(painter, rect),
        Some("exe" | "dll" | "so" | "sys") => paint_executable(painter, rect),
        Some(
            "rs" | "py" | "js" | "ts" | "c" | "cpp" | "cxx" | "cc" | "h" | "hpp" | "sh" | "bash"
            | "ps1" | "bat" | "cmd" | "lua",
        ) => paint_script(painter, rect),
        Some("pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt") => {
            paint_document(painter, rect)
        }
        Some("lnk") => paint_shortcut(painter, rect),
        _ => paint_file(painter, rect, generic_file_color(ext.as_deref())),
    }
}

fn generic_file_color(ext: Option<&str>) -> Color32 {
    match ext {
        Some("mp3" | "flac" | "ogg" | "wav" | "aac") => Color32::from_rgb(160, 80, 200),
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
    painter.rect_filled(
        tab,
        CornerRadius {
            nw: 2,
            ne: 2,
            sw: 0,
            se: 0,
        },
        color,
    );

    let body_min = Pos2::new(min.x, min.y + tab_h);
    let body = Rect::from_min_size(body_min, Vec2::new(w, h - tab_h));
    painter.rect_filled(
        body,
        CornerRadius {
            nw: 0,
            ne: 2,
            sw: 2,
            se: 2,
        },
        color,
    );
}

fn paint_file(painter: &Painter, rect: Rect, color: Color32) {
    let w = rect.width();
    let fold = (w * 0.28).max(3.0).min(5.0);
    let min = rect.min;

    painter.rect_filled(rect, CornerRadius::ZERO, color);

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

    painter.line_segment(
        [Pos2::new(tr.x - fold, tr.y), Pos2::new(min.x, tr.y)],
        Stroke::new(0.5, Color32::from_white_alpha(40)),
    );
}

fn paint_archive(painter: &Painter, rect: Rect) {
    let color = Color32::from_rgb(190, 130, 70);
    paint_file(painter, rect, color);

    let cx = rect.min.x + rect.width() * 0.44;
    let top = rect.min.y + 1.5;
    let bot = rect.max.y - 1.5;
    let zip_color = Color32::from_white_alpha(180);

    // Pull tab at top
    let tab = Rect::from_center_size(Pos2::new(cx, top), Vec2::new(3.5, 1.5));
    painter.rect_filled(tab, CornerRadius::same(1), zip_color);

    // Spine below the tab
    let spine_top = top + 1.5;
    painter.line_segment(
        [Pos2::new(cx, spine_top), Pos2::new(cx, bot)],
        Stroke::new(0.75, zip_color),
    );

    // Teeth: alternating left/right horizontal ticks
    let tooth = 2.0;
    let mut y = spine_top + 1.5;
    let mut go_left = true;
    while y < bot - 0.5 {
        let (x0, x1) = if go_left {
            (cx - tooth, cx)
        } else {
            (cx, cx + tooth)
        };
        painter.line_segment(
            [Pos2::new(x0, y), Pos2::new(x1, y)],
            Stroke::new(1.0, zip_color),
        );
        y += 2.2;
        go_left = !go_left;
    }
}

fn paint_video(painter: &Painter, rect: Rect) {
    let color = Color32::from_rgb(80, 90, 210);
    paint_file(painter, rect, color);

    // Play triangle centered slightly left to clear the fold
    let cx = rect.min.x + rect.width() * 0.44;
    let cy = rect.center().y;
    let r = rect.height() * 0.22;
    let play_pts = vec![
        Pos2::new(cx - r, cy - r * 1.15),
        Pos2::new(cx - r, cy + r * 1.15),
        Pos2::new(cx + r * 1.3, cy),
    ];
    painter.add(Shape::convex_polygon(
        play_pts,
        Color32::from_white_alpha(210),
        Stroke::NONE,
    ));
}

fn paint_image_file(painter: &Painter, rect: Rect) {
    // Frame
    let frame_color = Color32::from_rgb(160, 100, 45);
    painter.rect_filled(rect, CornerRadius::same(1), frame_color);

    // Canvas inside the frame
    let canvas = rect.shrink(2.0);
    let sky_color = Color32::from_rgb(120, 185, 230);
    painter.rect_filled(canvas, CornerRadius::ZERO, sky_color);

    // Ground strip
    let ground_h = canvas.height() * 0.38;
    let ground = Rect::from_min_size(
        Pos2::new(canvas.min.x, canvas.max.y - ground_h),
        Vec2::new(canvas.width(), ground_h),
    );
    painter.rect_filled(ground, CornerRadius::ZERO, Color32::from_rgb(75, 155, 75));

    // Mountain peak
    let base_y = canvas.max.y - ground_h;
    let peak = Pos2::new(canvas.center().x, canvas.min.y + canvas.height() * 0.28);
    let hw = canvas.width() * 0.32;
    let mtn_pts = vec![
        peak,
        Pos2::new(peak.x - hw, base_y),
        Pos2::new(peak.x + hw, base_y),
    ];
    painter.add(Shape::convex_polygon(
        mtn_pts,
        Color32::from_rgb(135, 135, 150),
        Stroke::NONE,
    ));

    // Snow cap
    let snow_h = (peak.y - canvas.min.y) * 0.38;
    let snow_hw = hw * 0.45;
    let snow_pts = vec![
        peak,
        Pos2::new(peak.x - snow_hw, peak.y + snow_h),
        Pos2::new(peak.x + snow_hw, peak.y + snow_h),
    ];
    painter.add(Shape::convex_polygon(
        snow_pts,
        Color32::from_rgb(235, 240, 245),
        Stroke::NONE,
    ));

    // Sun
    painter.circle_filled(
        Pos2::new(canvas.max.x - 2.0, canvas.min.y + 2.0),
        1.5,
        Color32::from_rgb(255, 220, 50),
    );
}

fn paint_comic(painter: &Painter, rect: Rect) {
    // Yellow file base
    paint_file(painter, rect, Color32::from_rgb(240, 185, 25));

    let w = rect.width();
    let h = rect.height();
    let fold = (w * 0.28).max(3.0).min(5.0);

    // Bubble body: white rounded rect, inset from edges, clear of fold
    let bx1 = rect.min.x + 1.5;
    let by1 = rect.min.y + 1.5;
    let bx2 = rect.max.x - fold - 1.0;
    let by2 = rect.min.y + h * 0.62;
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(bx1, by1), Pos2::new(bx2, by2)),
        CornerRadius::same(2),
        Color32::WHITE,
    );

    // Tail: triangle pointing down-left from the bubble's bottom edge
    let tail_cx = bx1 + (bx2 - bx1) * 0.28;
    let tail_pts = vec![
        Pos2::new(tail_cx - 1.5, by2),
        Pos2::new(tail_cx + 2.5, by2),
        Pos2::new(tail_cx - 1.0, rect.max.y - 1.5),
    ];
    painter.add(Shape::convex_polygon(
        tail_pts,
        Color32::WHITE,
        Stroke::NONE,
    ));
}

fn paint_shortcut(painter: &Painter, rect: Rect) {
    paint_file(painter, rect, Color32::from_rgb(100, 175, 235));

    let cy = rect.center().y + 1.0;
    let x0 = rect.min.x + 1.5;
    let x_mid = rect.min.x + 7.5;
    let x_tip = rect.min.x + 11.5;
    let white = Color32::from_white_alpha(235);

    // Shaft
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(x0, cy - 1.2), Pos2::new(x_mid + 0.5, cy + 1.2)),
        CornerRadius::ZERO,
        white,
    );
    // Head
    painter.add(Shape::convex_polygon(
        vec![
            Pos2::new(x_mid - 0.5, cy - 3.0),
            Pos2::new(x_mid - 0.5, cy + 3.0),
            Pos2::new(x_tip, cy),
        ],
        white,
        Stroke::NONE,
    ));
}

fn paint_executable(painter: &Painter, rect: Rect) {
    let bg = Color32::from_rgb(55, 100, 215);
    paint_file(painter, rect, bg);
    paint_gear(painter, rect, Color32::from_white_alpha(230), bg);
}

fn paint_script(painter: &Painter, rect: Rect) {
    let bg = Color32::from_rgb(65, 175, 75);
    paint_file(painter, rect, bg);
    paint_gear(painter, rect, Color32::from_white_alpha(230), bg);
}

fn paint_document(painter: &Painter, rect: Rect) {
    let bg = Color32::from_rgb(245, 245, 248);
    paint_file(painter, rect, bg);

    let fold = (rect.width() * 0.28).max(3.0).min(5.0);
    let lx1 = rect.min.x + 2.0;
    let lx2_full = rect.max.x - fold - 1.5;
    let line_color = Color32::from_rgb(50, 50, 55);
    let stroke = Stroke::new(0.8, line_color);

    // 4 text lines; the last one is shorter to mimic a real paragraph
    let top = rect.min.y + 3.5;
    let gap = (rect.height() - top + rect.min.y - 1.5) / 4.0;
    for i in 0u8..4 {
        let y = top + i as f32 * gap;
        let x2 = if i == 3 {
            lx1 + (lx2_full - lx1) * 0.6
        } else {
            lx2_full
        };
        painter.line_segment([Pos2::new(lx1, y), Pos2::new(x2, y)], stroke);
    }
}

// Draws a 6-tooth gear centered in the usable area of a file icon.
// `bg` is used for the central hole so it blends into the file background.
fn paint_gear(painter: &Painter, rect: Rect, color: Color32, bg: Color32) {
    let fold = (rect.width() * 0.28).max(3.0).min(5.0);
    let cx = rect.min.x + (rect.width() - fold) * 0.5;
    let cy = rect.center().y + 1.0;

    const N: usize = 6;
    const R_BODY: f32 = 3.1;
    const TOOTH_H: f32 = 1.7;
    const TOOTH_HW: f32 = 0.95; // half-width of each tooth
    const R_HOLE: f32 = 1.2;

    painter.circle_filled(Pos2::new(cx, cy), R_BODY, color);

    for i in 0..N {
        let angle = i as f32 * std::f32::consts::TAU / N as f32;
        let (ca, sa) = (angle.cos(), angle.sin());
        let (px, py) = (-sa, ca); // perpendicular direction

        let p1 = Pos2::new(
            cx + R_BODY * ca + TOOTH_HW * px,
            cy + R_BODY * sa + TOOTH_HW * py,
        );
        let p2 = Pos2::new(
            cx + R_BODY * ca - TOOTH_HW * px,
            cy + R_BODY * sa - TOOTH_HW * py,
        );
        let p3 = Pos2::new(
            cx + (R_BODY + TOOTH_H) * ca - TOOTH_HW * px,
            cy + (R_BODY + TOOTH_H) * sa - TOOTH_HW * py,
        );
        let p4 = Pos2::new(
            cx + (R_BODY + TOOTH_H) * ca + TOOTH_HW * px,
            cy + (R_BODY + TOOTH_H) * sa + TOOTH_HW * py,
        );
        painter.add(Shape::convex_polygon(
            vec![p1, p2, p3, p4],
            color,
            Stroke::NONE,
        ));
    }

    painter.circle_filled(Pos2::new(cx, cy), R_HOLE, bg);
}

fn paint_drive(painter: &Painter, rect: Rect) {
    let color = Color32::from_rgb(150, 152, 160);
    let w = rect.width();
    let h = rect.height();
    let min = rect.min;

    let disk = Rect::from_min_size(Pos2::new(min.x, min.y + h * 0.2), Vec2::new(w, h * 0.6));
    painter.rect_filled(disk, CornerRadius::same(3), color);

    painter.circle_filled(
        Pos2::new(min.x + w - w * 0.22, min.y + h * 0.5),
        h * 0.09,
        Color32::from_rgb(90, 220, 100),
    );
}
