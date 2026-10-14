//! RSVP speed reader: single window, display + controls, built with egui.
//!
//! Shortcuts (when the text box isn't focused):
//!   Space      play / pause
//!   Left/Right step one word back / forward
//!   Up/Down    speed +/- 25 WPM
//!   Home       restart
//!   Tab        toggle control panel (focus mode)
//! You can also click the display to play/pause, or drag & drop a .txt file.

use eframe::egui::{self, Align2, Color32, FontId, Key, Sense, Stroke};
use std::time::Instant;

const BLACK: Color32 = Color32::BLACK;
const WHITE: Color32 = Color32::WHITE;
const RED: Color32 = Color32::from_rgb(255, 50, 50);
const GRAY: Color32 = Color32::from_rgb(100, 100, 100);

const SAMPLE: &str = "Paste or type your text here, drag in a text file, or use Open file. \
Press space to start reading. Long words, commas and full stops get a little extra time, \
so the pace feels natural even at high speeds.";

struct App {
    text: String,
    text_dirty: bool,
    words: Vec<String>,
    index: usize,
    playing: bool,
    elapsed_ms: f64,
    wpm: f32,
    font_size: f32,
    smart_pacing: bool,
    show_panel: bool,
    show_guides: bool,
    orp_ratio: f32,
    last_frame: Instant,
    status: String,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(10, 10, 10);
        visuals.extreme_bg_color = Color32::from_rgb(17, 17, 17);
        cc.egui_ctx.set_visuals(visuals);

        let mut app = Self {
            text: SAMPLE.to_owned(),
            text_dirty: false,
            words: Vec::new(),
            index: 0,
            playing: false,
            elapsed_ms: 0.0,
            wpm: 450.0,
            font_size: 140.0,
            smart_pacing: true,
            show_panel: true,
            show_guides: true,
            orp_ratio: 1.0 / 3.0,
            last_frame: Instant::now(),
            status: String::new(),
        };
        app.load_text();
        app
    }

    fn load_text(&mut self) {
        self.words = self.text.split_whitespace().map(str::to_owned).collect();
        self.index = 0;
        self.elapsed_ms = 0.0;
        self.playing = false;
        self.text_dirty = false;
    }

    fn toggle_play(&mut self) {
        if self.text_dirty {
            self.load_text();
        }
        if self.words.is_empty() {
            return;
        }
        if !self.playing && self.index + 1 >= self.words.len() {
            self.index = 0; // replay from the start when finished
        }
        self.playing = !self.playing;
        self.elapsed_ms = 0.0;
    }

    fn step(&mut self, delta: isize) {
        if self.words.is_empty() {
            return;
        }
        let max = self.words.len() as isize - 1;
        self.index = (self.index as isize + delta).clamp(0, max) as usize;
        self.elapsed_ms = 0.0;
    }

    /// Display time for a word in milliseconds.
    fn delay_ms(&self, word: &str) -> f64 {
        let base = 60_000.0 / self.wpm as f64;
        if !self.smart_pacing {
            return base;
        }
        let trimmed = word.trim_end_matches(|c| matches!(c, '"' | '\'' | '”' | '’' | ')' | ']'));
        let len = trimmed.chars().filter(|c| c.is_alphanumeric()).count();

        let mut mult = 1.0;
        if len > 6 {
            mult += ((len - 6) as f64 * 0.06).min(0.6);
        }
        match trimmed.chars().last() {
            Some('.' | '!' | '?' | '…') => mult += 1.0,
            Some(',' | ';' | ':' | '—' | '–') => mult += 0.5,
            _ => {}
        }
        base * mult
    }

    fn advance(&mut self, dt_ms: f64) {
        self.elapsed_ms += dt_ms;
        loop {
            let Some(word) = self.words.get(self.index) else {
                self.playing = false;
                return;
            };
            let delay = self.delay_ms(word);
            if self.elapsed_ms < delay {
                break;
            }
            self.elapsed_ms -= delay;
            if self.index + 1 >= self.words.len() {
                self.playing = false;
                self.elapsed_ms = 0.0;
                break;
            }
            self.index += 1;
        }
    }

    /// Index (in chars) of the optimal recognition point, skipping leading punctuation.
    fn orp_index(chars: &[char]) -> usize {
        let offset = chars.iter().take_while(|c| !c.is_alphanumeric()).count();
        let core = chars[offset..]
            .iter()
            .filter(|c| c.is_alphanumeric())
            .count();
        let orp = match core {
            0..=1 => 0,
            2..=5 => 1,
            6..=9 => 2,
            _ => 3,
        };
        (offset + orp).min(chars.len().saturating_sub(1))
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        // Dropped text files
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            if let Some(path) = f.path {
                self.open_path(&path);
            }
        }

        if ctx.wants_keyboard_input() {
            return;
        }
        let (space, left, right, up, down, home, tab) = ctx.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::Tab),
            )
        });
        if space {
            self.toggle_play();
        }
        if left {
            self.step(-1);
        }
        if right {
            self.step(1);
        }
        if up {
            self.wpm = (self.wpm + 25.0).min(1000.0);
        }
        if down {
            self.wpm = (self.wpm - 25.0).max(100.0);
        }
        if home {
            self.index = 0;
            self.elapsed_ms = 0.0;
        }
        if tab {
            self.show_panel = !self.show_panel;
        }
    }

    fn open_path(&mut self, path: &std::path::Path) {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                self.text = s;
                self.load_text();
                self.status = format!("Loaded {}", path.display());
            }
            Err(e) => self.status = format!("Could not read file: {e}"),
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.heading("RSVP Reader");
        ui.add_space(6.0);

        let resp = ui.add(
            egui::TextEdit::multiline(&mut self.text)
                .desired_rows(12)
                .desired_width(f32::INFINITY)
                .hint_text("Paste text here…"),
        );
        if resp.changed() {
            self.text_dirty = true;
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let label = if self.playing { "⏸ Pause" } else { "▶ Play" };
            if ui.button(label).clicked() {
                self.toggle_play();
            }
            if ui.button("⏮ Restart").clicked() {
                if self.text_dirty {
                    self.load_text();
                }
                self.index = 0;
                self.elapsed_ms = 0.0;
            }
            if ui.button("📂 Open file").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Text", &["txt", "md"])
                    .pick_file()
                {
                    self.open_path(&path);
                }
            }
        });

        ui.add_space(10.0);
        ui.add(
            egui::Slider::new(&mut self.wpm, 100.0..=1000.0)
                .step_by(25.0)
                .text("WPM"),
        );
        ui.add(egui::Slider::new(&mut self.font_size, 40.0..=300.0).text("Font size"));
        ui.add(egui::Slider::new(&mut self.orp_ratio, 0.2..=0.5).text("Focus position"));
        ui.checkbox(&mut self.smart_pacing, "Smart pacing (punctuation & long words)");
        ui.checkbox(&mut self.show_guides, "Show guide lines");

        ui.add_space(10.0);
        if !self.words.is_empty() {
            let mut idx = self.index;
            let last = self.words.len() - 1;
            if ui
                .add(egui::Slider::new(&mut idx, 0..=last).show_value(false).text("Position"))
                .changed()
            {
                self.index = idx;
                self.elapsed_ms = 0.0;
            }
            let remaining = (self.words.len() - self.index) as f64 / self.wpm as f64;
            ui.label(format!(
                "Word {} / {}  ·  ~{:.1} min left",
                self.index + 1,
                self.words.len(),
                remaining
            ));
        }
        if !self.status.is_empty() {
            ui.label(&self.status);
        }

        ui.add_space(10.0);
        ui.separator();
        ui.small("Space play/pause · ←/→ step · ↑/↓ speed · Home restart · Tab hide panel");
    }

    fn draw_word(&self, ctx: &egui::Context, painter: &egui::Painter, rect: egui::Rect) {
        let Some(word) = self.words.get(self.index) else {
            return;
        };
        let chars: Vec<char> = word.chars().collect();
        let idx = Self::orp_index(&chars);
        let p1: String = chars[..idx].iter().collect();
        let focus: String = chars[idx].to_string();
        let p2: String = chars[idx + 1..].iter().collect();

        let cx = rect.left() + rect.width() * self.orp_ratio;
        let cy = rect.center().y;

        let layout = |size: f32| {
            let font = FontId::proportional(size);
            ctx.fonts(|f| {
                (
                    f.layout_no_wrap(p1.clone(), font.clone(), WHITE),
                    f.layout_no_wrap(focus.clone(), font.clone(), RED),
                    f.layout_no_wrap(p2.clone(), font.clone(), WHITE),
                )
            })
        };

        // Shrink the font if the word would run off either side of the window.
        let margin = 24.0;
        let (mut g1, mut gf, mut g2) = layout(self.font_size);
        let left_need = g1.size().x + gf.size().x / 2.0;
        let right_need = g2.size().x + gf.size().x / 2.0;
        let scale = ((cx - rect.left() - margin) / left_need.max(1.0))
            .min((rect.right() - cx - margin) / right_need.max(1.0))
            .min(1.0);
        if scale < 1.0 {
            (g1, gf, g2) = layout((self.font_size * scale).max(12.0));
        }

        let fx = cx - gf.size().x / 2.0;
        let fy = cy - gf.size().y / 2.0;
        let fh = gf.size().y;
        let fw = gf.size().x;

        painter.galley(egui::pos2(fx - g1.size().x, fy), g1, WHITE);
        painter.galley(egui::pos2(fx, fy), gf, RED);
        painter.galley(egui::pos2(fx + fw, fy), g2, WHITE);

        if self.show_guides {
            let stroke = Stroke::new(2.0, GRAY);
            painter.line_segment(
                [egui::pos2(cx, fy - 30.0), egui::pos2(cx, fy - 50.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(cx, fy + fh + 30.0), egui::pos2(cx, fy + fh + 50.0)],
                stroke,
            );
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last_frame).as_secs_f64() * 1000.0;
        self.last_frame = now;

        self.handle_input(ctx);

        if self.playing {
            self.advance(dt_ms.min(100.0));
            ctx.request_repaint();
        }

        if self.show_panel {
            egui::SidePanel::right("controls")
                .min_width(320.0)
                .default_width(360.0)
                .show(ctx, |ui| self.controls(ui));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BLACK))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let resp = ui.allocate_rect(rect, Sense::click());
                if resp.clicked() {
                    self.toggle_play();
                }
                let painter = ui.painter_at(rect);
                self.draw_word(ctx, &painter, rect);

                // Thin progress bar along the bottom edge.
                if !self.words.is_empty() {
                    let frac = (self.index + 1) as f32 / self.words.len() as f32;
                    let bar = egui::Rect::from_min_max(
                        egui::pos2(rect.left(), rect.bottom() - 3.0),
                        egui::pos2(rect.left() + rect.width() * frac, rect.bottom()),
                    );
                    painter.rect_filled(bar, 0.0, RED);
                }
                if !self.playing && !self.words.is_empty() {
                    painter.text(
                        rect.center_bottom() - egui::vec2(0.0, 18.0),
                        Align2::CENTER_CENTER,
                        "paused: space to play",
                        FontId::proportional(14.0),
                        GRAY,
                    );
                }
            });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1150.0, 520.0])
            .with_title("RSVP Reader"),
        ..Default::default()
    };
    eframe::run_native(
        "RSVP Reader",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}