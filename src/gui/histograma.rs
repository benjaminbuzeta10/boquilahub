// #####histograma de confianza#####
// #####histograma de confianza#####
// #####histograma de confianza#####
//
// histograma de confianza del data studio
//
// que hace este archivo:
//   - toma todos los resultados que se ven en el editor de resultados
//     (respeta los filtros del editor)
//   - los reparte en 10 grupos segun su confianza:
//       0-10%, 10-20%, 20-30%, ... , 90-100%
//   - dibuja una barra por grupo con la cantidad de resultados
//   - al pasar el mouse sobre una barra muestra el rango y la cantidad
//
// usa egui_plot, la misma libreria que boquilahub usa para el audio

use super::editor_resultados::DatosGraficos;
use egui_plot::{Bar, BarChart, Plot};

// cantidad de grupos (barras) del histograma
const CANTIDAD_GRUPOS: usize = 10;
// ancho de cada grupo en porcentaje (100 / 10 = 10)
const ANCHO_GRUPO: f64 = 10.0;

// color de las barras (azul). hay uno para tema claro y otro para tema oscuro
const AZUL_CLARO: egui::Color32 = egui::Color32::from_rgb(0x2a, 0x78, 0xd6);
const AZUL_OSCURO: egui::Color32 = egui::Color32::from_rgb(0x39, 0x87, 0xe5);

// cuenta cuantos resultados caen en cada grupo de confianza.
// devuelve una lista de 10 numeros: el primero es 0-10%, el ultimo 90-100%
fn contar_por_grupo(datos: &DatosGraficos) -> Vec<usize> {
    let mut cantidades = vec![0; CANTIDAD_GRUPOS];

    for (_etiqueta, confianza) in &datos.resultados {
        // en que grupo cae: 0-9.99 -> 0, 10-19.99 -> 1, ... , 90-100 -> 9
        let mut grupo = (*confianza / ANCHO_GRUPO as f32) as usize;
        // el 100% exacto cae en el ultimo grupo (y no en uno nuevo)
        if grupo >= CANTIDAD_GRUPOS {
            grupo = CANTIDAD_GRUPOS - 1;
        }
        cantidades[grupo] += 1;
    }

    cantidades
}

// dibuja el recuadro con el histograma
pub(super) fn ui_histograma(ui: &mut egui::Ui, datos: &DatosGraficos) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_width(ui.available_width());

        // titulo
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("Histograma de confianza").strong());
        });

        // si no hay resultados no hay nada que dibujar
        if datos.resultados.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(egui::RichText::new("Sin datos").weak());
                ui.add_space(60.0);
            });
            return;
        }

        // se elige el azul segun el tema (claro u oscuro)
        let color = if ui.visuals().dark_mode { AZUL_OSCURO } else { AZUL_CLARO };

        // se arma una barra por cada grupo
        let cantidades = contar_por_grupo(datos);
        let mut barras = Vec::new();
        for grupo in 0..CANTIDAD_GRUPOS {
            let desde = grupo as f64 * ANCHO_GRUPO; // ej: 30
            let hasta = desde + ANCHO_GRUPO; // ej: 40
            let centro = desde + ANCHO_GRUPO / 2.0; // ej: 35 (la barra se dibuja centrada aqui)

            let barra = Bar::new(centro, cantidades[grupo] as f64)
                // relleno solido (si no, egui_plot lo dibuja semi transparente)
                .fill(color)
                // un poco mas angosta que el grupo, para que quede un espacio entre barras
                .width(ANCHO_GRUPO * 0.9)
                .name(format!("{:.0}-{:.0}%", desde, hasta));
            barras.push(barra);
        }

        // texto que aparece al pasar el mouse sobre una barra
        let grafico = BarChart::new("resultados", barras)
            .color(color)
            .element_formatter(Box::new(|barra, _grafico| {
                let desde = barra.argument - ANCHO_GRUPO / 2.0;
                let hasta = barra.argument + ANCHO_GRUPO / 2.0;
                format!("confianza {:.0}-{:.0}%\n{} resultados", desde, hasta, barra.value)
            }));

        // el grafico no se puede mover ni hacer zoom (asi siempre se ve completo)
        Plot::new("histograma_confianza")
            .height(160.0)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .include_x(0.0)
            .include_x(100.0)
            .include_y(0.0)
            .x_axis_label("confianza (%)")
            .show(ui, |plot_ui| {
                plot_ui.bar_chart(grafico);
            });

        // total debajo del grafico
        ui.label(
            egui::RichText::new(format!("{} resultados en total", datos.resultados.len())).weak(),
        );
    });
    ui.add_space(8.0);
}

// #####
