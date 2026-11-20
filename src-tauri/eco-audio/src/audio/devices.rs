//! Dispositivos de audio: enumeración (con pseudo-dispositivo «default») y resolución por id.

use crate::types::{Device, Devices};
use cpal::traits::{DeviceTrait, HostTrait};

pub const DEFAULT_ID: &str = "default";

pub fn host() -> cpal::Host {
    cpal::default_host()
}

pub fn device_name(d: &cpal::Device) -> String {
    d.name().unwrap_or_else(|_| "Dispositivo desconocido".to_string())
}

fn default_label(real: Option<&str>) -> String {
    format!(
        "Predeterminado del sistema ({})",
        real.unwrap_or("sin dispositivo")
    )
}

fn cfg_of(d: &cpal::Device, input: bool) -> Option<(u32, u16)> {
    let c = if input { d.default_input_config() } else { d.default_output_config() }.ok()?;
    Some((c.sample_rate().0, c.channels()))
}

fn list_side(host: &cpal::Host, input: bool) -> Vec<Device> {
    let default_dev = if input { host.default_input_device() } else { host.default_output_device() };
    let default_name = default_dev.as_ref().map(device_name);
    let (dsr, dch) = default_dev
        .as_ref()
        .and_then(|d| cfg_of(d, input))
        .unwrap_or((0, 0));
    let mut out = vec![Device {
        id: DEFAULT_ID.into(),
        name: default_label(default_name.as_deref()),
        is_default: true,
        sample_rate: dsr,
        channels: dch,
        real_name: default_name.clone(),
    }];
    let devices: Vec<cpal::Device> = if input {
        host.input_devices().map(|i| i.collect()).unwrap_or_default()
    } else {
        host.output_devices().map(|i| i.collect()).unwrap_or_default()
    };
    for d in devices {
        let name = device_name(&d);
        if out.iter().any(|x| x.id == name) {
            continue; // el id es el nombre: ignora duplicados
        }
        let Some((sr, ch)) = cfg_of(&d, input) else { continue };
        out.push(Device {
            is_default: default_name.as_deref() == Some(name.as_str()),
            id: name.clone(),
            name,
            sample_rate: sr,
            channels: ch,
            real_name: None,
        });
    }
    out
}

pub fn list_devices() -> Devices {
    let h = host();
    Devices { inputs: list_side(&h, true), outputs: list_side(&h, false) }
}

/// Resuelve el id del ajuste a un dispositivo de entrada (micrófono).
pub fn resolve_input(id: &str) -> Result<cpal::Device, String> {
    let h = host();
    if id == DEFAULT_ID {
        return h
            .default_input_device()
            .ok_or_else(|| "No hay un micrófono predeterminado en el sistema.".to_string());
    }
    h.input_devices()
        .map_err(|e| format!("No se pudieron listar los micrófonos: {e}"))?
        .find(|d| d.name().map(|n| n == id).unwrap_or(false))
        .ok_or_else(|| format!("No se encontró el micrófono «{id}». ¿Está conectado?"))
}

/// Resuelve el id del ajuste a un dispositivo de salida (del que se captura el loopback).
pub fn resolve_output(id: &str) -> Result<cpal::Device, String> {
    let h = host();
    if id == DEFAULT_ID {
        return h
            .default_output_device()
            .ok_or_else(|| "No hay un dispositivo de reproducción predeterminado en el sistema.".to_string());
    }
    h.output_devices()
        .map_err(|e| format!("No se pudieron listar los dispositivos de reproducción: {e}"))?
        .find(|d| d.name().map(|n| n == id).unwrap_or(false))
        .ok_or_else(|| format!("No se encontró el dispositivo de reproducción «{id}». ¿Está conectado?"))
}

/// Nombre real al que apunta hoy un id (para detectar cambios del predeterminado).
pub fn resolved_name(id: &str, input: bool) -> Option<String> {
    let d = if input { resolve_input(id) } else { resolve_output(id) }.ok()?;
    Some(device_name(&d))
}
