// #####distribucion por tipo#####
// #####distribucion por tipo#####
// #####distribucion por tipo#####
//
// distribucion por tipo (especie) del data studio
//
// que hace este archivo:
//   - toma todos los resultados que se ven en el editor de resultados
//     (respeta los filtros del editor)
//   - cuenta cuantos resultados hay de cada etiqueta (ej: grus grus = 110)
//   - dibuja una dona (un circulo con hoyo) con un trozo por etiqueta
//   - debajo muestra una leyenda con el color, el nombre, la cantidad y el %
//   - al pasar el mouse sobre un trozo, el centro de la dona muestra sus datos
//
// colores:
//   - hay 7 colores fijos. se reparten segun que tan comun es cada etiqueta
//     en toda la carpeta (no solo en lo filtrado), asi una especie mantiene
//     su color aunque cambies los filtros
//   - si hay mas de 7 etiquetas, las menos comunes se juntan en "otros" (gris)

use super::editor_resultados::DatosGraficos;
use std::f32::consts::PI;

// maximo de etiquetas con color propio; el resto va a "otros"
const MAX_COLORES: usize = 7;

// colores para tema claro (en este orden: azul, naranjo, aqua, amarillo,
// rosado, verde, violeta). fueron revisados para que se distingan bien,
// incluso para personas daltonicas
const COLORES_TEMA_CLARO: [egui::Color32; MAX_COLORES] = [
    egui::Color32::from_rgb(0x2a, 0x78, 0xd6),
    egui::Color32::from_rgb(0xeb, 0x68, 0x34),
    egui::Color32::from_rgb(0x1b, 0xaf, 0x7a),
    egui::Color32::from_rgb(0xed, 0xa1, 0x00),
    egui::Color32::from_rgb(0xe8, 0x7b, 0xa4),
    egui::Color32::from_rgb(0x00, 0x83, 0x00),
    egui::Color32::from_rgb(0x62, 0x50, 0xd6),
];

// los mismos colores, ajustados para el tema oscuro
const COLORES_TEMA_OSCURO: [egui::Color32; MAX_COLORES] = [
    egui::Color32::from_rgb(0x39, 0x87, 0xe5),
    egui::Color32::from_rgb(0xd9, 0x59, 0x26),
    egui::Color32::from_rgb(0x19, 0x9e, 0x70),
    egui::Color32::from_rgb(0xc9, 0x85, 0x00),
    egui::Color32::from_rgb(0xd5, 0x51, 0x81),
    egui::Color32::from_rgb(0x00, 0x83, 0x00),
    egui::Color32::from_rgb(0x90, 0x85, 0xe9),
];

// color de "otros"
const GRIS_OTROS: egui::Color32 = egui::Color32::from_rgb(0x8a, 0x8a, 0x85);

// un trozo de la dona
struct Trozo {
    // nombre de la etiqueta (o "otros")
    nombre: String,
    // cuantos resultados tiene
    cantidad: usize,
    // color del trozo
    color: egui::Color32,
}

// arma la lista de trozos: cuenta por etiqueta, asigna colores y junta el
// resto en "otros". queda ordenada de mayor a menor cantidad
fn armar_trozos(datos: &DatosGraficos, colores: &[egui::Color32; MAX_COLORES]) -> Vec<Trozo> {
    // 1) contar cuantos resultados hay de cada etiqueta
    let mut conteo: Vec<(String, usize)> = Vec::new();
    for (etiqueta, _confianza) in &datos.resultados {
        let mut encontrada = false;
        for fila in &mut conteo {
            if fila.0 == *etiqueta {
                fila.1 += 1;
                encontrada = true;
                break;
            }
        }
        if !encontrada {
            conteo.push((etiqueta.clone(), 1));
        }
    }

    // 2) cada etiqueta recibe el color de su posicion en "orden_etiquetas"
    //    (las 7 mas comunes de la carpeta). si no esta entre esas 7, va a "otros"
    let mut trozos: Vec<Trozo> = Vec::new();
    let mut cantidad_otros = 0;
    for (etiqueta, cantidad) in conteo {
        let mut posicion: Option<usize> = None;
        for i in 0..datos.orden_etiquetas.len() {
            if datos.orden_etiquetas[i] == etiqueta {
                posicion = Some(i);
                break;
            }
        }
        match posicion {
            Some(p) if p < MAX_COLORES => {
                trozos.push(Trozo {
                    nombre: etiqueta,
                    cantidad,
                    color: colores[p],
                });
            }
            _ => {
                cantidad_otros += cantidad;
            }
        }
    }

    // 3) ordenar de mayor a menor
    trozos.sort_by(|a, b| b.cantidad.cmp(&a.cantidad));

    // 4) "otros" va siempre al final
    if cantidad_otros > 0 {
        trozos.push(Trozo {
            nombre: String::from("otros"),
            cantidad: cantidad_otros,
            color: GRIS_OTROS,
        });
    }

    trozos
}

// punto sobre un circulo. el angulo 0 es arriba y crece en sentido horario
fn punto_en_circulo(centro: egui::Pos2, radio: f32, angulo: f32) -> egui::Pos2 {
    egui::pos2(
        centro.x + radio * angulo.sin(),
        centro.y - radio * angulo.cos(),
    )
}

// dibuja el recuadro con la dona y la leyenda
pub(super) fn ui_distribucion(ui: &mut egui::Ui, datos: &DatosGraficos) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_width(ui.available_width());

        // titulo
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("Distribucion por tipo").strong());
        });
        ui.add_space(4.0);

        // si no hay resultados no hay nada que dibujar
        if datos.resultados.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.label(egui::RichText::new("Sin datos").weak());
                ui.add_space(80.0);
            });
            return;
        }

        // colores segun el tema (claro u oscuro)
        let colores = if ui.visuals().dark_mode { &COLORES_TEMA_OSCURO } else { &COLORES_TEMA_CLARO };
        let trozos = armar_trozos(datos, colores);
        let total = datos.resultados.len();

        // ----- espacio para la dona -----
        let diametro = ui.available_width().min(200.0);
        let (rectangulo, respuesta) = ui
            .vertical_centered(|ui| ui.allocate_exact_size(egui::vec2(diametro, diametro), egui::Sense::hover()))
            .inner;
        let pintor = ui.painter_at(rectangulo);
        let centro = rectangulo.center();
        let radio_exterior = diametro / 2.0 - 2.0;
        let radio_interior = radio_exterior * 0.55; // tamano del hoyo

        // ----- que trozo esta bajo el mouse (si hay alguno) -----
        let mut trozo_bajo_mouse: Option<usize> = None;
        if let Some(mouse) = respuesta.hover_pos() {
            let dx = mouse.x - centro.x;
            let dy = mouse.y - centro.y;
            let distancia = (dx * dx + dy * dy).sqrt();
            // solo cuenta si el mouse esta sobre el anillo (no en el hoyo ni afuera)
            if distancia >= radio_interior && distancia <= radio_exterior {
                // angulo del mouse: 0 arriba, crece en sentido horario
                let mut angulo_mouse = dx.atan2(-dy);
                if angulo_mouse < 0.0 {
                    angulo_mouse += 2.0 * PI;
                }
                // se busca en que trozo cae ese angulo
                let mut angulo_acumulado = 0.0;
                for i in 0..trozos.len() {
                    let tamano = trozos[i].cantidad as f32 / total as f32 * 2.0 * PI;
                    if angulo_mouse >= angulo_acumulado && angulo_mouse < angulo_acumulado + tamano {
                        trozo_bajo_mouse = Some(i);
                        break;
                    }
                    angulo_acumulado += tamano;
                }
            }
        }

        // ----- dibujar los trozos -----
        let mut angulo_inicio = 0.0;
        for i in 0..trozos.len() {
            let tamano = trozos[i].cantidad as f32 / total as f32 * 2.0 * PI;
            let angulo_fin = angulo_inicio + tamano;

            // el trozo bajo el mouse se dibuja un poco mas grande
            let radio_afuera = if trozo_bajo_mouse == Some(i) { radio_exterior + 2.0 } else { radio_exterior - 1.0 };

            // un arco no se puede dibujar directo, asi que se arma una "malla"
            // de triangulos: se ponen puntos en el borde de adentro y en el de
            // afuera (uno cada ~3 grados) y se unen de a dos triangulos
            let pasos = ((tamano / 0.05).ceil() as usize).max(1);
            let mut malla = egui::Mesh::default();
            for paso in 0..=pasos {
                let angulo = angulo_inicio + tamano * (paso as f32) / (pasos as f32);
                // punto de adentro y punto de afuera para este angulo
                malla.colored_vertex(punto_en_circulo(centro, radio_interior, angulo), trozos[i].color);
                malla.colored_vertex(punto_en_circulo(centro, radio_afuera, angulo), trozos[i].color);
            }
            for paso in 0..pasos {
                // posiciones de los 4 puntos de este pedacito en la malla
                let adentro_1 = (paso * 2) as u32;
                let afuera_1 = adentro_1 + 1;
                let adentro_2 = adentro_1 + 2;
                let afuera_2 = adentro_1 + 3;
                malla.add_triangle(adentro_1, afuera_1, afuera_2);
                malla.add_triangle(adentro_1, afuera_2, adentro_2);
            }
            pintor.add(egui::Shape::mesh(malla));

            angulo_inicio = angulo_fin;
        }

        // lineas del color del fondo entre trozos, para separarlos (solo si hay mas de uno)
        if trozos.len() > 1 {
            let color_fondo = ui.visuals().panel_fill;
            let mut angulo = 0.0;
            for trozo in &trozos {
                let p1 = punto_en_circulo(centro, radio_interior - 1.0, angulo);
                let p2 = punto_en_circulo(centro, radio_exterior + 3.0, angulo);
                pintor.line_segment([p1, p2], egui::Stroke::new(2.0, color_fondo));
                angulo += trozo.cantidad as f32 / total as f32 * 2.0 * PI;
            }
        }

        // ----- texto en el centro de la dona -----
        let color_texto = ui.visuals().text_color();
        let color_texto_suave = ui.visuals().weak_text_color();
        match trozo_bajo_mouse {
            Some(i) => {
                // datos del trozo bajo el mouse
                let porcentaje = trozos[i].cantidad as f32 / total as f32 * 100.0;
                pintor.text(
                    centro - egui::vec2(0.0, 9.0),
                    egui::Align2::CENTER_CENTER,
                    &trozos[i].nombre,
                    egui::FontId::proportional(13.0),
                    color_texto,
                );
                pintor.text(
                    centro + egui::vec2(0.0, 9.0),
                    egui::Align2::CENTER_CENTER,
                    format!("{} ({:.0}%)", trozos[i].cantidad, porcentaje),
                    egui::FontId::proportional(12.0),
                    color_texto_suave,
                );
            }
            None => {
                // total de resultados
                pintor.text(
                    centro - egui::vec2(0.0, 8.0),
                    egui::Align2::CENTER_CENTER,
                    format!("{}", total),
                    egui::FontId::proportional(18.0),
                    color_texto,
                );
                pintor.text(
                    centro + egui::vec2(0.0, 10.0),
                    egui::Align2::CENTER_CENTER,
                    "resultados",
                    egui::FontId::proportional(11.0),
                    color_texto_suave,
                );
            }
        }

        ui.add_space(8.0);

        // ----- leyenda: color, nombre, cantidad y porcentaje -----
        egui::Grid::new("leyenda_distribucion_tipo")
            .num_columns(3)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for trozo in &trozos {
                    let porcentaje = trozo.cantidad as f32 / total as f32 * 100.0;
                    ui.horizontal(|ui| {
                        // cuadrado de color
                        ui.label(egui::RichText::new("■").color(trozo.color));
                        ui.label(&trozo.nombre);
                    });
                    ui.label(format!("{}", trozo.cantidad));
                    ui.label(egui::RichText::new(format!("{:.0}%", porcentaje)).weak());
                    ui.end_row();
                }
            });
    });
    ui.add_space(8.0);
}

// #####
