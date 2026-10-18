


// ==================== DATA STUDIO (inicio) ===============================================================
//! Data Studio: módulo de revisión y análisis de predicciones.
//!
//! Por ahora es solo la pantalla (esqueleto según el mockup v3): no carga ni
//! procesa nada. Los botones de acción están deshabilitados y los gráficos
//! muestran un marcador "sin datos".

use super::Gui;
// #####editor de resultados#####
// #####editor de resultados#####
// #####editor de resultados#####
use super::editor_resultados;
// #####
// #####histograma de confianza#####
// #####histograma de confianza#####
// #####histograma de confianza#####
use super::histograma;
// #####
// #####distribucion por tipo#####
// #####distribucion por tipo#####
// #####distribucion por tipo#####
use super::distribucion_tipo;
// #####
use crate::localization::Lang;

/// Texto en español o inglés según el idioma elegido. Data Studio todavía no
/// está en `localization.rs`; cualquier idioma que no sea español usa inglés.
fn tr(lang: &Lang, es: &'static str, en: &'static str) -> &'static str {
    if *lang == Lang::ES { es } else { en }
}

impl Gui {
    pub(super) fn ui_data_studio(&mut self, ui: &mut egui::Ui) {
        // #####editor de resultados#####
        // #####editor de resultados#####
        // #####editor de resultados#####
        // datos de la ia elegida en la pantalla principal, para el boton
        // "analizar" del editor de resultados
        let nombre_ia = match self.ai_selected {
            Some(i) => self.ais[i].name.clone(),
            None => String::new(),
        };
        let info_ia = editor_resultados::InfoModelo {
            hay_modelo: self.ai_selected.is_some(),
            nombre: nombre_ia,
            es_audio: self.is_audio_model(),
            es_local: self.ep_selected.is_local(),
            ocupado: self.is_any_processing(),
            paso_video: self.video_step_frame,
        };
        // datos para los graficos (salen de lo que se ve en el editor)
        let datos_graficos = self.editor_resultados.datos_para_graficos();
        // #####
        let lang = &self.lang;

        ui.horizontal(|ui| {
            if ui.button(tr(lang, "⬅ Volver", "⬅ Back")).clicked() {
                self.data_studio = false;
            }
            ui.separator();
            ui.heading("📊 Data Studio");
            // #####editor de resultados#####
            // #####editor de resultados#####
            // #####editor de resultados#####
            // nombre de la carpeta abierta en el editor, como en el mockup
            if let Some(carpeta) = &self.editor_resultados.carpeta {
                if let Some(nombre) = carpeta.file_name() {
                    ui.heading(format!(": {}", nombre.to_string_lossy()));
                }
            }
            // #####
        });
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.columns(3, |cols| {
                // #####editor de resultados#####
                // #####editor de resultados#####
                // #####editor de resultados#####
                // columna 1: editor de resultados (el codigo esta en editor_resultados.rs)
                self.editor_resultados.ui(&mut cols[0], &info_ia);
                // #####

                // Columna 2: histograma y líneas de tiempo
                // #####histograma de confianza#####
                // #####histograma de confianza#####
                // #####histograma de confianza#####
                histograma::ui_histograma(&mut cols[1], &datos_graficos);
                // #####
                placeholder_box(&mut cols[1], tr(lang, "Población a lo largo del tiempo", "Population over time"), 160.0, lang, |_| {});
                placeholder_box(&mut cols[1], tr(lang, "Predicciones en audio a lo largo del tiempo", "Audio predictions over time"), 160.0, lang, |_| {});

                // Columna 3: distribuciones
                // #####distribucion por tipo#####
                // #####distribucion por tipo#####
                // #####distribucion por tipo#####
                distribucion_tipo::ui_distribucion(&mut cols[2], &datos_graficos);
                // #####
                placeholder_box(&mut cols[2], tr(lang, "Distribución día y noche", "Day / night distribution"), 250.0, lang, |_| {});
            });
        });
    }
}

/// Recuadro con título y un texto "sin datos" centrado. `extra` permite
/// agregar contenido arriba del marcador (p. ej. la barra de botones).
fn placeholder_box(
    ui: &mut egui::Ui,
    title: &str,
    min_height: f32,
    lang: &Lang,
    extra: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.set_min_height(min_height);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(title).strong());
        });
        ui.add_space(4.0);
        extra(ui);
        ui.vertical_centered(|ui| {
            ui.add_space(min_height / 3.0);
            ui.label(egui::RichText::new(tr(lang, "Sin datos", "No data")).weak());
        });
    });
    ui.add_space(8.0);
}
// ==================== DATA STUDIO (fin) =================================================================
