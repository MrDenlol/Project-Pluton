use circuit_lab::{
    arithmetic::mask,
    demos,
    model::{Circuit, Kind, Node, Port},
    simulator::{Signal, Simulator},
    storage,
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};

const MINT: Color32 = Color32::from_rgb(85, 222, 178);
const INK: Color32 = Color32::from_rgb(15, 23, 36);
const MUTED: Color32 = Color32::from_rgb(146, 165, 188);
const GOLD: Color32 = Color32::from_rgb(244, 194, 104);
const ERROR: Color32 = Color32::from_rgb(255, 113, 128);
const NODE_WIDTH: f32 = 208.;

enum Pending {
    Replace(Circuit, Option<PathBuf>),
    Close,
}

pub struct CircuitApp {
    circuit: Circuit,
    sim: Simulator,
    path: Option<PathBuf>,
    selected: Option<u64>,
    connecting: Option<Port>,
    dragging: Option<u64>,
    zoom: f32,
    pan: Vec2,
    fit: bool,
    running: bool,
    last_tick: Instant,
    hz: f32,
    width: u8,
    value_edit: String,
    message: String,
    dirty: bool,
    undo: Vec<Circuit>,
    redo: Vec<Circuit>,
    traces: VecDeque<(u64, Vec<(u64, Signal)>)>,
    pending: Option<Pending>,
    allow_close: bool,
    help: bool,
}

impl CircuitApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::from_rgb(22, 32, 48);
        style.visuals.window_fill = Color32::from_rgb(26, 38, 56);
        style.visuals.selection.bg_fill = Color32::from_rgb(30, 95, 86);
        style.visuals.selection.stroke.color = MINT;
        style.spacing.item_spacing = Vec2::new(10., 8.);
        style.spacing.button_padding = Vec2::new(12., 7.);
        cc.egui_ctx.set_style(style);
        let mut circuit = demos::alu64();
        let mut message =
            "Готово. Выберите вход A, B или код операции и измените значение справа.".into();
        let mut path = None;
        if let Some(arg) = std::env::args_os().nth(1) {
            let candidate = PathBuf::from(arg);
            match storage::load(&candidate) {
                Ok(c) => {
                    circuit = c;
                    path = Some(candidate);
                }
                Err(e) => message = e,
            }
        }
        let mut sim = Simulator::default();
        if let Err(e) = sim.reset(&mut circuit) {
            message = e;
        }
        Self {
            circuit,
            sim,
            path,
            selected: None,
            connecting: None,
            dragging: None,
            zoom: 1.,
            pan: Vec2::ZERO,
            fit: true,
            running: false,
            last_tick: Instant::now(),
            hz: 2.,
            width: 1,
            value_edit: String::new(),
            message,
            dirty: false,
            undo: vec![],
            redo: vec![],
            traces: VecDeque::new(),
            pending: None,
            allow_close: false,
            help: false,
        }
    }
    fn checkpoint(&mut self) {
        self.undo.push(self.circuit.clone());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }
    fn refresh(&mut self) {
        if let Err(e) = self.sim.propagate(&self.circuit) {
            self.message = e;
            self.running = false;
        }
    }
    fn record(&mut self) {
        let readings = self
            .circuit
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Output || n.kind.sequential())
            .take(8)
            .map(|n| (n.id, self.sim.reading(&self.circuit, n)))
            .collect();
        self.traces.push_back((self.sim.ticks, readings));
        if self.traces.len() > 64 {
            self.traces.pop_front();
        }
    }
    fn tick(&mut self) {
        match self.sim.step(&mut self.circuit) {
            Ok(()) => self.record(),
            Err(e) => {
                self.message = e;
                self.running = false;
            }
        }
    }
    fn reset(&mut self) {
        self.running = false;
        if let Err(e) = self.sim.reset(&mut self.circuit) {
            self.message = e;
        }
        self.traces.clear();
        self.record();
    }
    fn select(&mut self, id: u64) {
        self.selected = Some(id);
        self.value_edit = self
            .circuit
            .node(id)
            .map(|n| n.value.to_string())
            .unwrap_or_default();
    }
    fn replace(&mut self, mut circuit: Circuit, path: Option<PathBuf>) {
        self.running = false;
        if let Err(e) = self.sim.reset(&mut circuit) {
            self.message = e;
            return;
        }
        self.circuit = circuit;
        self.path = path;
        self.selected = None;
        self.connecting = None;
        self.undo.clear();
        self.redo.clear();
        self.traces.clear();
        self.fit = true;
        self.dirty = false;
        self.message = "Схема открыта. Память триггеров и тактовый сигнал сброшены в 0.".into();
    }
    fn request_replace(&mut self, circuit: Circuit, path: Option<PathBuf>) {
        if self.dirty {
            self.running = false;
            self.pending = Some(Pending::Replace(circuit, path));
        } else {
            self.replace(circuit, path);
        }
    }
    fn open(&mut self) {
        self.running = false;
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Схема Circuit Lab", &["json"])
            .pick_file()
        {
            match storage::load(&path) {
                Ok(c) => self.request_replace(c, Some(path)),
                Err(e) => self.message = e,
            }
        }
    }
    fn save(&mut self, save_as: bool) -> bool {
        self.running = false;
        let path = if save_as || self.path.is_none() {
            rfd::FileDialog::new()
                .add_filter("Схема Circuit Lab", &["json"])
                .set_file_name("circuit.json")
                .save_file()
        } else {
            self.path.clone()
        };
        let Some(path) = path else {
            return false;
        };
        let mut document = self.circuit.clone();
        for n in &mut document.nodes {
            if n.kind == Kind::Clock {
                n.value = 0;
            }
        }
        match storage::save(&document, &path) {
            Ok(()) => {
                self.message = format!("Сохранено: {}", path.display());
                self.path = Some(path);
                self.dirty = false;
                true
            }
            Err(e) => {
                self.message = e;
                false
            }
        }
    }
    fn undo(&mut self, redo: bool) {
        let target = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some(c) = target {
            if redo {
                self.undo.push(self.circuit.clone());
            } else {
                self.redo.push(self.circuit.clone());
            }
            self.circuit = c;
            self.selected = None;
            self.connecting = None;
            self.dirty = true;
            self.reset();
        }
    }
    fn delete_selected(&mut self) {
        if let Some(id) = self.selected.take() {
            self.checkpoint();
            self.circuit.remove(id);
            self.connecting = None;
            self.refresh();
        }
    }
    fn top(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(5.);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("CIRCUIT")
                        .strong()
                        .size(22.)
                        .color(MINT),
                );
                ui.label(egui::RichText::new("LAB").size(22.));
                ui.separator();
                if ui.button("Новая").clicked() {
                    self.request_replace(Circuit::empty(), None);
                }
                if ui.button("Открыть").clicked() {
                    self.open();
                }
                if ui.button("Сохранить").clicked() {
                    self.save(false);
                }
                ui.menu_button("Ещё", |ui| {
                    if ui.button("Сохранить как…").clicked() {
                        self.save(true);
                        ui.close();
                    }
                    if ui
                        .add_enabled(!self.undo.is_empty(), egui::Button::new("Отменить  Ctrl+Z"))
                        .clicked()
                    {
                        self.undo(false);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            !self.redo.is_empty(),
                            egui::Button::new("Повторить  Ctrl+Y"),
                        )
                        .clicked()
                    {
                        self.undo(true);
                        ui.close();
                    }
                    if ui.button("Инструкция").clicked() {
                        self.help = true;
                        ui.close();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Сброс").clicked() {
                        self.reset();
                    }
                    if ui
                        .button("Шаг →")
                        .on_hover_text("Переключить CLK: один шаг = половина периода")
                        .clicked()
                    {
                        self.running = false;
                        self.tick();
                    }
                    if ui
                        .add(
                            egui::Button::new(if self.running {
                                "Ⅱ  Пауза"
                            } else {
                                "▶  Запуск"
                            })
                            .fill(Color32::from_rgb(30, 90, 77)),
                        )
                        .clicked()
                    {
                        self.running = !self.running;
                        self.last_tick = Instant::now();
                    }
                    ui.add(
                        egui::Slider::new(&mut self.hz, 0.5..=20.)
                            .suffix(" шаг/с")
                            .show_value(true),
                    );
                });
            });
            ui.add_space(5.);
        });
    }
    fn palette(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("palette")
            .exact_width(216.)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(12.);
                ui.label(egui::RichText::new("БИБЛИОТЕКА").color(MUTED).strong());
                ui.label("Добавьте блок на рабочее поле");
                ui.horizontal(|ui| {
                    ui.label("Биты");
                    egui::ComboBox::from_id_salt("width")
                        .selected_text(self.width.to_string())
                        .show_ui(ui, |ui| {
                            for w in [1, 2, 4, 8, 16, 32, 64] {
                                ui.selectable_value(&mut self.width, w, w.to_string());
                            }
                        });
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (name, kinds) in [
                        ("Входы и наблюдение", &Kind::ALL[0..3]),
                        ("Логические вентили", &Kind::ALL[3..9]),
                        ("Арифметика и выбор", &Kind::ALL[9..16]),
                        ("Память · передний фронт", &Kind::ALL[16..20]),
                    ] {
                        egui::CollapsingHeader::new(name)
                            .default_open(true)
                            .show(ui, |ui| {
                                for &kind in kinds {
                                    if ui
                                        .add_sized([174., 28.], egui::Button::new(kind.title()))
                                        .clicked()
                                    {
                                        self.checkpoint();
                                        let offset = (self.circuit.nodes.len() % 6) as f32 * 35.;
                                        let id = self.circuit.add(
                                            kind,
                                            self.width,
                                            [
                                                (50. - self.pan.x) / self.zoom + offset,
                                                (50. - self.pan.y) / self.zoom + offset,
                                            ],
                                        );
                                        self.select(id);
                                        self.refresh();
                                    }
                                }
                            });
                    }
                    ui.add_space(10.);
                    ui.separator();
                    ui.label(egui::RichText::new("УЧЕБНЫЕ СХЕМЫ").color(MUTED).strong());
                    for (label, make) in [
                        ("01  АЛУ · 64 бита", demos::alu64 as fn() -> Circuit),
                        ("02  Сумматор из вентилей", demos::full_adder),
                        ("03  Триггеры D / JK / T / SR", demos::flip_flops),
                        ("04  Счётчик с регистром", demos::counter),
                    ] {
                        if ui
                            .add_sized([190., 30.], egui::Button::new(label))
                            .clicked()
                        {
                            self.request_replace(make(), None);
                        }
                    }
                });
            });
    }
    fn inspector(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("inspector").exact_width(270.).resizable(false).show(ctx, |ui| {
            ui.add_space(12.);
            ui.label(egui::RichText::new("ИНСПЕКТОР").color(MUTED).strong());
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Some(node) = self.selected.and_then(|id| self.circuit.node(id)).cloned() {
                    ui.heading(node.kind.title());
                    ui.label(format!("{} бит  /  блок #{}", node.width, node.id));
                    let mut label = node.label.clone();
                    if ui.text_edit_singleline(&mut label).changed() {
                        self.checkpoint();
                        if let Some(n) = self.circuit.nodes.iter_mut().find(|n| n.id == node.id) { n.label = label; }
                    }
                    ui.separator();
                    if node.kind == Kind::Input {
                        ui.label("Значение входа");
                        let edit = ui.text_edit_singleline(&mut self.value_edit);
                        ui.small("Десятичное, 0xHEX или 0bBIN");
                        if ui.button("Применить").clicked() || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                            let text = self.value_edit.trim().replace('_', "");
                            let parsed = if let Some(v) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) { u64::from_str_radix(v, 16) }
                                else if let Some(v) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) { u64::from_str_radix(v, 2) }
                                else { text.parse::<u64>() };
                            match parsed {
                                Ok(value) if value <= mask(node.width) => {
                                    self.checkpoint(); self.circuit.nodes.iter_mut().find(|n| n.id == node.id).unwrap().value = value;
                                    self.message = format!("{} = {value}", node.label); self.refresh(); self.record();
                                },
                                _ => self.message = format!("Введите целое число от 0 до {}", mask(node.width)),
                            }
                        }
                        if node.width == 1 && ui.button("Переключить 0 / 1").clicked() {
                            self.checkpoint(); self.circuit.nodes.iter_mut().find(|n| n.id == node.id).unwrap().value ^= 1;
                            self.select(node.id); self.refresh(); self.record();
                        }
                    }
                    if node.kind == Kind::Clock { ui.label("CLK переключается кнопкой «Шаг» и во время запуска."); }
                    if node.kind.sequential() { ui.label("Захват по фронту 0 → 1. Начальное Q = 0."); }
                    if node.kind == Kind::Sr { ui.colored_label(GOLD, "S = R = 1 → ошибка SR!"); }
                    let reading = self.sim.reading(&self.circuit, &node);
                    ui.add_space(8.);
                    ui.label(egui::RichText::new(reading.text()).size(25.).color(signal_color(reading)));
                    if let Signal::Value(value) = reading {
                        ui.monospace(format!("HEX  {value:016X}"));
                        ui.label("BIN"); ui.add(egui::Label::new(egui::RichText::new(format!("{value:0width$b}", width = node.width as usize)).monospace()).wrap());
                        ui.label(format!("Z = {}", u8::from(value == 0)));
                    }
                    ui.separator(); ui.label("Входные порты");
                    for (pin, spec) in node.inputs().iter().enumerate() {
                        let port = Port { node: node.id, pin };
                        let value = self.sim.input(&self.circuit, port);
                        ui.horizontal(|ui| {
                            ui.label(format!("{} [{}] = {}", spec.name, spec.width, value.text()));
                            if self.circuit.wires.iter().any(|w| w.to == port) && ui.small_button("×").on_hover_text("Удалить соединение").clicked() {
                                self.checkpoint(); self.circuit.wires.retain(|w| w.to != port); self.refresh();
                            }
                        });
                    }
                    ui.add_space(8.); ui.label("Выходные порты");
                    for (pin, spec) in node.outputs().iter().enumerate() {
                        let value = self.sim.output(Port { node: node.id, pin });
                        ui.colored_label(signal_color(value), format!("{} [{}] = {}", spec.name, spec.width, value.text()));
                    }
                    ui.add_space(20.);
                    if ui.button("Удалить блок").clicked() { self.delete_selected(); }
                } else {
                    ui.heading("Исследуйте схему");
                    ui.label("Выберите блок, чтобы увидеть его входы, выходы и значение сигнала.");
                    ui.add_space(15.);
                    ui.colored_label(MINT, "Соединение");
                    ui.label("Нажмите выходной кружок справа, затем входной кружок слева.");
                    ui.colored_label(MINT, "Навигация");
                    ui.label("Перетаскивайте блоки левой кнопкой. Средняя кнопка перемещает поле. Колесо меняет масштаб.");
                    ui.colored_label(GOLD, "АЛУ · коды операции");
                    ui.label("0  →  A + B\n1  →  A − B\n2  →  A × B\n3  →  A ÷ B");
                    ui.label("Беззнаковые целые 64 бит. Переполнение по модулю 2⁶⁴. Деление на ноль даёт DIV/0.");
                    ui.separator();
                    ui.label("X — вход не подключён или значение неизвестно.");
                }
            });
        });
    }
    fn trace_panel(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("trace").default_height(180.).resizable(true).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("СИГНАЛЫ ВО ВРЕМЕНИ").strong().color(MUTED));
                ui.label(format!("Шаг {}  ·  последние 64 отсчёта  ·  до 8 сигналов", self.sim.ticks));
            });
            if self.traces.is_empty() { ui.label("Нажмите «Шаг» или «Запуск», чтобы записать сигналы индикаторов и триггеров."); return; }
            egui::ScrollArea::vertical().show(ui, |ui| {
                let rows: Vec<_> = self.circuit.nodes.iter().filter(|n| n.kind == Kind::Output || n.kind.sequential()).take(8).collect();
                for node in rows {
                    ui.horizontal(|ui| {
                        ui.add_sized([140., 28.], egui::Label::new(&node.label));
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.), Sense::hover());
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 3., INK);
                        let step = rect.width() / self.traces.len().max(16) as f32;
                        let mut last_y = None;
                        for (index, (_, values)) in self.traces.iter().enumerate() {
                            let v = values.iter().find(|(id, _)| *id == node.id).map(|(_, v)| *v).unwrap_or(Signal::Unknown);
                            let x = rect.left() + index as f32 * step;
                            if node.width == 1 {
                                let y = rect.top() + if v == Signal::Value(0) { 23. } else if v == Signal::Value(1) { 5. } else { 14. };
                                if let Some(old) = last_y { painter.line_segment([Pos2::new(x,old),Pos2::new(x,y)], Stroke::new(1.5_f32,signal_color(v))); }
                                painter.line_segment([Pos2::new(x,y),Pos2::new(x+step,y)], Stroke::new(1.5_f32,signal_color(v))); last_y = Some(y);
                            } else {
                                painter.rect_stroke(Rect::from_min_size(Pos2::new(x,rect.top()+3.),Vec2::new(step,22.)), 2., Stroke::new(1_f32,signal_color(v)), egui::StrokeKind::Inside);
                                if step > 24. { painter.text(Pos2::new(x+step/2.,rect.center().y),Align2::CENTER_CENTER,v.text(),FontId::monospace(10.),signal_color(v)); }
                            }
                            let hover = ui.interact(Rect::from_min_size(Pos2::new(x,rect.top()),Vec2::new(step,28.)),ui.id().with((node.id,index)),Sense::hover());
                            hover.on_hover_text(format!("Шаг {}: {}", self.traces[index].0, v.text()));
                        }
                    });
                }
            });
        });
    }
    fn canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(INK))
            .show(ctx, |ui| {
                ui.add_space(10.);
                ui.horizontal(|ui| {
                    ui.add_space(15.);
                    ui.heading(format!(
                        "{}{}",
                        self.circuit.name,
                        if self.dirty { " •" } else { "" }
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Вписать").clicked() {
                            self.fit = true;
                        }
                        ui.label(format!("{:.0}%", self.zoom * 100.));
                    });
                });
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                if self.fit {
                    if !self.circuit.nodes.is_empty() {
                        let min_x = self
                            .circuit
                            .nodes
                            .iter()
                            .map(|n| n.position[0])
                            .fold(f32::INFINITY, f32::min);
                        let min_y = self
                            .circuit
                            .nodes
                            .iter()
                            .map(|n| n.position[1])
                            .fold(f32::INFINITY, f32::min);
                        let max_x = self
                            .circuit
                            .nodes
                            .iter()
                            .map(|n| n.position[0] + NODE_WIDTH)
                            .fold(f32::NEG_INFINITY, f32::max);
                        let max_y = self
                            .circuit
                            .nodes
                            .iter()
                            .map(|n| n.position[1] + node_height(n))
                            .fold(f32::NEG_INFINITY, f32::max);
                        self.zoom = ((rect.width() - 60.) / (max_x - min_x))
                            .min((rect.height() - 50.) / (max_y - min_y))
                            .clamp(0.25, 1.1);
                        self.pan = Vec2::new(
                            (rect.width() - (max_x - min_x) * self.zoom) / 2. - min_x * self.zoom,
                            25. - min_y * self.zoom,
                        );
                    } else {
                        self.pan = Vec2::ZERO;
                        self.zoom = 1.;
                    }
                    self.fit = false;
                }
                if response.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.
                        && let Some(pointer) = ui.input(|i| i.pointer.hover_pos())
                    {
                        let old = self.zoom;
                        self.zoom = (self.zoom * (scroll * 0.002).exp()).clamp(0.2, 2.);
                        let relative = pointer - rect.min;
                        self.pan = relative - (relative - self.pan) * (self.zoom / old);
                    }
                }
                if response.dragged_by(egui::PointerButton::Middle) {
                    self.pan += ui.input(|i| i.pointer.delta());
                }
                let origin = rect.min + self.pan;
                if response.drag_started_by(egui::PointerButton::Primary)
                    && let Some(pos) = ui.input(|i| i.pointer.press_origin())
                    && self.hit_port(origin, pos).is_none()
                {
                    self.dragging = self.hit_node(origin, pos);
                    if let Some(id) = self.dragging {
                        self.checkpoint();
                        self.select(id);
                    }
                }
                if response.dragged_by(egui::PointerButton::Primary)
                    && let Some(id) = self.dragging
                {
                    let delta = ui.input(|i| i.pointer.delta()) / self.zoom;
                    if let Some(n) = self.circuit.nodes.iter_mut().find(|n| n.id == id) {
                        n.position[0] += delta.x;
                        n.position[1] += delta.y;
                    }
                }
                if response.drag_stopped() {
                    self.dragging = None;
                }
                if response.clicked()
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    if let Some((port, output)) = self.hit_port(origin, pos) {
                        if output {
                            self.connecting = Some(port);
                            self.message = "Теперь нажмите входной порт. Esc — отмена.".into();
                        } else if let Some(from) = self.connecting {
                            let previous = self.circuit.clone();
                            match self.circuit.connect(from, port) {
                                Ok(()) => {
                                    self.undo.push(previous);
                                    self.redo.clear();
                                    self.dirty = true;
                                    self.connecting = None;
                                    self.message = "Соединение создано.".into();
                                    self.refresh();
                                }
                                Err(e) => self.message = e,
                            }
                        } else {
                            self.select(port.node);
                        }
                    } else if let Some(id) = self.hit_node(origin, pos) {
                        self.select(id);
                    } else {
                        self.selected = None;
                        self.connecting = None;
                    }
                }
                if response.secondary_clicked()
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    if let Some((port, false)) = self.hit_port(origin, pos) {
                        self.checkpoint();
                        self.circuit.wires.retain(|w| w.to != port);
                        self.refresh();
                    } else {
                        self.connecting = None;
                    }
                }
                let painter = ui.painter_at(rect);
                let grid = 28. * self.zoom;
                if grid > 8. {
                    let mut x = rect.left() + self.pan.x.rem_euclid(grid);
                    while x < rect.right() {
                        let mut y = rect.top() + self.pan.y.rem_euclid(grid);
                        while y < rect.bottom() {
                            painter.circle_filled(
                                Pos2::new(x, y),
                                1.,
                                Color32::from_rgb(39, 53, 73),
                            );
                            y += grid;
                        }
                        x += grid;
                    }
                }
                for wire in &self.circuit.wires {
                    if let (Some(a), Some(b)) = (
                        self.circuit.node(wire.from.node),
                        self.circuit.node(wire.to.node),
                    ) {
                        draw_wire(
                            &painter,
                            port_pos(a, wire.from.pin, true, origin, self.zoom),
                            port_pos(b, wire.to.pin, false, origin, self.zoom),
                            signal_color(self.sim.output(wire.from)),
                            self.zoom,
                        );
                    }
                }
                if let Some(port) = self.connecting
                    && let Some(n) = self.circuit.node(port.node)
                    && let Some(mouse) = ui.input(|i| i.pointer.hover_pos())
                {
                    draw_wire(
                        &painter,
                        port_pos(n, port.pin, true, origin, self.zoom),
                        mouse,
                        GOLD,
                        self.zoom,
                    );
                }
                for n in &self.circuit.nodes {
                    let r = node_rect(n, origin, self.zoom);
                    if !rect.intersects(r) {
                        continue;
                    }
                    let selected = self.selected == Some(n.id);
                    painter.rect_filled(r, 8., Color32::from_rgb(29, 43, 62));
                    painter.rect_stroke(
                        r,
                        8.,
                        Stroke::new(
                            if selected { 2_f32 } else { 1_f32 },
                            if selected {
                                MINT
                            } else {
                                Color32::from_rgb(64, 83, 107)
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        r.min + Vec2::new(12., 17.) * self.zoom,
                        Align2::LEFT_CENTER,
                        &n.label,
                        FontId::proportional(14. * self.zoom),
                        Color32::WHITE,
                    );
                    painter.text(
                        r.min + Vec2::new(12., 37.) * self.zoom,
                        Align2::LEFT_CENTER,
                        format!("{} · {} bit", n.kind.title(), n.width),
                        FontId::monospace(10. * self.zoom),
                        MUTED,
                    );
                    painter.line_segment(
                        [
                            r.min + Vec2::new(10., 50.) * self.zoom,
                            r.min + Vec2::new(NODE_WIDTH - 10., 50.) * self.zoom,
                        ],
                        Stroke::new(1_f32, Color32::from_rgb(52, 69, 91)),
                    );
                    for (output, pins) in [(false, n.inputs()), (true, n.outputs())] {
                        for (pin, spec) in pins.iter().enumerate() {
                            let point = port_pos(n, pin, output, origin, self.zoom);
                            let port = Port { node: n.id, pin };
                            let signal = if output {
                                self.sim.output(port)
                            } else {
                                self.sim.input(&self.circuit, port)
                            };
                            painter.circle_filled(point, 5.5 * self.zoom, signal_color(signal));
                            let shift = if output { -11. } else { 11. };
                            painter.text(
                                point + Vec2::new(shift * self.zoom, 0.),
                                if output {
                                    Align2::RIGHT_CENTER
                                } else {
                                    Align2::LEFT_CENTER
                                },
                                spec.name,
                                FontId::monospace(11. * self.zoom),
                                MUTED,
                            );
                        }
                    }
                    let value = self.sim.reading(&self.circuit, n);
                    painter.text(
                        Pos2::new(r.center().x, r.bottom() - 17. * self.zoom),
                        Align2::CENTER_CENTER,
                        value.text(),
                        FontId::monospace(13. * self.zoom),
                        signal_color(value),
                    );
                }
                if self.circuit.nodes.is_empty() {
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "Начните с входа и логического элемента\nВыберите блоки в библиотеке слева",
                        FontId::proportional(22.),
                        MUTED,
                    );
                }
                if let Some(pos) = response.hover_pos()
                    && let Some((port, output)) = self.hit_port(origin, pos)
                    && let Some(node) = self.circuit.node(port.node)
                {
                    let pins = if output {
                        node.outputs()
                    } else {
                        node.inputs()
                    };
                    let signal = if output {
                        self.sim.output(port)
                    } else {
                        self.sim.input(&self.circuit, port)
                    };
                    response.on_hover_text(format!(
                        "{} · {} · {} бит\n{}",
                        node.label,
                        pins[port.pin].name,
                        pins[port.pin].width,
                        signal.text()
                    ));
                }
            });
    }
    fn hit_node(&self, origin: Pos2, pos: Pos2) -> Option<u64> {
        self.circuit
            .nodes
            .iter()
            .rev()
            .find(|n| node_rect(n, origin, self.zoom).contains(pos))
            .map(|n| n.id)
    }
    fn hit_port(&self, origin: Pos2, pos: Pos2) -> Option<(Port, bool)> {
        for n in self.circuit.nodes.iter().rev() {
            for (output, count) in [(false, n.inputs().len()), (true, n.outputs().len())] {
                for pin in 0..count {
                    if port_pos(n, pin, output, origin, self.zoom).distance(pos)
                        < (10. * self.zoom).max(6.)
                    {
                        return Some((Port { node: n.id, pin }, output));
                    }
                }
            }
        }
        None
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() {
            let mut choice = 0;
            egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
                ui.heading("Сохранить изменения?");
                ui.label("В текущей схеме есть несохранённые изменения.");
                ui.horizontal(|ui| {
                    if ui.button("Сохранить").clicked() {
                        choice = 1;
                    }
                    if ui.button("Не сохранять").clicked() {
                        choice = 2;
                    }
                    if ui.button("Отмена").clicked() {
                        choice = 3;
                    }
                });
            });
            if choice == 3 {
                self.pending = None;
            }
            if choice == 2 || (choice == 1 && self.save(false)) {
                match self.pending.take() {
                    Some(Pending::Replace(c, p)) => self.replace(c, p),
                    Some(Pending::Close) => {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    None => {}
                }
            }
        }
        egui::Window::new("Как пользоваться Circuit Lab").open(&mut self.help).default_width(540.).show(ctx, |ui| {
            ui.label("1. Выберите разрядность и добавьте блоки из библиотеки.");
            ui.label("2. Соедините выход справа со входом слева. Разрядности должны совпадать.");
            ui.label("3. Выберите вход, введите число в инспекторе и нажмите «Применить».");
            ui.label("4. «Шаг» переключает CLK. Триггеры меняются только по фронту 0 → 1.");
            ui.label("5. «Запуск» выполняет шаги автоматически. «Сброс» обнуляет память.");
            ui.label("6. Сохраните схему в JSON. При загрузке симуляция начинается с Q = 0.");
            ui.separator();
            ui.label("Ctrl+S — сохранить; Ctrl+O — открыть; Ctrl+Z / Ctrl+Y — отмена / повтор; Delete — удалить блок; Esc — отменить соединение.");
            ui.label("Правый щелчок по входному порту удаляет провод. Средняя кнопка перемещает поле. Колесо — масштаб.");
            ui.separator();
            ui.label("Модель: идеальная цифровая логика, без задержек и аналоговых эффектов. Разрядность 1–64. Все арифметические операции беззнаковые. SR здесь — тактируемый триггер, не асинхронная защёлка.");
        });
    }
}

impl eframe::App for CircuitApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.running = false;
            self.pending = Some(Pending::Close);
        }
        if self.pending.is_none() && !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
                self.delete_selected();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.connecting = None;
            }
            if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
                self.save(false);
            }
            if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O)) {
                self.open();
            }
            if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) {
                self.undo(false);
            }
            if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Y)) {
                self.undo(true);
            }
        }
        if self.running && self.last_tick.elapsed() >= Duration::from_secs_f32(1. / self.hz) {
            self.tick();
            self.last_tick = Instant::now();
        }
        if self.running {
            ctx.request_repaint_after(Duration::from_millis(20));
        }
        self.top(ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    if self.running { MINT } else { MUTED },
                    if self.running {
                        "● СИМУЛЯЦИЯ"
                    } else {
                        "● ПАУЗА"
                    },
                );
                ui.separator();
                ui.label(&self.message);
            });
        });
        self.trace_panel(ctx);
        self.palette(ctx);
        self.inspector(ctx);
        self.canvas(ctx);
        self.dialogs(ctx);
    }
}

fn signal_color(signal: Signal) -> Color32 {
    match signal {
        Signal::Value(0) => Color32::from_rgb(107, 136, 164),
        Signal::Value(_) => MINT,
        Signal::Unknown => GOLD,
        _ => ERROR,
    }
}
fn node_height(n: &Node) -> f32 {
    94. + n.inputs().len().max(n.outputs().len()) as f32 * 24.
}
fn node_rect(n: &Node, origin: Pos2, zoom: f32) -> Rect {
    Rect::from_min_size(
        origin + Vec2::new(n.position[0], n.position[1]) * zoom,
        Vec2::new(NODE_WIDTH, node_height(n)) * zoom,
    )
}
fn port_pos(n: &Node, pin: usize, output: bool, origin: Pos2, zoom: f32) -> Pos2 {
    origin
        + Vec2::new(
            n.position[0] + if output { NODE_WIDTH } else { 0. },
            n.position[1] + 68. + pin as f32 * 24.,
        ) * zoom
}
fn draw_wire(p: &egui::Painter, a: Pos2, b: Pos2, color: Color32, zoom: f32) {
    let bend = ((b.x - a.x).abs() * 0.45).max(40. * zoom);
    p.add(egui::epaint::CubicBezierShape::from_points_stroke(
        [a, a + Vec2::new(bend, 0.), b - Vec2::new(bend, 0.), b],
        false,
        Color32::TRANSPARENT,
        Stroke::new(1.8_f32, color),
    ));
}
