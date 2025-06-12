use chrono::Local;
use eframe::{egui, App, Frame};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CUSTOM_FONT_DATA: &[u8] = include_bytes!("方正小标宋简体.TTF");

struct ClockApp {
    countdown_input: String,
    countdown_start: Option<Instant>,
    countdown_duration: Option<Duration>,
    countdown_active: bool,
}

impl Default for ClockApp {
    fn default() -> Self {
        Self {
            countdown_input: String::new(),
            countdown_start: None,
            countdown_duration: None,
            countdown_active: false,
        }
    }
}

impl App for ClockApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let now = Local::now();
            let time_string = now.format("%H:%M:%S").to_string();
            ui.heading("当前时间");
            ui.label(time_string);

            ui.separator();

            ui.heading("倒计时");

            ui.horizontal(|ui| {
                ui.label("秒数:");
                ui.text_edit_singleline(&mut self.countdown_input);
            });

            if !self.countdown_active {
                if ui.button("开始倒计时").clicked() {
                    if let Ok(secs) = self.countdown_input.trim().parse::<u64>() {
                        if secs > 0 {
                            self.countdown_start = Some(Instant::now());
                            self.countdown_duration = Some(Duration::from_secs(secs));
                            self.countdown_active = true;
                        }
                    }
                }
            } else {
                if let (Some(start), Some(duration)) = (self.countdown_start, self.countdown_duration) {
                    let elapsed = start.elapsed();
                    if elapsed >= duration {
                        self.countdown_active = false;
                        self.countdown_start = None;
                        self.countdown_duration = None;
                        self.countdown_input.clear();

                        ui.label("倒计时结束！");
                    } else {
                        let remain = duration - elapsed;
                        ui.label(format!("倒计时剩余: {}秒", remain.as_secs()));
                        ui.label("点击“停止”以取消倒计时");
                    }
                }

                if ui.button("停止").clicked() {
                    self.countdown_active = false;
                    self.countdown_start = None;
                    self.countdown_duration = None;
                    self.countdown_input.clear();
                }
            }
        });

        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

fn main() {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
    "Rust Clock + Countdown",
    native_options,
    Box::new(|cc| {
        let mut fonts = egui::FontDefinitions::default();

        fonts.font_data.insert(
            "fz_font".to_owned(),
            egui::FontData::from_static(CUSTOM_FONT_DATA).into(),
        );

        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "fz_font".to_owned());
        fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "fz_font".to_owned());

        cc.egui_ctx.set_fonts(fonts);

        Ok(Box::new(ClockApp::default()))
    }),
);

}
