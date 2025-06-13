use chrono::{DateTime, Local};
use eframe::{egui, App, Frame};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Cursor,
    path::Path,
    time::{Duration, Instant},
};

use egui::{Color32, Rect, TextureOptions, RichText};

const CUSTOM_FONT_DATA: &[u8] = include_bytes!("方正小标宋简体.TTF");
const ALARM_WAV: &[u8] = include_bytes!("alarm.wav");
const BACKGROUND_IMAGE_PATH: &str = "background.png";

#[derive(Clone, Serialize, Deserialize)]
struct CountdownTask {
    id: usize,
    name: String, // 新增任务名
    input: String,
    duration: Duration,
    created_at: DateTime<Local>,
    #[serde(skip)]
    start: Option<Instant>,
    paused: bool,
    #[serde(skip)]
    pause_start: Option<Instant>,
    elapsed_before_pause: Duration,
    #[serde(skip)]
    finished_at: Option<DateTime<Local>>,
}

impl CountdownTask {
    fn new(id: usize, name: String, input: String, duration: Duration) -> Self {
        Self {
            id,
            name,
            input,
            duration,
            created_at: Local::now(),
            start: Some(Instant::now()),
            paused: false,
            pause_start: None,
            elapsed_before_pause: Duration::ZERO,
            finished_at: None,
        }
    }

    fn elapsed(&self) -> Duration {
        if let Some(start) = self.start {
            if self.paused {
                self.elapsed_before_pause
            } else {
                self.elapsed_before_pause + start.elapsed()
            }
        } else {
            Duration::ZERO
        }
    }

    fn remaining(&self) -> Duration {
        let elapsed = self.elapsed();
        if elapsed >= self.duration {
            Duration::ZERO
        } else {
            self.duration - elapsed
        }
    }

    fn is_finished(&self) -> bool {
        self.elapsed() >= self.duration
    }
}

#[derive(Serialize, Deserialize, Default)]
struct PersistentData {
    history: Vec<CountdownTask>,
    text_color: [u8; 4], // egui::Color32 RGBA
}

struct ClockApp {
    tasks: Vec<CountdownTask>,
    next_task_id: usize,
    new_task_input: String,
    new_task_name: String, // 新增任务名输入框内容
    history: Vec<CountdownTask>,
    show_finished_popup: Option<usize>,

    background_texture: Option<egui::TextureHandle>,
    text_color: Color32,

    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    active_sinks: Vec<Sink>,
}

impl Default for ClockApp {
    fn default() -> Self {
        let (_stream, stream_handle) =
            OutputStream::try_default().expect("Failed to initialize audio output");

        Self {
            tasks: Vec::new(),
            next_task_id: 0,
            new_task_input: String::new(),
            new_task_name: String::new(),
            history: Vec::new(),
            show_finished_popup: None,
            background_texture: None,
            text_color: Color32::from_rgb(220, 220, 220),
            _stream,
            stream_handle,
            active_sinks: Vec::new(),
        }
    }
}

impl ClockApp {
    fn data_path() -> &'static str {
        "countdown_data.json"
    }

    fn load_data(&mut self) {
        if Path::new(Self::data_path()).exists() {
            if let Ok(data) = fs::read_to_string(Self::data_path()) {
                if let Ok(persist) = serde_json::from_str::<PersistentData>(&data) {
                    self.history = persist.history;
                    self.text_color = Color32::from_rgba_unmultiplied(
                        persist.text_color[0],
                        persist.text_color[1],
                        persist.text_color[2],
                        persist.text_color[3],
                    );
                    if let Some(max_id) = self.history.iter().map(|t| t.id).max() {
                        self.next_task_id = max_id + 1;
                    }
                }
            }
        }
    }

    fn save_data(&self) {
        let persist = PersistentData {
            history: self.history.clone(),
            text_color: self.text_color.to_array(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&persist) {
            let _ = fs::write(Self::data_path(), json);
        }
    }

    fn parse_duration(input: &str) -> Option<Duration> {
        let parts: Vec<&str> = input.trim().split(':').collect();
        match parts.len() {
            1 => parts[0].parse::<u64>().ok().map(Duration::from_secs),
            2 => {
                let mins = parts[0].parse::<u64>().ok()?;
                let secs = parts[1].parse::<u64>().ok()?;
                Some(Duration::from_secs(mins * 60 + secs))
            }
            3 => {
                let hours = parts[0].parse::<u64>().ok()?;
                let mins = parts[1].parse::<u64>().ok()?;
                let secs = parts[2].parse::<u64>().ok()?;
                Some(Duration::from_secs(hours * 3600 + mins * 60 + secs))
            }
            _ => None,
        }
    }

    fn play_alarm_sound(&mut self) {
        if let Ok(sink) = Sink::try_new(&self.stream_handle) {
            let cursor = Cursor::new(ALARM_WAV);
            if let Ok(source) = Decoder::new(cursor) {
                sink.append(source);
                self.active_sinks.push(sink);
            }
        }
    }

    fn show_notification(summary: &str, body: &str) {
        let _ = notify_rust::Notification::new()
            .summary(summary)
            .body(body)
            .show();
    }

    fn load_background(&mut self, ctx: &egui::Context) {
        if self.background_texture.is_some() {
            return;
        }
        if Path::new(BACKGROUND_IMAGE_PATH).exists() {
            if let Ok(img) = image::open(BACKGROUND_IMAGE_PATH) {
                let size = [img.width() as usize, img.height() as usize];
                let img = img.to_rgba8();
                let pixels = img.as_flat_samples();
                let color_image =
                    egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                self.background_texture = Some(ctx.load_texture(
                    "background",
                    color_image,
                    TextureOptions::LINEAR,
                ));
            }
        }
    }
}

impl App for ClockApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        use egui::*;

        self.active_sinks.retain(|sink| !sink.empty());

        let mut style = (*ctx.style()).clone();
        style.visuals.override_text_color = Some(self.text_color);
        ctx.set_style(style);

        self.load_background(ctx);

        if let Some(texture) = &self.background_texture {
            let painter = ctx.layer_painter(LayerId::background());
            let rect = ctx.input(|i| i.screen_rect());
            painter.image(texture.id(), rect, Rect::from_min_max(rect.min, rect.max), Color32::WHITE);
        }

        CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);
                ui.heading(
                    RichText::new(Local::now().format("%H:%M:%S").to_string())
                        .size(48.0)
                        .color(self.text_color),
                );
                ui.add_space(10.0);
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("文字颜色:");
                let mut color = {
                    let [r, g, b, _a] = self.text_color.to_array();
                    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
                };
                if ui.color_edit_button_rgb(&mut color).changed() {
                    self.text_color = Color32::from_rgb(
                        (color[0] * 255.0) as u8,
                        (color[1] * 255.0) as u8,
                        (color[2] * 255.0) as u8,
                    );
                    self.save_data();
                }
            });

            ui.separator();

            // 改为垂直布局，避免按钮被挤出窗口
            ui.group(|ui| {
                ui.label("任务名:");
                ui.text_edit_singleline(&mut self.new_task_name);
                ui.add_space(4.0);

                ui.label("倒计时 (秒或 HH:MM:SS):");
                ui.text_edit_singleline(&mut self.new_task_input);
                ui.add_space(4.0);

                if ui.button("添加").clicked() {
                    if let Some(dur) = Self::parse_duration(&self.new_task_input) {
                        if dur.as_secs() > 0 {
                            let id = self.next_task_id;
                            self.next_task_id += 1;
                            let name = if self.new_task_name.trim().is_empty() {
                                format!("任务#{}", id)
                            } else {
                                self.new_task_name.trim().to_string()
                            };
                            self.tasks.push(CountdownTask::new(
                                id,
                                name,
                                self.new_task_input.clone(),
                                dur,
                            ));
                            self.new_task_input.clear();
                            self.new_task_name.clear();
                        }
                    }
                }
            });

            ui.separator();

            let mut just_finished_tasks = Vec::new();

            ui.push_id("countdown_tasks", |ui| {
                ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    let mut remove_ids = Vec::new();

                    for task in &mut self.tasks {
                        if task.is_finished() && task.finished_at.is_none() {
                            task.finished_at = Some(Local::now());
                            just_finished_tasks.push(task.clone());
                        }

                        ui.group(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(format!("任务名: {}", task.name)).strong(),
                                );
                                ui.label(RichText::new(
                                    format!("开始时间: {}", task.created_at.format("%Y-%m-%d %H:%M:%S")),
                                ));
                                ui.label(format!("设定时长: {}", task.input));

                                ui.horizontal(|ui| {
                                    let remain = task.remaining();
                                    ui.label(format!(
                                        "剩余时间: {:02}:{:02}:{:02}",
                                        remain.as_secs() / 3600,
                                        (remain.as_secs() / 60) % 60,
                                        remain.as_secs() % 60
                                    ));
                                    let progress = 1.0 - remain.as_secs_f32() / task.duration.as_secs_f32();
                                    ui.add(ProgressBar::new(progress).show_percentage());
                                });

                                ui.horizontal(|ui| {
                                    if task.is_finished() {
                                        if ui.button("删除").clicked() {
                                            remove_ids.push(task.id);
                                        }
                                    } else {
                                        if task.paused {
                                            if ui.button("继续").clicked() {
                                                if let Some(pause_start) = task.pause_start {
                                                    let paused_dur = pause_start.elapsed();
                                                    task.elapsed_before_pause += paused_dur;
                                                    task.paused = false;
                                                    task.pause_start = None;
                                                }
                                            }
                                        } else if ui.button("暂停").clicked() {
                                            task.paused = true;
                                            task.pause_start = Some(Instant::now());
                                        }

                                        if ui.button("停止").clicked() {
                                            remove_ids.push(task.id);
                                        }
                                    }
                                });
                            });
                        });

                        ui.add_space(10.0);
                    }

                    self.tasks.retain(|t| !remove_ids.contains(&t.id));

                    for task in just_finished_tasks {
                        self.play_alarm_sound();
                        Self::show_notification(
                            "倒计时结束",
                            &format!("任务“{}”开始于 {} 的倒计时已结束", task.name, task.created_at.format("%Y-%m-%d %H:%M:%S")),
                        );
                        self.history.push(task.clone());
                        self.save_data();
                        self.show_finished_popup = Some(task.id);
                    }
                });
            });

            ui.separator();

            ui.heading("历史记录");

            ui.push_id("history_list", |ui| {
                ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                    if self.history.is_empty() {
                        ui.label("暂无历史记录");
                    }
                    let mut remove_history_ids = Vec::new();
                    for task in self.history.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "任务名: {}，开始时间: {}，设定时长: {}",
                                task.name,
                                task.created_at.format("%Y-%m-%d %H:%M:%S"),
                                task.input
                            ));
                            if ui.button("删除").clicked() {
                                remove_history_ids.push(task.id);
                            }
                        });
                        ui.add_space(4.0);
                    }
                    if !remove_history_ids.is_empty() {
                        self.history.retain(|t| !remove_history_ids.contains(&t.id));
                        self.save_data();
                    }
                });
            });
        });

        if let Some(id) = self.show_finished_popup {
            Window::new("提醒")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let task_time = self
                        .tasks
                        .iter()
                        .find(|t| t.id == id)
                        .map(|t| t.created_at.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_else(|| "未知".to_string());
                    let task_name = self
                        .tasks
                        .iter()
                        .find(|t| t.id == id)
                        .map(|t| t.name.clone())
                        .unwrap_or_else(|| "未知任务".to_string());
                    ui.label(format!("任务“{}”开始于 {} 的倒计时已结束！", task_name, task_time));
                    if ui.button("关闭").clicked() {
                        self.show_finished_popup = None;
                    }
                });
        }

        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

fn main() {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        "Rust 多任务倒计时",
        native_options,
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "fz_font".to_owned(),
                egui::FontData::from_static(CUSTOM_FONT_DATA).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "fz_font".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "fz_font".to_owned());
            cc.egui_ctx.set_fonts(fonts);

            let mut app = ClockApp::default();
            app.load_data();
            Box::new(app)
        }),
    );
}
