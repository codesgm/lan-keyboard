mod layout;

use crate::config::{Config, Role};
use crate::worker::{Shared, Worker};
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
    worker: Option<Worker>,
    shared: Shared,
    status: String,
}

impl App {
    pub fn new(cfg: Config, ctx: egui::Context) -> Self {
        Self { cfg, tab: Tab::Connection, worker: None, shared: Shared::new(Some(ctx)), status: "Parado".into() }
    }

    fn running(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| !w.is_finished())
    }

    fn toggle(&mut self) {
        if self.worker.take().is_some_and(|w| !w.is_finished()) {
            return; // drop(Worker) para e espera a thread
        }
        self.save();
        self.shared.update(|s| s.status = "Iniciando…".into());
        self.worker = Some(Worker::start(self.cfg.clone(), self.shared.clone()));
    }

    fn save(&mut self) {
        let msg = match self.cfg.save() {
            Ok(()) => format!("Config salva em {}", Config::path().display()),
            Err(e) => format!("Erro ao salvar: {e}"),
        };
        self.shared.update(|s| s.status = msg);
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
            ui.label("PIN:");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.cfg.pin).desired_width(100.0));
                if ui.button("Gerar novo").clicked() {
                    self.cfg.pin = crate::secure::random_pin();
                }
            });
            ui.end_row();
        });
        ui.label("Use o mesmo PIN nos dois PCs.");
        ui.add_space(12.0);
        let label = if self.running() { "Parar" } else { "Iniciar" };
        if ui.button(label).clicked() {
            self.toggle();
        }
        if self.running() {
            ui.label("Mudanças de config valem ao reiniciar.");
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
        let (status, local, remote) = self.shared.read(|s| (s.status.clone(), s.local_size, s.remote_size));
        if !status.is_empty() {
            self.status = status;
        }
        if let Some(l) = local {
            self.cfg.local_size = l;
        }
        if let Some(r) = remote {
            self.cfg.remote_size = r;
        }
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
            Tab::Layout if self.cfg.role == Role::Client => {
                ui.label("Layout é configurado no servidor (PC com teclado/mouse).");
            }
            Tab::Layout => layout::show(ui, &mut self.cfg),
            Tab::Settings => self.settings_tab(ui),
        });
    }
}
