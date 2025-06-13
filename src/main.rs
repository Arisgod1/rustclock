use chrono::Local;
use eframe::{egui, App, Frame};
use std::time::{Duration, Instant};

const CUSTOM_FONT_DATA: &[u8] = include_bytes!("方正小标宋简体.TTF");

struct ClockApp {
    countdown_input: String,
    countdown_start: Option<Instant>,
    countdown_duration: Option<Duration>,
    countdown_active: bool,
    paused: bool,
    paused_instant: Option<Instant>,
    history: Vec<(String, chrono::DateTime<Local>)>,
    show_finished_popup: bool,
}

impl Default for ClockApp {
    fn default() -> Self {
        Self {
            countdown_input: String::new(),
            countdown_start: None,
            countdown_duration: None,
            countdown_active: false,
            paused: false,
            paused_instant: None,
            history: Vec::new(),
            show_finished_popup: false,
        }
    }
}

impl ClockApp {
    /// 支持秒或者 HH:MM:SS 格式的解析
    fn parse_duration(input: &str) -> Option<Duration> {
        let parts: Vec<&str> = input.trim().split(':').collect();
        match parts.len() {
            1 => {
                // 纯秒数
                parts[0].parse::<u64>().ok().map(Duration::from_secs)
            }
            2 => {
                // MM:SS
                let mins = parts[0].parse::<u64>().ok()?;
                let secs = parts[1].parse::<u64>().ok()?;
                Some(Duration::from_secs(mins * 60 + secs))
            }
            3 => {
                // HH:MM:SS
                let hours = parts[0].parse::<u64>().ok()?;
                let mins = parts[1].parse::<u64>().ok()?;
                let secs = parts[2].parse::<u64>().ok()?;
                Some(Duration::from_secs(hours * 3600 + mins * 60 + secs))
            }
            _ => None,
        }
    }

    /// 获取倒计时已流逝时间，已考虑暂停时间
    fn elapsed(&self) -> Duration {
        if !self.countdown_active {
            return Duration::ZERO;
        }
        if let Some(start) = self.countdown_start {
            if self.paused {
                if let Some(pause_start) = self.paused_instant {
                    // 处于暂停状态，elapsed计到暂停开始时刻
                    pause_start.duration_since(start)
                } else {
                    Duration::ZERO
                }
            } else {
                start.elapsed()
            }
        } else {
            Duration::ZERO
        }
    }
}

impl App for ClockApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // 当前时间显示
            let now = Local::now();
            let time_string = now.format("%H:%M:%S").to_string();
            ui.heading("当前时间");
            ui.label(time_string);

            ui.separator();

            // 倒计时输入和控制
            ui.heading("倒计时 (支持秒 或 HH:MM:SS 格式)");

            ui.horizontal(|ui| {
                ui.label("时间:");
                ui.text_edit_singleline(&mut self.countdown_input);
            });

            if !self.countdown_active {
                if ui.button("开始倒计时").clicked() {
                    if let Some(dur) = Self::parse_duration(&self.countdown_input) {
                        if dur.as_secs() > 0 {
                            self.countdown_start = Some(Instant::now());
                            self.countdown_duration = Some(dur);
                            self.countdown_active = true;
                            self.paused = false;
                            self.paused_instant = None;
                        }
                    }
                }
            } else {
                let duration = self.countdown_duration.unwrap_or_default();
                let elapsed = self.elapsed();

                if elapsed >= duration {
                    // 结束倒计时
                    self.countdown_active = false;
                    self.countdown_start = None;
                    self.countdown_duration = None;
                    self.paused = false;
                    self.paused_instant = None;

                    // 记录历史
                    let input_str = self.countdown_input.clone();
                    self.history.push((input_str, Local::now()));
                    self.countdown_input.clear();

                    // 弹窗提醒
                    self.show_finished_popup = true;
                } else {
                    let remain = duration - elapsed;
                    let progress = 1.0 - remain.as_secs_f32() / duration.as_secs_f32();

                    ui.label(format!(
                        "倒计时剩余: {:02}:{:02}:{:02}",
                        remain.as_secs() / 3600,
                        (remain.as_secs() / 60) % 60,
                        remain.as_secs() % 60
                    ));
                    ui.add(egui::ProgressBar::new(progress).show_percentage());

                    ui.horizontal(|ui| {
                        if self.paused {
                            if ui.button("继续").clicked() {
                                if let Some(pause_start) = self.paused_instant {
                                    let paused_duration = pause_start.elapsed();
                                    if let Some(start) = self.countdown_start.as_mut() {
                                        *start += paused_duration;
                                    }
                                    self.paused = false;
                                    self.paused_instant = None;
                                }
                            }
                        } else {
                            if ui.button("暂停").clicked() {
                                self.paused = true;
                                self.paused_instant = Some(Instant::now());
                            }
                        }

                        if ui.button("停止").clicked() {
                            self.countdown_active = false;
                            self.countdown_start = None;
                            self.countdown_duration = None;
                            self.paused = false;
                            self.paused_instant = None;
                            self.countdown_input.clear();
                        }
                    });
                }
            }

            ui.separator();

            // 历史记录显示
            ui.heading("倒计时历史");
            egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                if self.history.is_empty() {
                    ui.label("暂无历史记录");
                } else {
                    for (dur_str, finish_time) in self.history.iter().rev() {
                        ui.label(format!(
                            "倒计时 {} 于 {} 结束",
                            dur_str,
                            finish_time.format("%Y-%m-%d %H:%M:%S")
                        ));
                    }
                }
            });
        });

        // 倒计时结束弹窗
        if self.show_finished_popup {
            egui::Window::new("提醒")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("倒计时结束！");
                    if ui.button("关闭").clicked() {
                        self.show_finished_popup = false;
                    }
                });
        }

        // 请求重绘，保证动画和时间更新
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

fn main() {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        "Rust Clock + Countdown (增强版)",
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

            Ok(Box::new(ClockApp::default()))
        }),
    );
}
