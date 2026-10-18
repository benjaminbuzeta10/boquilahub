// #####editor de resultados#####
// #####editor de resultados#####
// #####editor de resultados#####
//
// editor de resultados del data studio
//
// que hace este archivo:
//   - abre una carpeta y lista las imagenes, audios y videos que tiene
//   - para cada archivo lee sus predicciones desde el json que boquilahub
//     guarda al lado del archivo (ej: foto.jpg -> foto_predictions.json)
//       * imagenes y audios: el json tiene directamente las predicciones
//       * videos: el json tiene una lista de cuadros (frames) y cada cuadro
//         analizado tiene sus propias predicciones
//   - muestra cada archivo con un borde de color segun la confianza:
//       verde   = confianza alta (75% o mas)
//       naranjo = confianza media (entre 50% y 75%)
//       rojo    = confianza baja (menos de 50%)
//       gris    = el archivo todavia no tiene predicciones
//   - boton "modificar": permite cambiar la etiqueta y la confianza de cada
//     resultado, o borrar un resultado, y guardar los cambios en el json
//   - boton "analizar": corre la ia elegida en la pantalla principal sobre
//     los archivos (los seleccionados, o todos los que se ven si no hay
//     ninguno seleccionado) y guarda el json de cada uno al terminar.
//     es como apretar "analizar" y despues "exportar predicciones" en la
//     pantalla principal
//   - boton "seleccionar": muestra casillas para elegir varios archivos
//   - boton "eliminar": borra el json de predicciones de los archivos
//     seleccionados (la imagen, audio o video original nunca se borra)
//   - boton "filtros": filtra por etiqueta, formato y confianza, y ordena
//
// el codigo esta escrito de forma simple a proposito (structs, for y if),
// aunque quede mas largo, para que sea facil de leer y modificar

use crate::api::abstractions::{sidecar_predictions_path, AIOutputs, PredVideo};
use crate::api::audio::AudioData;
use crate::api::bq::{process_audio, process_imgbuf};
use crate::api::formats;
use crate::api::video_file::VideofileProcessor;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// constantes
// ---------------------------------------------------------------------------

// desde este valor hacia arriba el borde es verde (en porcentaje)
const CONFIANZA_ALTA: f32 = 75.0;
// desde este valor hacia arriba (y bajo la alta) el borde es naranjo
const CONFIANZA_MEDIA: f32 = 50.0;
// maximo de resultados que se muestran al modificar un archivo.
// los videos pueden tener miles (uno o mas por cada cuadro analizado) y
// dibujarlos todos haria lenta la pantalla
const MAX_RESULTADOS_EN_PANTALLA: usize = 100;

// ---------------------------------------------------------------------------
// tipos de datos
// ---------------------------------------------------------------------------

// tipo de archivo
#[derive(Clone, Copy, PartialEq)]
pub(super) enum TipoArchivo {
    Imagen,
    Audio,
    Video,
}

// predicciones leidas del json. las imagenes y audios usan el formato
// simple (AIOutputs); los videos usan PredVideo, que guarda un AIOutputs
// por cada cuadro analizado
pub(super) enum Predicciones {
    Simple(AIOutputs),
    Video(PredVideo),
}

// un resultado (una prediccion) que se puede editar en pantalla
#[derive(Clone)]
pub(super) struct ResultadoEditable {
    // nombre de la clase, por ejemplo "grus grus"
    etiqueta: String,
    // numero de la clase que usa el modelo
    id_clase: u32,
    // confianza en porcentaje, de 0 a 100
    confianza: f32,
    // inicio y fin en segundos (audios: rango del trozo, videos: momento del cuadro)
    inicio: Option<f32>,
    fin: Option<f32>,
    // numero de cuadro del video al que pertenece (none si no es video)
    cuadro: Option<usize>,
    // si es true, este resultado se borra al guardar
    borrar: bool,
}

// un archivo (imagen, audio o video) de la carpeta abierta
pub(super) struct ArchivoEditor {
    // ruta completa del archivo
    ruta: PathBuf,
    // solo el nombre, por ejemplo "imagen1.png"
    nombre: String,
    // extension en minusculas, por ejemplo "png"
    formato: String,
    // imagen, audio o video
    tipo: TipoArchivo,
    // lo que se leyo del json de predicciones (none si no tiene json)
    predicciones: Option<Predicciones>,
    // resultados ya guardados (lo que se muestra en la lista)
    resultados: Vec<ResultadoEditable>,
    // copia de los resultados mientras se estan modificando
    borrador: Vec<ResultadoEditable>,
    // true si el archivo esta marcado con la casilla de seleccion
    seleccionado: bool,
    // true si el panel "modificar" de este archivo esta abierto
    abierto: bool,
}

// formas de ordenar la lista
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Orden {
    Nombre,
    ConfianzaMenorPrimero,
    ConfianzaMayorPrimero,
}

// datos de la ia elegida en la pantalla principal. data_studio.rs los llena
// cada vez que dibuja el editor, porque el editor no puede ver la pantalla
// principal directamente
pub(super) struct InfoModelo {
    // true si hay una ia elegida
    pub(super) hay_modelo: bool,
    // nombre de la ia elegida (vacio si no hay)
    pub(super) nombre: String,
    // true si la ia es de audio, false si es de imagen
    pub(super) es_audio: bool,
    // true si la ia corre en este computador (no en un boquilahub remoto)
    pub(super) es_local: bool,
    // true si la pantalla principal esta analizando algo en este momento
    pub(super) ocupado: bool,
    // cada cuantos cuadros se analiza un video (igual que en la pantalla principal)
    pub(super) paso_video: usize,
}

// aviso que manda el hilo de analisis cuando termina un archivo
struct AvisoAnalisis {
    // posicion del archivo en la lista "archivos"
    indice: usize,
    // true si se analizo y se guardo bien
    ok: bool,
}

// todo lo que se necesita mientras un analisis esta corriendo
struct AnalisisEnCurso {
    // por aqui llegan los avisos del hilo de analisis
    receptor: mpsc::Receiver<AvisoAnalisis>,
    // si se pone en true, el hilo se detiene en cuanto pueda
    cancelar: Arc<AtomicBool>,
    // cuantos archivos hay que analizar en total
    total: usize,
    // cuantos ya terminaron (bien o mal)
    terminados: usize,
    // cuantos terminaron con error
    errores: usize,
    // nombre del archivo que se esta analizando ahora
    analizando: String,
}

// datos que el editor le pasa a los graficos (histograma y distribucion por tipo)
pub(super) struct DatosGraficos {
    // un elemento por resultado visible: (etiqueta, confianza en porcentaje)
    pub(super) resultados: Vec<(String, f32)>,
    // todas las etiquetas de la carpeta, de la mas comun a la menos comun.
    // los graficos la usan para que cada etiqueta tenga siempre el mismo color
    pub(super) orden_etiquetas: Vec<String>,
}

// todo el estado del editor de resultados
pub(super) struct EditorResultados {
    // carpeta abierta (none si todavia no se abre ninguna)
    pub(super) carpeta: Option<PathBuf>,
    // archivos de la carpeta
    archivos: Vec<ArchivoEditor>,
    // true cuando se muestran las casillas de seleccion
    modo_seleccion: bool,
    // true cuando se muestra el panel de filtros
    mostrar_filtros: bool,
    // true cuando se muestra la ventana "seguro que quieres eliminar?"
    confirmar_eliminar: bool,
    // archivos que se quieren analizar y que ya tenian predicciones. si no
    // esta vacio se muestra la ventana "reemplazar o solo los que faltan?"
    pendientes_de_confirmar: Vec<usize>,
    // analisis que esta corriendo (none si no hay ninguno)
    analisis: Option<AnalisisEnCurso>,
    // mensaje corto para el usuario (ej: "cambios guardados")
    mensaje: String,

    // ----- filtros -----
    // etiqueta elegida en el filtro ("" significa todas)
    filtro_etiqueta: String,
    // formato elegido en el filtro ("" significa todos)
    filtro_formato: String,
    // confianza minima y maxima en porcentaje
    filtro_confianza_min: f32,
    filtro_confianza_max: f32,
    // si es true solo se muestran archivos que ya tienen predicciones
    filtro_solo_con_predicciones: bool,
    // orden de la lista
    orden: Orden,
}

// valores iniciales del editor (se usa cuando se crea la app)
impl Default for EditorResultados {
    fn default() -> Self {
        EditorResultados {
            carpeta: None,
            archivos: Vec::new(),
            modo_seleccion: false,
            mostrar_filtros: false,
            confirmar_eliminar: false,
            pendientes_de_confirmar: Vec::new(),
            analisis: None,
            mensaje: String::new(),
            filtro_etiqueta: String::new(),
            filtro_formato: String::new(),
            filtro_confianza_min: 0.0,
            filtro_confianza_max: 100.0,
            filtro_solo_con_predicciones: false,
            orden: Orden::Nombre,
        }
    }
}

// ---------------------------------------------------------------------------
// funciones de ayuda para leer y guardar predicciones (no dibujan nada)
// ---------------------------------------------------------------------------

// revisa si una extension esta en una lista de formatos (sin importar mayusculas)
fn extension_esta_en_lista(extension: &str, lista: &[&str]) -> bool {
    for formato in lista {
        if extension.eq_ignore_ascii_case(formato) {
            return true;
        }
    }
    false
}

// dice si un archivo es imagen, audio o video segun su extension.
// devuelve none si no es ninguno de los tres
fn tipo_de_archivo(ruta: &PathBuf) -> Option<TipoArchivo> {
    let extension = match ruta.extension() {
        Some(e) => e.to_string_lossy().to_string(),
        None => return None,
    };
    if extension_esta_en_lista(&extension, &formats::IMAGE_FORMATS) {
        return Some(TipoArchivo::Imagen);
    }
    if extension_esta_en_lista(&extension, &formats::AUDIO_FORMATS) {
        return Some(TipoArchivo::Audio);
    }
    if extension_esta_en_lista(&extension, &formats::VIDEO_FORMATS) {
        return Some(TipoArchivo::Video);
    }
    None
}

// lee el json de predicciones de un archivo, si existe
fn leer_predicciones(ruta: &PathBuf, tipo: TipoArchivo) -> Option<Predicciones> {
    // ruta del json, ej: carpeta/foto.jpg -> carpeta/foto_predictions.json
    let ruta_json = match sidecar_predictions_path(ruta) {
        Ok(r) => r,
        Err(_) => return None,
    };

    // si no existe el json, el archivo no tiene predicciones
    if !ruta_json.exists() {
        return None;
    }

    // se lee el texto del json
    let texto = match fs::read_to_string(&ruta_json) {
        Ok(t) => t,
        Err(_) => return None,
    };

    // los videos usan otro formato que las imagenes y audios
    if tipo == TipoArchivo::Video {
        match serde_json::from_str::<PredVideo>(&texto) {
            Ok(video) => Some(Predicciones::Video(video)),
            Err(_) => None,
        }
    } else {
        match serde_json::from_str::<AIOutputs>(&texto) {
            Ok(salida) => Some(Predicciones::Simple(salida)),
            Err(_) => None,
        }
    }
}

// convierte las predicciones de un archivo a una lista simple de resultados
fn resultados_desde_salida(salida: &AIOutputs) -> Vec<ResultadoEditable> {
    let mut lista = Vec::new();

    match salida {
        // deteccion de objetos (cajas)
        AIOutputs::ObjectDetection(cajas) => {
            for caja in cajas {
                lista.push(ResultadoEditable {
                    etiqueta: caja.label.clone(),
                    id_clase: caja.xyxy.class_id,
                    confianza: caja.xyxy.prob * 100.0,
                    inicio: None,
                    fin: None,
                    cuadro: None,
                    borrar: false,
                });
            }
        }
        // deteccion de puntos
        AIOutputs::PointDetection(puntos) => {
            for punto in puntos {
                lista.push(ResultadoEditable {
                    etiqueta: punto.label.clone(),
                    id_clase: punto.xy.class_id,
                    confianza: punto.xy.prob * 100.0,
                    inicio: None,
                    fin: None,
                    cuadro: None,
                    borrar: false,
                });
            }
        }
        // clasificacion de imagen
        AIOutputs::Classification(probabilidades) => {
            for p in probabilidades {
                lista.push(ResultadoEditable {
                    etiqueta: p.label.clone(),
                    id_clase: p.class_id,
                    confianza: p.prob * 100.0,
                    inicio: None,
                    fin: None,
                    cuadro: None,
                    borrar: false,
                });
            }
        }
        // segmentacion (se edita la caja que acompana a cada mascara)
        AIOutputs::Segmentation(segmentos) => {
            for s in segmentos {
                lista.push(ResultadoEditable {
                    etiqueta: s.bbox.label.clone(),
                    id_clase: s.bbox.xyxy.class_id,
                    confianza: s.bbox.xyxy.prob * 100.0,
                    inicio: None,
                    fin: None,
                    cuadro: None,
                    borrar: false,
                });
            }
        }
        // clasificacion de audio (tiene inicio y fin en segundos)
        AIOutputs::AudioClassification(audios) => {
            for a in audios {
                lista.push(ResultadoEditable {
                    etiqueta: a.prediction.label.clone(),
                    id_clase: a.prediction.class_id,
                    confianza: a.prediction.prob * 100.0,
                    inicio: Some(a.start),
                    fin: Some(a.end),
                    cuadro: None,
                    borrar: false,
                });
            }
        }
        // los embeddings no tienen etiquetas ni confianza, no se muestran
        AIOutputs::Embed(_) => {}
    }

    lista
}

// igual que la funcion anterior, pero sirve para imagenes, audios y videos.
// en los videos se juntan los resultados de todos los cuadros analizados,
// y cada resultado recuerda de que cuadro vino
fn resultados_desde_predicciones(predicciones: &Predicciones) -> Vec<ResultadoEditable> {
    match predicciones {
        Predicciones::Simple(salida) => resultados_desde_salida(salida),
        Predicciones::Video(video) => {
            let mut lista = Vec::new();
            for numero_cuadro in 0..video.frames.len() {
                // los cuadros que no se analizaron son none y se saltan
                if let Some(salida) = &video.frames[numero_cuadro] {
                    // momento del cuadro en segundos
                    let segundos = if video.fps > 0.0 {
                        Some((numero_cuadro as f64 / video.fps) as f32)
                    } else {
                        None
                    };
                    let resultados_cuadro = resultados_desde_salida(salida);
                    for mut r in resultados_cuadro {
                        r.cuadro = Some(numero_cuadro);
                        r.inicio = segundos;
                        r.fin = segundos;
                        lista.push(r);
                    }
                }
            }
            lista
        }
    }
}

// copia los cambios del borrador de vuelta a un AIOutputs.
// el borrador tiene el mismo orden que la lista original, por eso se usa
// el mismo indice "i" para los dos
fn aplicar_cambios(salida: &mut AIOutputs, borrador: &Vec<ResultadoEditable>) {
    match salida {
        AIOutputs::ObjectDetection(cajas) => {
            let mut nuevas = Vec::new();
            for i in 0..cajas.len() {
                if i < borrador.len() && borrador[i].borrar {
                    continue; // este resultado se borra
                }
                let mut caja = cajas[i].clone();
                if i < borrador.len() {
                    caja.label = borrador[i].etiqueta.clone();
                    caja.xyxy.class_id = borrador[i].id_clase;
                    caja.xyxy.prob = borrador[i].confianza / 100.0;
                }
                nuevas.push(caja);
            }
            *cajas = nuevas;
        }
        AIOutputs::PointDetection(puntos) => {
            let mut nuevos = Vec::new();
            for i in 0..puntos.len() {
                if i < borrador.len() && borrador[i].borrar {
                    continue;
                }
                let mut punto = puntos[i].clone();
                if i < borrador.len() {
                    punto.label = borrador[i].etiqueta.clone();
                    punto.xy.class_id = borrador[i].id_clase;
                    punto.xy.prob = borrador[i].confianza / 100.0;
                }
                nuevos.push(punto);
            }
            *puntos = nuevos;
        }
        AIOutputs::Classification(probabilidades) => {
            let mut nuevas = Vec::new();
            for i in 0..probabilidades.len() {
                if i < borrador.len() && borrador[i].borrar {
                    continue;
                }
                let mut p = probabilidades[i].clone();
                if i < borrador.len() {
                    p.label = borrador[i].etiqueta.clone();
                    p.class_id = borrador[i].id_clase;
                    p.prob = borrador[i].confianza / 100.0;
                }
                nuevas.push(p);
            }
            *probabilidades = nuevas;
        }
        AIOutputs::Segmentation(segmentos) => {
            let mut nuevos = Vec::new();
            for i in 0..segmentos.len() {
                if i < borrador.len() && borrador[i].borrar {
                    continue;
                }
                let mut s = segmentos[i].clone();
                if i < borrador.len() {
                    s.bbox.label = borrador[i].etiqueta.clone();
                    s.bbox.xyxy.class_id = borrador[i].id_clase;
                    s.bbox.xyxy.prob = borrador[i].confianza / 100.0;
                }
                nuevos.push(s);
            }
            *segmentos = nuevos;
        }
        AIOutputs::AudioClassification(audios) => {
            let mut nuevos = Vec::new();
            for i in 0..audios.len() {
                if i < borrador.len() && borrador[i].borrar {
                    continue;
                }
                let mut a = audios[i].clone();
                if i < borrador.len() {
                    a.prediction.label = borrador[i].etiqueta.clone();
                    a.prediction.class_id = borrador[i].id_clase;
                    a.prediction.prob = borrador[i].confianza / 100.0;
                }
                nuevos.push(a);
            }
            *audios = nuevos;
        }
        AIOutputs::Embed(_) => {}
    }
}

// igual que la funcion anterior, pero sirve para imagenes, audios y videos.
// en los videos, cada cuadro recibe solo los resultados que vienen de el
fn aplicar_cambios_predicciones(predicciones: &mut Predicciones, borrador: &Vec<ResultadoEditable>) {
    match predicciones {
        Predicciones::Simple(salida) => aplicar_cambios(salida, borrador),
        Predicciones::Video(video) => {
            for numero_cuadro in 0..video.frames.len() {
                if let Some(salida) = &mut video.frames[numero_cuadro] {
                    // resultados del borrador que son de este cuadro (en orden)
                    let mut del_cuadro = Vec::new();
                    for r in borrador {
                        if r.cuadro == Some(numero_cuadro) {
                            del_cuadro.push(r.clone());
                        }
                    }
                    if !del_cuadro.is_empty() {
                        aplicar_cambios(salida, &del_cuadro);
                    }
                }
            }
        }
    }
}

// escribe un texto json al lado del archivo original. devuelve true si funciono
fn escribir_json(ruta: &PathBuf, texto: String) -> bool {
    let ruta_json = match sidecar_predictions_path(ruta) {
        Ok(r) => r,
        Err(_) => return false,
    };
    match fs::write(&ruta_json, texto) {
        Ok(_) => true,
        Err(_) => false,
    }
}

// guarda las predicciones de un archivo en su json. devuelve true si funciono
fn guardar_predicciones(ruta: &PathBuf, predicciones: &Predicciones) -> bool {
    // se convierte a texto json con el mismo formato que usa boquilahub
    let texto = match predicciones {
        Predicciones::Simple(salida) => serde_json::to_string(salida),
        Predicciones::Video(video) => serde_json::to_string(video),
    };
    match texto {
        Ok(t) => escribir_json(ruta, t),
        Err(_) => false,
    }
}

// la confianza mas alta de un archivo (none si no tiene resultados)
fn confianza_maxima(archivo: &ArchivoEditor) -> Option<f32> {
    let mut maxima: Option<f32> = None;
    for r in &archivo.resultados {
        match maxima {
            None => maxima = Some(r.confianza),
            Some(m) => {
                if r.confianza > m {
                    maxima = Some(r.confianza);
                }
            }
        }
    }
    maxima
}

// color del borde segun la confianza
fn color_por_confianza(confianza: Option<f32>) -> egui::Color32 {
    match confianza {
        None => egui::Color32::GRAY,
        Some(c) => {
            if c >= CONFIANZA_ALTA {
                egui::Color32::from_rgb(120, 200, 80) // verde
            } else if c >= CONFIANZA_MEDIA {
                egui::Color32::from_rgb(240, 150, 60) // naranjo
            } else {
                egui::Color32::from_rgb(220, 70, 60) // rojo
            }
        }
    }
}

// crea un archivo del editor a partir de su ruta (y lee su json si existe)
fn nuevo_archivo(ruta: PathBuf, tipo: TipoArchivo) -> ArchivoEditor {
    // nombre del archivo, ej: "imagen1.png"
    let nombre = match ruta.file_name() {
        Some(n) => n.to_string_lossy().to_string(),
        None => String::from("(sin nombre)"),
    };

    // extension en minusculas, ej: "png"
    let formato = match ruta.extension() {
        Some(e) => e.to_string_lossy().to_lowercase(),
        None => String::new(),
    };

    // predicciones guardadas por boquilahub
    let predicciones = leer_predicciones(&ruta, tipo);
    let resultados = match &predicciones {
        Some(p) => resultados_desde_predicciones(p),
        None => Vec::new(),
    };

    ArchivoEditor {
        ruta,
        nombre,
        formato,
        tipo,
        predicciones,
        resultados,
        borrador: Vec::new(),
        seleccionado: false,
        abierto: false,
    }
}

// ---------------------------------------------------------------------------
// analisis con la ia (corre en un hilo aparte para no congelar la pantalla)
// ---------------------------------------------------------------------------

// analiza una imagen y guarda su json. devuelve true si funciono
fn analizar_imagen(ruta: &PathBuf) -> bool {
    // se abre la imagen y se pasa a rgb
    let imagen = match image::open(ruta) {
        Ok(i) => i.into_rgb8(),
        Err(_) => return false,
    };

    // se corre la ia (la misma que usa "analizar" en la pantalla principal)
    let salida = match process_imgbuf(&imagen) {
        Ok(s) => s,
        Err(_) => return false,
    };

    // se guarda el json (igual que "exportar predicciones")
    match serde_json::to_string(&salida) {
        Ok(texto) => escribir_json(ruta, texto),
        Err(_) => false,
    }
}

// analiza un audio y guarda su json. devuelve true si funciono
fn analizar_audio(ruta: &PathBuf) -> bool {
    // se abre el audio y se pasa a un solo canal (mono)
    let audio = match AudioData::from_file(ruta) {
        Ok(a) => a.to_mono(),
        Err(_) => return false,
    };

    // se corre la ia
    let salida = match process_audio(&audio) {
        Ok(s) => s,
        Err(_) => return false,
    };

    // se guarda el json
    match serde_json::to_string(&salida) {
        Ok(texto) => escribir_json(ruta, texto),
        Err(_) => false,
    }
}

// analiza un video cuadro por cuadro y guarda su json. devuelve true si funciono.
// solo se analiza 1 de cada "paso" cuadros, igual que en la pantalla principal
fn analizar_video(ruta: &PathBuf, paso: usize, cancelar: &Arc<AtomicBool>) -> bool {
    // ffmpeg necesita la ruta como texto
    let ruta_texto = match ruta.to_str() {
        Some(t) => t.to_string(),
        None => return false,
    };

    // primero se leen los datos del video (tamano, cuadros por segundo, etc)
    let datos = match VideofileProcessor::probe(&ruta_texto) {
        Ok(d) => d,
        Err(_) => return false,
    };

    // se arma el resultado vacio: un espacio por cada cuadro del video
    let paso = if paso == 0 { 1 } else { paso };
    let mut video = PredVideo {
        file_path: ruta.clone(),
        width: datos.width,
        height: datos.height,
        fps: datos.fps,
        n_frames: datos.n_frames,
        step: paso as u32,
        frames: vec![None; datos.n_frames as usize],
        wasprocessed: false,
    };

    // se recorren los cuadros del video uno por uno
    let procesador = VideofileProcessor::new(&ruta_texto);
    for (numero_cuadro, imagen) in procesador {
        // si el usuario apreto cancelar, se deja el video sin guardar
        if cancelar.load(Ordering::Relaxed) {
            return false;
        }
        // solo 1 de cada "paso" cuadros
        if numero_cuadro % (paso as u64) != 0 {
            continue;
        }
        // se corre la ia sobre el cuadro y se anota el resultado
        match process_imgbuf(&imagen) {
            Ok(salida) => video.record(numero_cuadro, salida),
            Err(_) => return false,
        }
    }

    // se marca como terminado y se guarda el json
    video.wasprocessed = true;
    match serde_json::to_string(&video) {
        Ok(texto) => escribir_json(ruta, texto),
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// funciones del editor que cambian su estado
// ---------------------------------------------------------------------------

impl EditorResultados {
    // abre una carpeta y carga todas sus imagenes, audios y videos
    fn cargar_carpeta(&mut self, carpeta: PathBuf) {
        let mut archivos = Vec::new();

        // se leen las entradas de la carpeta
        let entradas = match fs::read_dir(&carpeta) {
            Ok(e) => e,
            Err(_) => {
                self.mensaje = String::from("no se pudo abrir la carpeta");
                return;
            }
        };

        for entrada in entradas {
            // si una entrada falla se salta
            let entrada = match entrada {
                Ok(e) => e,
                Err(_) => continue,
            };
            let ruta = entrada.path();

            // solo archivos (no subcarpetas)
            if !ruta.is_file() {
                continue;
            }

            // solo imagenes, audios y videos
            let tipo = match tipo_de_archivo(&ruta) {
                Some(t) => t,
                None => continue,
            };

            archivos.push(nuevo_archivo(ruta, tipo));
        }

        // orden alfabetico por nombre para que siempre salga igual
        archivos.sort_by(|a, b| a.nombre.cmp(&b.nombre));

        // se guarda todo en el editor y se limpian los filtros
        self.mensaje = format!("{} archivos cargados", archivos.len());
        self.archivos = archivos;
        self.carpeta = Some(carpeta);
        self.filtro_etiqueta = String::new();
        self.filtro_formato = String::new();
        self.modo_seleccion = false;
    }

    // vuelve a leer el json de un archivo (se usa despues de analizarlo)
    fn recargar_archivo(&mut self, indice: usize) {
        if indice >= self.archivos.len() {
            return;
        }
        let ruta = self.archivos[indice].ruta.clone();
        let tipo = self.archivos[indice].tipo;
        let seleccionado = self.archivos[indice].seleccionado;
        let mut nuevo = nuevo_archivo(ruta, tipo);
        nuevo.seleccionado = seleccionado; // se mantiene la seleccion
        self.archivos[indice] = nuevo;
    }

    // lista de etiquetas distintas que aparecen en la carpeta (con su id de clase)
    fn etiquetas_conocidas(&self) -> Vec<(String, u32)> {
        let mut etiquetas: Vec<(String, u32)> = Vec::new();
        for archivo in &self.archivos {
            for r in &archivo.resultados {
                // se revisa si la etiqueta ya esta en la lista
                let mut ya_esta = false;
                for (e, _) in &etiquetas {
                    if *e == r.etiqueta {
                        ya_esta = true;
                        break;
                    }
                }
                if !ya_esta {
                    etiquetas.push((r.etiqueta.clone(), r.id_clase));
                }
            }
        }
        etiquetas.sort_by(|a, b| a.0.cmp(&b.0));
        etiquetas
    }

    // lista de formatos distintos que aparecen en la carpeta
    fn formatos_conocidos(&self) -> Vec<String> {
        let mut lista: Vec<String> = Vec::new();
        for archivo in &self.archivos {
            if !lista.contains(&archivo.formato) {
                lista.push(archivo.formato.clone());
            }
        }
        lista.sort();
        lista
    }

    // revisa si un archivo pasa todos los filtros activos
    fn pasa_filtros(&self, archivo: &ArchivoEditor) -> bool {
        // filtro: solo archivos con predicciones
        if self.filtro_solo_con_predicciones && archivo.resultados.is_empty() {
            return false;
        }

        // filtro: formato
        if !self.filtro_formato.is_empty() && archivo.formato != self.filtro_formato {
            return false;
        }

        // filtro: etiqueta (el archivo debe tener al menos un resultado con esa etiqueta)
        if !self.filtro_etiqueta.is_empty() {
            let mut tiene_etiqueta = false;
            for r in &archivo.resultados {
                if r.etiqueta == self.filtro_etiqueta {
                    tiene_etiqueta = true;
                    break;
                }
            }
            if !tiene_etiqueta {
                return false;
            }
        }

        // filtro: confianza (solo aplica a archivos con predicciones)
        if let Some(c) = confianza_maxima(archivo) {
            if c < self.filtro_confianza_min || c > self.filtro_confianza_max {
                return false;
            }
        }

        true
    }

    // posiciones de los archivos que se deben mostrar, ya filtrados y ordenados
    fn indices_visibles(&self) -> Vec<usize> {
        // primero se filtran
        let mut indices = Vec::new();
        for i in 0..self.archivos.len() {
            if self.pasa_filtros(&self.archivos[i]) {
                indices.push(i);
            }
        }

        // despues se ordenan. los archivos sin predicciones quedan al final
        // cuando se ordena por confianza
        match self.orden {
            Orden::Nombre => {
                indices.sort_by(|a, b| self.archivos[*a].nombre.cmp(&self.archivos[*b].nombre));
            }
            Orden::ConfianzaMenorPrimero => {
                indices.sort_by(|a, b| {
                    let ca = confianza_maxima(&self.archivos[*a]).unwrap_or(1000.0);
                    let cb = confianza_maxima(&self.archivos[*b]).unwrap_or(1000.0);
                    ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            Orden::ConfianzaMayorPrimero => {
                indices.sort_by(|a, b| {
                    let ca = confianza_maxima(&self.archivos[*a]).unwrap_or(-1.0);
                    let cb = confianza_maxima(&self.archivos[*b]).unwrap_or(-1.0);
                    cb.partial_cmp(&ca).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        indices
    }

    // cuantos archivos estan seleccionados
    fn cantidad_seleccionados(&self) -> usize {
        let mut cantidad = 0;
        for archivo in &self.archivos {
            if archivo.seleccionado {
                cantidad += 1;
            }
        }
        cantidad
    }

    // borra el json de predicciones de los archivos seleccionados.
    // el archivo original (imagen, audio o video) no se toca
    fn eliminar_seleccionados(&mut self) {
        let mut borrados = 0;
        let mut errores = 0;

        for archivo in &mut self.archivos {
            if !archivo.seleccionado {
                continue;
            }

            // si no tiene predicciones no hay nada que borrar
            if archivo.predicciones.is_none() {
                archivo.seleccionado = false;
                continue;
            }

            let ruta_json = match sidecar_predictions_path(&archivo.ruta) {
                Ok(r) => r,
                Err(_) => {
                    errores += 1;
                    continue;
                }
            };

            match fs::remove_file(&ruta_json) {
                Ok(_) => {
                    // el archivo queda como "sin predicciones"
                    archivo.predicciones = None;
                    archivo.resultados = Vec::new();
                    archivo.borrador = Vec::new();
                    archivo.abierto = false;
                    archivo.seleccionado = false;
                    borrados += 1;
                }
                Err(_) => {
                    errores += 1;
                }
            }
        }

        if errores > 0 {
            self.mensaje = format!("predicciones eliminadas: {}, con error: {}", borrados, errores);
        } else {
            self.mensaje = format!("predicciones eliminadas: {}", borrados);
        }
    }

    // datos para los graficos. solo usa los archivos que se ven en la lista
    // (respeta los filtros) y los resultados ya guardados
    pub(super) fn datos_para_graficos(&self) -> DatosGraficos {
        // resultados de los archivos visibles
        let mut resultados: Vec<(String, f32)> = Vec::new();
        for i in self.indices_visibles() {
            for r in &self.archivos[i].resultados {
                resultados.push((r.etiqueta.clone(), r.confianza));
            }
        }

        // cuantas veces aparece cada etiqueta en toda la carpeta (sin filtros)
        let mut conteo: Vec<(String, usize)> = Vec::new();
        for archivo in &self.archivos {
            for r in &archivo.resultados {
                let mut encontrada = false;
                for fila in &mut conteo {
                    if fila.0 == r.etiqueta {
                        fila.1 += 1;
                        encontrada = true;
                        break;
                    }
                }
                if !encontrada {
                    conteo.push((r.etiqueta.clone(), 1));
                }
            }
        }
        // de mas comun a menos comun (si empatan, por nombre)
        conteo.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        let mut orden_etiquetas = Vec::new();
        for (etiqueta, _) in conteo {
            orden_etiquetas.push(etiqueta);
        }

        DatosGraficos {
            resultados,
            orden_etiquetas,
        }
    }

    // ---------------------------------------------------------------------
    // analisis
    // ---------------------------------------------------------------------

    // archivos que se van a analizar: los seleccionados, o si no hay ninguno
    // seleccionado, todos los que se ven en la lista. solo se incluyen los que
    // sirven para la ia elegida (de imagen: imagenes y videos; de audio: audios)
    fn archivos_para_analizar(&self, info: &InfoModelo) -> Vec<usize> {
        let mut candidatos = Vec::new();
        if self.cantidad_seleccionados() > 0 {
            for i in 0..self.archivos.len() {
                if self.archivos[i].seleccionado {
                    candidatos.push(i);
                }
            }
        } else {
            candidatos = self.indices_visibles();
        }

        let mut compatibles = Vec::new();
        for i in candidatos {
            let tipo = self.archivos[i].tipo;
            let sirve = if info.es_audio {
                tipo == TipoArchivo::Audio
            } else {
                tipo == TipoArchivo::Imagen || tipo == TipoArchivo::Video
            };
            if sirve {
                compatibles.push(i);
            }
        }
        compatibles
    }

    // se llama al apretar "analizar". si algun archivo ya tenia predicciones
    // se pregunta antes (igual que en la pantalla principal)
    fn pedir_analisis(&mut self, info: &InfoModelo) {
        let indices = self.archivos_para_analizar(info);
        if indices.is_empty() {
            if info.es_audio {
                self.mensaje = String::from("no hay audios para analizar con esta ia");
            } else {
                self.mensaje = String::from("no hay imagenes ni videos para analizar con esta ia");
            }
            return;
        }

        // se revisa si alguno ya tiene predicciones
        let mut alguno_ya_tiene = false;
        for i in &indices {
            if self.archivos[*i].predicciones.is_some() {
                alguno_ya_tiene = true;
                break;
            }
        }

        if alguno_ya_tiene {
            // se guarda la lista y se muestra la ventana de confirmacion
            self.pendientes_de_confirmar = indices;
        } else {
            self.iniciar_analisis(indices, info.paso_video);
        }
    }

    // parte el analisis en un hilo aparte. cada vez que termina un archivo,
    // el hilo manda un aviso por el canal "receptor"
    fn iniciar_analisis(&mut self, indices: Vec<usize>, paso_video: usize) {
        // lista de trabajos: (posicion en la lista, ruta, tipo)
        let mut trabajos: Vec<(usize, PathBuf, TipoArchivo)> = Vec::new();
        for i in &indices {
            let archivo = &self.archivos[*i];
            trabajos.push((*i, archivo.ruta.clone(), archivo.tipo));
        }

        let (emisor, receptor) = mpsc::channel();
        let cancelar = Arc::new(AtomicBool::new(false));
        let cancelar_para_el_hilo = cancelar.clone();
        let total = trabajos.len();

        std::thread::spawn(move || {
            for (indice, ruta, tipo) in trabajos {
                // si se apreto cancelar se dejan de analizar archivos
                if cancelar_para_el_hilo.load(Ordering::Relaxed) {
                    break;
                }

                let ok = match tipo {
                    TipoArchivo::Imagen => analizar_imagen(&ruta),
                    TipoArchivo::Audio => analizar_audio(&ruta),
                    TipoArchivo::Video => analizar_video(&ruta, paso_video, &cancelar_para_el_hilo),
                };

                // si la pantalla ya no escucha, se termina
                if emisor.send(AvisoAnalisis { indice, ok }).is_err() {
                    break;
                }
            }
            // al salir de aqui el "emisor" se destruye y el editor sabe que termino
        });

        let primer_nombre = match indices.first() {
            Some(i) => self.archivos[*i].nombre.clone(),
            None => String::new(),
        };

        self.analisis = Some(AnalisisEnCurso {
            receptor,
            cancelar,
            total,
            terminados: 0,
            errores: 0,
            analizando: primer_nombre,
        });
        self.mensaje = format!("analizando {} archivo(s)...", total);
    }

    // revisa si llegaron avisos del hilo de analisis. se llama en cada cuadro
    fn revisar_analisis(&mut self) {
        // primero se juntan los avisos (sin tocar la lista de archivos todavia)
        let mut avisos: Vec<AvisoAnalisis> = Vec::new();
        let mut termino = false;

        if let Some(analisis) = &self.analisis {
            loop {
                match analisis.receptor.try_recv() {
                    Ok(aviso) => avisos.push(aviso),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        termino = true;
                        break;
                    }
                }
            }
        } else {
            return; // no hay analisis corriendo
        }

        // despues se procesan los avisos
        for aviso in avisos {
            if aviso.ok {
                self.recargar_archivo(aviso.indice);
            }
            if let Some(analisis) = &mut self.analisis {
                analisis.terminados += 1;
                if !aviso.ok {
                    analisis.errores += 1;
                }
            }
        }

        // nombre del siguiente archivo (solo para mostrarlo en pantalla)
        let mut siguiente_nombre: Option<String> = None;
        if let Some(analisis) = &self.analisis {
            if !termino {
                // no se sabe exactamente cual va, se muestra el contador
                siguiente_nombre = Some(format!(
                    "archivo {} de {}",
                    analisis.terminados + 1,
                    analisis.total
                ));
            }
        }
        if let (Some(n), Some(analisis)) = (siguiente_nombre, &mut self.analisis) {
            analisis.analizando = n;
        }

        // si el hilo termino se muestra el resumen
        if termino {
            if let Some(analisis) = &self.analisis {
                let bien = analisis.terminados - analisis.errores;
                let cancelado = analisis.cancelar.load(Ordering::Relaxed);
                if cancelado {
                    self.mensaje = format!("analisis cancelado: {} archivo(s) guardados", bien);
                } else if analisis.errores > 0 {
                    self.mensaje = format!(
                        "analisis terminado: {} guardados, {} con error",
                        bien, analisis.errores
                    );
                } else {
                    self.mensaje = format!("analisis terminado: {} archivo(s) guardados", bien);
                }
            }
            self.analisis = None;
        }
    }

    // ---------------------------------------------------------------------
    // dibujo en pantalla
    // ---------------------------------------------------------------------

    // dibuja todo el editor de resultados. se llama una vez por cuadro.
    // "info" trae los datos de la ia elegida en la pantalla principal
    pub(super) fn ui(&mut self, ui: &mut egui::Ui, info: &InfoModelo) {
        // si hay un analisis corriendo se revisan sus avisos y se pide redibujar
        if self.analisis.is_some() {
            self.revisar_analisis();
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            // titulo del recuadro
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Editor de resultados").strong());
            });
            ui.add_space(4.0);

            // fila de botones de arriba
            self.ui_barra_de_botones(ui, info);

            // barra de progreso del analisis (solo si hay uno corriendo)
            self.ui_progreso_analisis(ui);

            // panel de filtros (solo si esta abierto)
            if self.mostrar_filtros {
                ui.separator();
                self.ui_filtros(ui);
            }

            ui.separator();

            // mensaje para el usuario
            if !self.mensaje.is_empty() {
                ui.label(egui::RichText::new(&self.mensaje).weak());
            }

            // si no hay carpeta abierta se muestra una ayuda y se termina
            if self.carpeta.is_none() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("Abre una carpeta para ver sus resultados").weak());
                });
                ui.add_space(20.0);
                return;
            }

            // lista de archivos
            self.ui_lista(ui);
        });

        // ventanas que se dibujan encima de todo
        if self.confirmar_eliminar {
            self.ui_confirmar_eliminar(ui);
        }
        if !self.pendientes_de_confirmar.is_empty() {
            self.ui_confirmar_analisis(ui, info);
        }
    }

    // fila de botones: abrir carpeta, analizar, seleccionar, eliminar, filtros
    fn ui_barra_de_botones(&mut self, ui: &mut egui::Ui, info: &InfoModelo) {
        let analizando = self.analisis.is_some();

        ui.horizontal_wrapped(|ui| {
            // abrir carpeta (no se puede mientras se analiza, porque cambiaria la lista)
            if ui
                .add_enabled(!analizando, egui::Button::new("📂 Abrir carpeta"))
                .on_disabled_hover_text("espera a que termine el analisis")
                .clicked()
            {
                if let Some(carpeta) = rfd::FileDialog::new().pick_folder() {
                    self.cargar_carpeta(carpeta);
                }
            }

            // analizar: solo si hay carpeta, hay una ia local elegida y nada
            // mas se esta analizando
            let mut motivo_no_se_puede = "";
            if self.carpeta.is_none() {
                motivo_no_se_puede = "primero abre una carpeta";
            } else if !info.es_local {
                motivo_no_se_puede = "el analisis desde el editor solo funciona con la ia en este computador";
            } else if !info.hay_modelo {
                motivo_no_se_puede = "primero elige una ia en la pantalla principal";
            } else if info.ocupado {
                motivo_no_se_puede = "la pantalla principal esta analizando, espera a que termine";
            } else if analizando {
                motivo_no_se_puede = "ya hay un analisis corriendo";
            }
            let se_puede_analizar = motivo_no_se_puede.is_empty();

            // texto de ayuda al pasar el mouse sobre "analizar"
            let ayuda_analizar = if self.cantidad_seleccionados() > 0 {
                format!("analiza los archivos seleccionados con {} y guarda sus resultados", info.nombre)
            } else {
                format!("analiza todos los archivos de la lista con {} y guarda sus resultados", info.nombre)
            };

            let boton_analizar = ui
                .add_enabled(se_puede_analizar, egui::Button::new("Analizar ▶"))
                .on_hover_text(ayuda_analizar)
                .on_disabled_hover_text(motivo_no_se_puede);
            if boton_analizar.clicked() {
                self.pedir_analisis(info);
            }

            // seleccionar: muestra u oculta las casillas
            let texto_seleccionar = if self.modo_seleccion { "Seleccionar ☑" } else { "Seleccionar ☐" };
            if ui
                .add(egui::Button::new(texto_seleccionar).selected(self.modo_seleccion))
                .clicked()
            {
                self.modo_seleccion = !self.modo_seleccion;
                // al salir del modo seleccion se desmarca todo
                if !self.modo_seleccion {
                    for archivo in &mut self.archivos {
                        archivo.seleccionado = false;
                    }
                }
            }

            // eliminar: solo si hay algo seleccionado y no se esta analizando
            let hay_seleccionados = self.cantidad_seleccionados() > 0;
            if ui
                .add_enabled(hay_seleccionados && !analizando, egui::Button::new("Eliminar 🗑"))
                .on_disabled_hover_text("primero selecciona archivos (y espera a que termine el analisis)")
                .clicked()
            {
                self.confirmar_eliminar = true;
            }

            // filtros: muestra u oculta el panel de filtros
            if ui
                .add(egui::Button::new("Filtros ⏷").selected(self.mostrar_filtros))
                .clicked()
            {
                self.mostrar_filtros = !self.mostrar_filtros;
            }
        });

        // en modo seleccion se agregan botones para marcar o desmarcar todo
        if self.modo_seleccion {
            ui.horizontal(|ui| {
                if ui.small_button("marcar todos").clicked() {
                    // solo se marcan los que se ven con los filtros actuales
                    let visibles = self.indices_visibles();
                    for i in visibles {
                        self.archivos[i].seleccionado = true;
                    }
                }
                if ui.small_button("desmarcar todos").clicked() {
                    for archivo in &mut self.archivos {
                        archivo.seleccionado = false;
                    }
                }
                ui.label(format!("{} seleccionados", self.cantidad_seleccionados()));
            });
        }
    }

    // barra de progreso y boton cancelar mientras se analiza
    fn ui_progreso_analisis(&mut self, ui: &mut egui::Ui) {
        let mut apreto_cancelar = false;

        if let Some(analisis) = &self.analisis {
            let progreso = if analisis.total > 0 {
                analisis.terminados as f32 / analisis.total as f32
            } else {
                0.0
            };
            ui.add_space(4.0);
            ui.label(format!("analizando: {}", analisis.analizando));
            ui.horizontal(|ui| {
                ui.add(
                    egui::ProgressBar::new(progreso)
                        .show_percentage()
                        .animate(true),
                );
                if ui.button("Cancelar").clicked() {
                    apreto_cancelar = true;
                }
            });
        }

        if apreto_cancelar {
            if let Some(analisis) = &self.analisis {
                analisis.cancelar.store(true, Ordering::Relaxed);
            }
            self.mensaje = String::from("cancelando...");
        }
    }

    // panel de filtros
    fn ui_filtros(&mut self, ui: &mut egui::Ui) {
        // se calculan antes para no pedir prestado "self" dos veces
        let etiquetas = self.etiquetas_conocidas();
        let formatos = self.formatos_conocidos();

        egui::Grid::new("filtros_editor_resultados")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                // ----- etiqueta -----
                ui.label("Etiqueta");
                let texto_etiqueta = if self.filtro_etiqueta.is_empty() {
                    String::from("todas")
                } else {
                    self.filtro_etiqueta.clone()
                };
                egui::ComboBox::from_id_salt("filtro_etiqueta")
                    .selected_text(texto_etiqueta)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.filtro_etiqueta, String::new(), "todas");
                        for (etiqueta, _) in &etiquetas {
                            ui.selectable_value(&mut self.filtro_etiqueta, etiqueta.clone(), etiqueta);
                        }
                    });
                ui.end_row();

                // ----- formato -----
                ui.label("Formato");
                let texto_formato = if self.filtro_formato.is_empty() {
                    String::from("todos")
                } else {
                    self.filtro_formato.clone()
                };
                egui::ComboBox::from_id_salt("filtro_formato")
                    .selected_text(texto_formato)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.filtro_formato, String::new(), "todos");
                        for formato in &formatos {
                            ui.selectable_value(&mut self.filtro_formato, formato.clone(), formato);
                        }
                    });
                ui.end_row();

                // ----- ordenar -----
                ui.label("Ordenar");
                let texto_orden = match self.orden {
                    Orden::Nombre => "nombre",
                    Orden::ConfianzaMenorPrimero => "confianza (menor primero)",
                    Orden::ConfianzaMayorPrimero => "confianza (mayor primero)",
                };
                egui::ComboBox::from_id_salt("filtro_orden")
                    .selected_text(texto_orden)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.orden, Orden::Nombre, "nombre");
                        ui.selectable_value(&mut self.orden, Orden::ConfianzaMenorPrimero, "confianza (menor primero)");
                        ui.selectable_value(&mut self.orden, Orden::ConfianzaMayorPrimero, "confianza (mayor primero)");
                    });
                ui.end_row();

                // ----- confianza minima -----
                ui.label("Confianza min");
                ui.add(egui::Slider::new(&mut self.filtro_confianza_min, 0.0..=100.0).suffix("%"));
                ui.end_row();

                // ----- confianza maxima -----
                ui.label("Confianza max");
                ui.add(egui::Slider::new(&mut self.filtro_confianza_max, 0.0..=100.0).suffix("%"));
                ui.end_row();
            });

        // si el minimo queda sobre el maximo, se corrige
        if self.filtro_confianza_min > self.filtro_confianza_max {
            self.filtro_confianza_max = self.filtro_confianza_min;
        }

        ui.checkbox(&mut self.filtro_solo_con_predicciones, "Solo archivos con predicciones");

        // boton para dejar los filtros como al principio
        if ui.small_button("limpiar filtros").clicked() {
            self.filtro_etiqueta = String::new();
            self.filtro_formato = String::new();
            self.filtro_confianza_min = 0.0;
            self.filtro_confianza_max = 100.0;
            self.filtro_solo_con_predicciones = false;
            self.orden = Orden::Nombre;
        }
    }

    // lista de archivos con scroll
    fn ui_lista(&mut self, ui: &mut egui::Ui) {
        // se calculan antes de dibujar
        let visibles = self.indices_visibles();
        let etiquetas = self.etiquetas_conocidas();
        let modo_seleccion = self.modo_seleccion;

        ui.label(
            egui::RichText::new(format!("mostrando {} de {} archivos", visibles.len(), self.archivos.len()))
                .weak(),
        );

        // aqui se guarda un mensaje si alguna fila pide guardar o cancelar
        let mut nuevo_mensaje: Option<String> = None;

        egui::ScrollArea::vertical()
            .id_salt("lista_editor_resultados")
            .max_height(520.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for i in visibles {
                    let archivo = &mut self.archivos[i];
                    if let Some(m) = ui_fila_archivo(ui, i, archivo, modo_seleccion, &etiquetas) {
                        nuevo_mensaje = Some(m);
                    }
                    ui.add_space(4.0);
                }
            });

        if let Some(m) = nuevo_mensaje {
            self.mensaje = m;
        }
    }

    // ventana "seguro que quieres eliminar?"
    fn ui_confirmar_eliminar(&mut self, ui: &mut egui::Ui) {
        let cantidad = self.cantidad_seleccionados();
        egui::Window::new("Eliminar predicciones")
            .collapsible(false)
            .resizable(false)
            .show(ui, |ui| {
                ui.label(format!(
                    "Se borraran las predicciones de {} archivo(s).",
                    cantidad
                ));
                ui.label("Las imagenes, audios y videos originales NO se borran.");
                ui.horizontal(|ui| {
                    if ui.button("Si, eliminar").clicked() {
                        self.eliminar_seleccionados();
                        self.confirmar_eliminar = false;
                    }
                    if ui.button("Cancelar").clicked() {
                        self.confirmar_eliminar = false;
                    }
                });
            });
    }

    // ventana "algunos archivos ya tienen predicciones, que hago?"
    // (igual a la que muestra la pantalla principal al analizar)
    fn ui_confirmar_analisis(&mut self, ui: &mut egui::Ui, info: &InfoModelo) {
        // cuantos ya tienen predicciones y cuantos no
        let total = self.pendientes_de_confirmar.len();
        let mut ya_tienen = 0;
        for i in &self.pendientes_de_confirmar {
            if self.archivos[*i].predicciones.is_some() {
                ya_tienen += 1;
            }
        }

        // que eligio el usuario (se aplica despues de dibujar la ventana)
        let mut eleccion = 0; // 0 = nada, 1 = todos, 2 = solo faltantes, 3 = cancelar

        egui::Window::new("Analizar")
            .collapsible(false)
            .resizable(false)
            .show(ui, |ui| {
                ui.label(format!(
                    "{} de {} archivo(s) ya tienen predicciones.",
                    ya_tienen, total
                ));
                ui.label("Si los vuelves a analizar se reemplazan sus resultados (y los cambios que hayas hecho).");
                ui.horizontal(|ui| {
                    if ui.button("Analizar todos").clicked() {
                        eleccion = 1;
                    }
                    if ui.button("Solo los que faltan").clicked() {
                        eleccion = 2;
                    }
                    if ui.button("Cancelar").clicked() {
                        eleccion = 3;
                    }
                });
            });

        if eleccion == 1 {
            // todos los archivos de la lista
            let indices = self.pendientes_de_confirmar.clone();
            self.pendientes_de_confirmar = Vec::new();
            self.iniciar_analisis(indices, info.paso_video);
        } else if eleccion == 2 {
            // solo los que no tienen predicciones
            let mut indices = Vec::new();
            for i in &self.pendientes_de_confirmar {
                if self.archivos[*i].predicciones.is_none() {
                    indices.push(*i);
                }
            }
            self.pendientes_de_confirmar = Vec::new();
            if indices.is_empty() {
                self.mensaje = String::from("todos los archivos ya tenian predicciones");
            } else {
                self.iniciar_analisis(indices, info.paso_video);
            }
        } else if eleccion == 3 {
            self.pendientes_de_confirmar = Vec::new();
        }
    }
}

// dibuja una fila de la lista (un archivo). devuelve un mensaje si se guardo
// o si hubo un error. "posicion" se usa para que cada fila tenga ids unicos
fn ui_fila_archivo(
    ui: &mut egui::Ui,
    posicion: usize,
    archivo: &mut ArchivoEditor,
    modo_seleccion: bool,
    etiquetas: &Vec<(String, u32)>,
) -> Option<String> {
    let mut mensaje: Option<String> = None;

    // color del borde segun la confianza mas alta del archivo
    let confianza = confianza_maxima(archivo);
    let color = color_por_confianza(confianza);

    // icono segun el tipo de archivo
    let icono = match archivo.tipo {
        TipoArchivo::Imagen => "🖼",
        TipoArchivo::Audio => "🔊",
        TipoArchivo::Video => "🎞",
    };

    egui::Frame::new()
        .stroke(egui::Stroke::new(1.5, color))
        .corner_radius(4.0)
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            // ----- linea principal: casilla, nombre, info, confianza, boton -----
            ui.horizontal(|ui| {
                // casilla de seleccion (solo en modo seleccion)
                if modo_seleccion {
                    ui.checkbox(&mut archivo.seleccionado, "");
                }

                // icono y nombre del archivo
                ui.label(format!("{} {}", icono, archivo.nombre));

                // lo de la derecha se dibuja de derecha a izquierda
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // boton modificar / cerrar (solo si hay predicciones)
                    if !archivo.resultados.is_empty() {
                        let texto_boton = if archivo.abierto { "Cerrar" } else { "Modificar ✏" };
                        if ui.small_button(texto_boton).clicked() {
                            if archivo.abierto {
                                // se cierra sin guardar
                                archivo.abierto = false;
                                archivo.borrador = Vec::new();
                            } else {
                                // se abre con una copia de los resultados
                                archivo.abierto = true;
                                archivo.borrador = archivo.resultados.clone();
                            }
                        }
                    }

                    // confianza mas alta en porcentaje, con el mismo color del borde
                    if let Some(c) = confianza {
                        ui.label(egui::RichText::new(format!("{:.0}%", c)).color(color).strong());
                    }

                    // icono de informacion: al pasar el mouse muestra un resumen
                    if !archivo.resultados.is_empty() {
                        ui.label("ℹ").on_hover_ui(|ui| {
                            ui_resumen_resultados(ui, archivo);
                        });
                    }
                });
            });

            // ----- panel de modificar (solo si esta abierto) -----
            if archivo.abierto {
                ui.separator();
                ui.label(egui::RichText::new("Resultados").strong());

                // en videos puede haber muchisimos resultados: se muestran solo los primeros
                let total = archivo.borrador.len();
                let cuantos_mostrar = if total > MAX_RESULTADOS_EN_PANTALLA {
                    MAX_RESULTADOS_EN_PANTALLA
                } else {
                    total
                };
                if total > cuantos_mostrar {
                    ui.label(
                        egui::RichText::new(format!(
                            "mostrando los primeros {} de {} resultados",
                            cuantos_mostrar, total
                        ))
                        .weak(),
                    );
                }

                for n in 0..cuantos_mostrar {
                    let r = &mut archivo.borrador[n];

                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());

                        // titulo del resultado (en videos se agrega el cuadro)
                        match r.cuadro {
                            Some(numero_cuadro) => {
                                let segundos = r.inicio.unwrap_or(0.0);
                                ui.label(format!(
                                    "resultado {} (cuadro {}, {:.2}s):",
                                    n + 1,
                                    numero_cuadro,
                                    segundos
                                ));
                            }
                            None => {
                                ui.label(format!("resultado {}:", n + 1));
                            }
                        }

                        // etiqueta: lista desplegable con las etiquetas de la carpeta
                        ui.horizontal(|ui| {
                            ui.label("Etiqueta");
                            egui::ComboBox::from_id_salt(("etiqueta_resultado", posicion, n))
                                .selected_text(r.etiqueta.clone())
                                .show_ui(ui, |ui| {
                                    for (etiqueta, id_clase) in etiquetas {
                                        let elegida = ui.selectable_label(r.etiqueta == *etiqueta, etiqueta);
                                        if elegida.clicked() {
                                            r.etiqueta = etiqueta.clone();
                                            r.id_clase = *id_clase;
                                        }
                                    }
                                });
                        });

                        // confianza en porcentaje
                        ui.horizontal(|ui| {
                            ui.label("Confianza");
                            ui.add(
                                egui::DragValue::new(&mut r.confianza)
                                    .range(0.0..=100.0)
                                    .speed(0.5)
                                    .suffix("%"),
                            );
                        });

                        // rango de tiempo (solo audios; en videos ya va en el titulo)
                        if r.cuadro.is_none() {
                            if let (Some(inicio), Some(fin)) = (r.inicio, r.fin) {
                                ui.label(format!("Rango de tiempo: {:.2}s - {:.2}s", inicio, fin));
                            }
                        }

                        // marcar para borrar este resultado
                        ui.checkbox(&mut r.borrar, "Borrar este resultado");
                    });
                }

                // botones guardar y cancelar
                ui.horizontal(|ui| {
                    if ui.button("💾 Guardar").clicked() {
                        mensaje = Some(guardar_borrador(archivo));
                    }
                    if ui.button("Cancelar").clicked() {
                        archivo.abierto = false;
                        archivo.borrador = Vec::new();
                    }
                });
            }
        });

    mensaje
}

// resumen que aparece al pasar el mouse sobre el icono de informacion
fn ui_resumen_resultados(ui: &mut egui::Ui, archivo: &ArchivoEditor) {
    if archivo.tipo == TipoArchivo::Video {
        // en videos se cuenta cuantas veces aparece cada etiqueta y su mejor confianza
        let mut resumen: Vec<(String, usize, f32)> = Vec::new(); // (etiqueta, veces, maxima)
        for r in &archivo.resultados {
            let mut encontrada = false;
            for fila in &mut resumen {
                if fila.0 == r.etiqueta {
                    fila.1 += 1;
                    if r.confianza > fila.2 {
                        fila.2 = r.confianza;
                    }
                    encontrada = true;
                    break;
                }
            }
            if !encontrada {
                resumen.push((r.etiqueta.clone(), 1, r.confianza));
            }
        }
        ui.label(format!("{} resultados en el video", archivo.resultados.len()));
        for (etiqueta, veces, maxima) in resumen {
            ui.label(format!("{}: {} veces (max {:.0}%)", etiqueta, veces, maxima));
        }
    } else {
        // en imagenes y audios se muestra cada resultado
        for r in &archivo.resultados {
            ui.label(format!("{} — {:.0}%", r.etiqueta, r.confianza));
        }
    }
}

// guarda el borrador de un archivo en su json. devuelve el mensaje a mostrar
fn guardar_borrador(archivo: &mut ArchivoEditor) -> String {
    // sin predicciones no hay nada que guardar (no deberia pasar)
    let predicciones = match &mut archivo.predicciones {
        Some(p) => p,
        None => return String::from("este archivo no tiene predicciones"),
    };

    // se copia el borrador a las predicciones y se escribe el json
    aplicar_cambios_predicciones(predicciones, &archivo.borrador);
    if !guardar_predicciones(&archivo.ruta, predicciones) {
        return format!("error al guardar {}", archivo.nombre);
    }

    // la lista visible pasa a ser lo que se acaba de guardar
    archivo.resultados = resultados_desde_predicciones(predicciones);
    archivo.borrador = Vec::new();
    archivo.abierto = false;
    format!("cambios guardados en {}", archivo.nombre)
}

// #####
