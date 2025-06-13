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

use egui::{Color32, Rect, TextureOptions};

const CUSTOM_FONT_DATA: &[u8] = include_bytes!("方正小标宋简体.TTF");
const ALARM_WAV: &[u8] = include_bytes!("alarm.wav");
const BACKGROUND_IMAGE_PATH: &str = "background.png";

#[derive(Clone, Serialize, Deserialize)]
struct CountdownTask {
    id: usize,
    input: String,
    duration: Duration,
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
    fn new(id: usize, input: String, duration: Duration) -> Self {
        Self {
            id,
            input,
            duration,
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

struct ClockApp {
    tasks: Vec<CountdownTask>,
    next_task_id: usize,
    new_task_input: String,
    history: Vec<CountdownTask>,
    show_finished_popup: Option<usize>,

    background_texture: Option<egui::TextureHandle>,
    text_color: Color32,

    // 音频播放相关字段，保持音频流和句柄活着
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    active_sinks: Vec<Sink>, // 保存正在播放的Sink
}

impl Default for ClockApp {
    fn default() -> Self {
        let (_stream, stream_handle) =
            OutputStream::try_default().expect("Failed to initialize audio output");

        Self {
            tasks: Vec::new(),
            next_task_id: 0,
            new_task_input: String::new(),
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
                // 不调用 detach，保持 sink 活着
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

    fn history_path() -> &'static str {
        "countdown_history.json"
    }

    fn load_history(&mut self) {
        if Path::new(Self::history_path()).exists() {
            if let Ok(data) = fs::read_to_string(Self::history_path()) {
                if let Ok(hist) = serde_json::from_str::<Vec<CountdownTask>>(&data) {
                    self.history = hist;
                    // 设置 next_task_id 为最大已用 ID + 1，避免编号重复
                    if let Some(max_id) = self.history.iter().map(|t| t.id).max() {
                        self.next_task_id = max_id + 1;
                    }
                }
            }
        }
    }

    fn save_history(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.history) {
            let _ = fs::write(Self::history_path(), json);
        }
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

        // 清理已播放完的Sink
        self.active_sinks.retain(|sink| !sink.empty());

        // 设置文字颜色
        let mut style = (*ctx.style()).clone();
        style.visuals.override_text_color = Some(self.text_color);
        ctx.set_style(style);

        // 加载背景纹理
        self.load_background(ctx);

        // 绘制背景
        if let Some(texture) = &self.background_texture {
            let painter = ctx.layer_painter(LayerId::background());
            let rect = ctx.input(|i| i.screen_rect());
            painter.image(texture.id(), rect, Rect::from_min_max(rect.min, rect.max), Color32::WHITE);
        }

        CentralPanel::default().show(ctx, |ui| {
            ui.heading("当前时间");
            let now = Local::now();
            ui.label(now.format("%H:%M:%S").to_string());

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
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("新倒计时 (秒或 HH:MM:SS):");
                ui.text_edit_singleline(&mut self.new_task_input);
                if ui.button("添加").clicked() {
                    if let Some(dur) = Self::parse_duration(&self.new_task_input) {
                        if dur.as_secs() > 0 {
                            let id = self.next_task_id;
                            self.next_task_id += 1;
                            self.tasks.push(CountdownTask::new(id, self.new_task_input.clone(), dur));
                            self.new_task_input.clear();
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
                            ui.horizontal(|ui| {
                                ui.label(format!("任务#{}，设定时间: {}", task.id, task.input));
                                let remain = task.remaining();
                                ui.label(format!(
                                    "剩余 {:02}:{:02}:{:02}",
                                    remain.as_secs() / 3600,
                                    (remain.as_secs() / 60) % 60,
                                    remain.as_secs() % 60
                                ));
                                let progress = 1.0 - remain.as_secs_f32() / task.duration.as_secs_f32();
                                ui.add(ProgressBar::new(progress).show_percentage());

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
                    }

                    self.tasks.retain(|t| !remove_ids.contains(&t.id));
                });
            });

            for task in just_finished_tasks {
                self.play_alarm_sound();
                Self::show_notification("倒计时结束", &format!("任务#{} 已结束", task.id));
                self.history.push(task.clone());
                self.save_history();
                self.show_finished_popup = Some(task.id);
            }

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
                            ui.label(format!("任务#{} 结束，设定时间: {}", task.id, task.input));
                            if ui.button("删除").clicked() {
                                remove_history_ids.push(task.id);
                            }
                        });
                    }
                    if !remove_history_ids.is_empty() {
                        self.history.retain(|t| !remove_history_ids.contains(&t.id));
                        self.save_history();
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
                    ui.label(format!("任务#{} 已结束！", id));
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
        "Rust 多任务倒计时 (带声音提醒/通知/历史)",
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
            app.load_history();
            Box::new(app)
        }),
    );
}
