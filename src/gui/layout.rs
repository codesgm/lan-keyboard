//! Canvas: tela local fixa no centro, tela remota arrastável que gruda numa borda.

use crate::config::{Config, Side};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

pub fn show(ui: &mut egui::Ui, cfg: &mut Config) {
    ui.label("Arraste a tela remota para a posição real dela em relação a esta.");
    let avail = ui.available_size();
    let (resp, painter) = ui.allocate_painter(avail, Sense::drag());
    let area = resp.rect;

    let local = Vec2::new(cfg.local_size.0 as f32, cfg.local_size.1 as f32);
    let remote = Vec2::new(cfg.remote_size.0 as f32, cfg.remote_size.1 as f32);
    // Escala p/ caber local + remota em qualquer lado.
    let span = Vec2::new(local.x + 2.0 * remote.x, local.y + 2.0 * remote.y);
    let scale = (area.width() / span.x).min(area.height() / span.y) * 0.95;
    let l = local * scale;
    let r = remote * scale;
    let local_rect = Rect::from_center_size(area.center(), l);

    if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.dragged()) {
        let (side, offset) = side_from_pointer(p, local_rect);
        cfg.remote_side = side;
        cfg.remote_offset = offset;
    }
    cfg.remote_offset = clamp_offset(cfg.remote_side, cfg.remote_offset, l, r);
    let remote_rect = remote_rect(local_rect, r, cfg.remote_side, cfg.remote_offset);

    draw_screen(&painter, local_rect, "Este PC", Color32::from_rgb(60, 110, 180));
    draw_screen(&painter, remote_rect, "Remoto", Color32::from_rgb(70, 150, 90));
}

fn side_from_pointer(p: Pos2, local: Rect) -> (Side, f32) {
    let d = p - local.center();
    let nx = d.x / local.width();
    let ny = d.y / local.height();
    if nx.abs() >= ny.abs() {
        (if nx < 0.0 { Side::Left } else { Side::Right }, ny)
    } else {
        (if ny < 0.0 { Side::Top } else { Side::Bottom }, nx)
    }
}

/// Garante sobreposição de pelo menos 10% ao longo da borda.
fn clamp_offset(side: Side, offset: f32, l: Vec2, r: Vec2) -> f32 {
    let (ll, rl) = match side {
        Side::Left | Side::Right => (l.y, r.y),
        Side::Top | Side::Bottom => (l.x, r.x),
    };
    let max = ((ll + rl) / 2.0 - 0.1 * ll.min(rl)) / ll;
    offset.clamp(-max, max)
}

fn remote_rect(local: Rect, r: Vec2, side: Side, offset: f32) -> Rect {
    let c = local.center();
    let center = match side {
        Side::Left => Pos2::new(local.left() - r.x / 2.0, c.y + offset * local.height()),
        Side::Right => Pos2::new(local.right() + r.x / 2.0, c.y + offset * local.height()),
        Side::Top => Pos2::new(c.x + offset * local.width(), local.top() - r.y / 2.0),
        Side::Bottom => Pos2::new(c.x + offset * local.width(), local.bottom() + r.y / 2.0),
    };
    Rect::from_center_size(center, r)
}

fn draw_screen(painter: &egui::Painter, rect: Rect, label: &str, color: Color32) {
    painter.rect_filled(rect, 4.0, color);
    painter.rect_stroke(rect, 4.0, Stroke::new(2.0, Color32::WHITE), StrokeKind::Inside);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(16.0),
        Color32::WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_picks_side() {
        let local = Rect::from_center_size(Pos2::ZERO, Vec2::new(100.0, 50.0));
        assert_eq!(side_from_pointer(Pos2::new(90.0, 0.0), local).0, Side::Right);
        assert_eq!(side_from_pointer(Pos2::new(-90.0, 5.0), local).0, Side::Left);
        assert_eq!(side_from_pointer(Pos2::new(0.0, -60.0), local).0, Side::Top);
        assert_eq!(side_from_pointer(Pos2::new(10.0, 60.0), local).0, Side::Bottom);
    }

    #[test]
    fn offset_keeps_overlap() {
        let l = Vec2::new(100.0, 50.0);
        let off = clamp_offset(Side::Right, 10.0, l, l);
        assert!(off < 1.0 && off > 0.8);
    }
}
