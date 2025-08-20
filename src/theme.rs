use eframe::egui;

// --- ライトモードのVisualsを返す関数 ---
pub fn light_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::light();
    let background_color = egui::Color32::from_rgb(242, 242, 247);
    let panel_color = egui::Color32::from_rgb(255, 255, 255);
    let text_color = egui::Color32::BLACK;
    let accent_color = egui::Color32::from_rgb(0, 110, 230);
    let separator_color = egui::Color32::from_gray(225);

    visuals.window_fill = background_color;
    visuals.panel_fill = panel_color;
    visuals.override_text_color = Some(text_color);
    visuals.hyperlink_color = accent_color;
    visuals.faint_bg_color = background_color;
    visuals.extreme_bg_color = egui::Color32::from_gray(230);
    visuals.window_stroke = egui::Stroke::new(1.0, separator_color);
    visuals.selection.bg_fill = accent_color.linear_multiply(0.3);
    visuals.selection.stroke = egui::Stroke::new(1.0, text_color);

    let widget_visuals = &mut visuals.widgets;
    widget_visuals.noninteractive.bg_fill = egui::Color32::TRANSPARENT;
    widget_visuals.noninteractive.bg_stroke = egui::Stroke::NONE;
    widget_visuals.noninteractive.fg_stroke = egui::Stroke::new(1.0, text_color);

    widget_visuals.inactive.bg_fill = egui::Color32::from_gray(235);
    widget_visuals.inactive.bg_stroke = egui::Stroke::NONE;
    widget_visuals.inactive.fg_stroke = egui::Stroke::new(1.0, text_color);

    widget_visuals.hovered.bg_fill = egui::Color32::from_gray(220);
    widget_visuals.hovered.bg_stroke = egui::Stroke::NONE;
    widget_visuals.hovered.fg_stroke = egui::Stroke::new(1.0, text_color);

    widget_visuals.active.bg_fill = egui::Color32::from_gray(210);
    widget_visuals.active.bg_stroke = egui::Stroke::NONE;
    widget_visuals.active.fg_stroke = egui::Stroke::new(1.0, accent_color);

    visuals
}

// --- ダークモードのVisualsを返す関数 ---
pub fn dark_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    let background_color = egui::Color32::from_rgb(29, 29, 31);
    let panel_color = egui::Color32::from_rgb(44, 44, 46);
    let text_color = egui::Color32::from_gray(230);
    let accent_color = egui::Color32::from_rgb(10, 132, 255);
    let separator_color = egui::Color32::from_gray(58);

    visuals.window_fill = background_color;
    visuals.panel_fill = panel_color;
    visuals.override_text_color = Some(text_color);
    visuals.hyperlink_color = accent_color;
    visuals.faint_bg_color = background_color;
    visuals.extreme_bg_color = egui::Color32::from_gray(60);
    visuals.window_stroke = egui::Stroke::new(1.0, separator_color);
    visuals.selection.bg_fill = accent_color.linear_multiply(0.3);
    visuals.selection.stroke = egui::Stroke::new(1.0, text_color);

    let widget_visuals = &mut visuals.widgets;
    widget_visuals.noninteractive.bg_fill = egui::Color32::from_rgb(40, 40, 40);
    widget_visuals.noninteractive.bg_stroke = egui::Stroke::NONE;
    widget_visuals.noninteractive.fg_stroke = egui::Stroke::new(1.0, text_color);

    widget_visuals.inactive.bg_fill = egui::Color32::from_gray(50);
    widget_visuals.inactive.bg_stroke = egui::Stroke::NONE;
    widget_visuals.inactive.fg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 180, 180));

    widget_visuals.hovered.bg_fill = egui::Color32::from_gray(70);
    widget_visuals.hovered.bg_stroke = egui::Stroke::NONE;
    widget_visuals.hovered.fg_stroke = egui::Stroke::new(1.0, text_color);

    widget_visuals.active.bg_fill = egui::Color32::from_gray(85);
    widget_visuals.active.bg_stroke = egui::Stroke::NONE;
    widget_visuals.active.fg_stroke = egui::Stroke::new(1.0, accent_color);

    visuals
}
