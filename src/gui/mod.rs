mod layout;

use crate::config::{Config, Role};
use eframe::egui;

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Connection,
    Layout,
    Settings,
}

pub struct App {
    cfg: Config,
    tab: Tab,
    running: bool,
    status: String,
}

impl App {
    pub fn new(cfg: Config) -> Self {
        Self { cfg, tab: Tab::Connection, running: false, status: "Parado".into() }
    }

    fn save(&mut self) {
        self.status = match self.cfg.save() {
            Ok(()) => format!("Config salva em {}", Config::path().display()),
            Err(e) => format!("Erro ao salvar: {e}"),
        };
    }

    fn connection_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Modo:");
            ui.selectable_value(&mut self.cfg.role, Role::Server, "Servidor (tem teclado/mouse)");
            ui.selectable_value(&mut self.cfg.role, Role::Client, "Cliente");
        });
        ui.add_space(8.0);
        egui::Grid::new("conn").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            if self.cfg.role == Role::Client {
                ui.label("IP do servidor:");
                ui.text_edit_singleline(&mut self.cfg.server_addr);
                ui.end_row();
            }
            ui.label("Porta:");
            ui.add(egui::DragValue::new(&mut self.cfg.port).range(1024..=65535));
            ui.end_row();
        });
        ui.add_space(12.0);
        let label = if self.running { "Parar" } else { "Iniciar" };
        if ui.button(label).clicked() {
            // Rede/input entram nas fases 2–4.
            self.running = !self.running;
            self.status = if self.running { "Rodando (sem rede ainda)".into() } else { "Parado".into() };
        }
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
            ui.label("Sensibilidade do mouse (cliente):");
            ui.add(egui::Slider::new(&mut self.cfg.sensitivity, 0.25..=3.0).suffix("x"));
            ui.end_row();
            ui.label("Velocidade do scroll (cliente):");
            ui.add(egui::Slider::new(&mut self.cfg.scroll_speed, 0.25..=3.0).suffix("x"));
            ui.end_row();
            ui.label("Resolução local:");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.cfg.local_size.0).range(320..=16384));
                ui.label("x");
                ui.add(egui::DragValue::new(&mut self.cfg.local_size.1).range(240..=16384));
            });
            ui.end_row();
        });
        ui.add_space(8.0);
        ui.label("Atalho de emergência: Ctrl+Alt+Pause (devolve controle ao PC local)");
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Connection, "Conexão");
                ui.selectable_value(&mut self.tab, Tab::Layout, "Layout");
                ui.selectable_value(&mut self.tab, Tab::Settings, "Ajustes");
            });
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Salvar").clicked() {
                    self.save();
                }
                ui.label(&self.status);
            });
        });
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Connection => self.connection_tab(ui),
            Tab::Layout => layout::show(ui, &mut self.cfg),
            Tab::Settings => self.settings_tab(ui),
        });
    }
}
