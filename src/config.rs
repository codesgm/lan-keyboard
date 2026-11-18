use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Server,
    Client,
}

/// Lado da tela local onde fica a tela remota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub role: Role,
    /// IP do servidor (usado no modo cliente).
    pub server_addr: String,
    pub port: u16,
    /// Onde a tela remota fica em relação à local.
    pub remote_side: Side,
    /// Deslocamento da tela remota ao longo da borda, em fração da tela local (-1..1).
    pub remote_offset: f32,
    /// Multiplicador de movimento do mouse aplicado no cliente.
    pub sensitivity: f32,
    /// Multiplicador de scroll aplicado no cliente.
    pub scroll_speed: f32,
    /// Resolução local (detectada ou manual).
    pub local_size: (u32, u32),
    /// Resolução remota (recebida no Hello; valor padrão até conectar).
    pub remote_size: (u32, u32),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            role: Role::Server,
            server_addr: "192.168.0.10".into(),
            port: 24800,
            remote_side: Side::Right,
            remote_offset: 0.0,
            sensitivity: 1.0,
            scroll_speed: 1.0,
            local_size: (1920, 1080),
            remote_size: (1920, 1080),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lan-keyboard")
            .join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(s) => toml::from_str(&s).unwrap_or_else(|e| {
                log::warn!("config inválida em {}: {e}; usando padrão", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}
