//! Conversão de posição na borda entre tela do servidor e do cliente,
//! usando o layout da GUI (lado + offset). Tudo em pixels, escala 1:1.

use crate::config::Side;

#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub side: Side,
    /// Deslocamento do centro da tela remota, em fração da dimensão local ao longo da borda.
    pub offset: f32,
    pub local: (f32, f32),
    pub remote: (f32, f32),
}

impl Layout {
    fn lengths(&self) -> (f32, f32) {
        match self.side {
            Side::Left | Side::Right => (self.local.1, self.remote.1),
            Side::Top | Side::Bottom => (self.local.0, self.remote.0),
        }
    }

    /// Coordenada local ao longo da borda (px) -> fração 0..1 na borda remota.
    pub fn local_to_remote(&self, along: f32) -> f32 {
        let (l, r) = self.lengths();
        let rel = along - l / 2.0 - self.offset * l;
        ((rel + r / 2.0) / r).clamp(0.0, 1.0)
    }

    /// Fração 0..1 na borda remota -> coordenada local ao longo da borda (px).
    pub fn remote_to_local(&self, pos: f32) -> f32 {
        let (l, r) = self.lengths();
        let along = pos * r - r / 2.0 + self.offset * l + l / 2.0;
        along.clamp(0.0, l - 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_same_size() {
        let l = Layout { side: Side::Top, offset: 0.0, local: (1920.0, 1080.0), remote: (1920.0, 1080.0) };
        assert!((l.local_to_remote(960.0) - 0.5).abs() < 1e-4);
        assert!((l.remote_to_local(0.25) - 480.0).abs() < 1e-3);
    }

    #[test]
    fn offset_and_sizes() {
        // Remota 2560 de largura, deslocada meia tela local p/ direita.
        let l = Layout { side: Side::Top, offset: 0.5, local: (1920.0, 1080.0), remote: (2560.0, 1440.0) };
        // Borda direita da local (1919) = centro remoto - 1280 + 1919 - 960 - 960
        let p = l.local_to_remote(1919.0);
        let back = l.remote_to_local(p);
        assert!((back - 1919.0).abs() < 0.5);
        assert_eq!(l.local_to_remote(0.0), 0.0);
    }
}
