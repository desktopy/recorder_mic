//! Volumen del micrófono en Windows (IAudioEndpointVolume del dispositivo de captura elegido).
//!
//! Solo lee y cambia el nivel del dispositivo de captura (`level` 0..1, escala del control de volumen de Windows).
//! No toca el volumen de reproducción ni el del sistema. En otras plataformas devuelve un error.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndpointVolume {
    pub device_name: String,
    /// 0..1 (escala del control de volumen de Windows).
    pub level: f32,
    pub muted: bool,
    /// Refuerzo de micrófono del driver (dB). No expuesto por la API genérica: siempre `null`.
    pub boost: Option<f32>,
}

/// Lee el volumen del micrófono `input_id` (`"default"` = predeterminado de Windows).
pub fn get_input_endpoint_volume(input_id: &str) -> Result<EndpointVolume, String> {
    imp::run(input_id, None)
}

/// Fija el volumen (0..1) del micrófono `input_id`; si `level` > 0 también le quita el silencio. Devuelve el estado resultante.
pub fn set_input_endpoint_volume(input_id: &str, level: f32) -> Result<EndpointVolume, String> {
    if !level.is_finite() {
        return Err("Nivel de volumen no válido.".into());
    }
    imp::run(input_id, Some(level.clamp(0.0, 1.0)))
}

#[cfg(windows)]
mod imp {
    use super::EndpointVolume;
    use windows::core::PWSTR;
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
    };

    fn friendly_name(dev: &IMMDevice) -> Result<String, String> {
        unsafe {
            let store = dev.OpenPropertyStore(STGM_READ).map_err(|e| e.to_string())?;
            let v = store.GetValue(&PKEY_Device_FriendlyName).map_err(|e| e.to_string())?;
            let p: PWSTR = PropVariantToStringAlloc(&v).map_err(|e| e.to_string())?;
            let s = p.to_string().map_err(|e| e.to_string());
            CoTaskMemFree(Some(p.0 as *const _));
            s
        }
    }

    fn find_device(input_id: &str) -> Result<IMMDevice, String> {
        unsafe {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("No se pudo consultar los dispositivos de audio de Windows: {e}"))?;
            if input_id == "default" {
                return en
                    .GetDefaultAudioEndpoint(eCapture, eConsole)
                    .map_err(|_| "No hay un micrófono predeterminado en el sistema.".to_string());
            }
            let coll = en
                .EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
                .map_err(|e| format!("No se pudieron listar los micrófonos: {e}"))?;
            let n = coll.GetCount().map_err(|e| e.to_string())?;
            for i in 0..n {
                if let Ok(d) = coll.Item(i) {
                    if friendly_name(&d).map(|x| x == input_id).unwrap_or(false) {
                        return Ok(d);
                    }
                }
            }
            Err(format!("No se encontró el micrófono «{input_id}». ¿Está conectado?"))
        }
    }

    fn work(input_id: &str, set: Option<f32>) -> Result<EndpointVolume, String> {
        let dev = find_device(input_id)?;
        let device_name = friendly_name(&dev)?;
        unsafe {
            let vol: IAudioEndpointVolume = dev
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| format!("«{device_name}» no permite leer su volumen: {e}"))?;
            if let Some(level) = set {
                vol.SetMasterVolumeLevelScalar(level, std::ptr::null())
                    .map_err(|e| format!("No se pudo cambiar el volumen del micrófono: {e}"))?;
                if level > 0.0 {
                    vol.SetMute(false, std::ptr::null())
                        .map_err(|e| format!("No se pudo quitar el silencio del micrófono: {e}"))?;
                }
            }
            let level = vol.GetMasterVolumeLevelScalar().map_err(|e| e.to_string())?;
            let muted = vol.GetMute().map_err(|e| e.to_string())?.as_bool();
            Ok(EndpointVolume { device_name, level, muted, boost: None })
        }
    }

    pub fn run(input_id: &str, set: Option<f32>) -> Result<EndpointVolume, String> {
        // COM por llamada: si el hilo ya estaba en otro modelo (cambio de modo) se sigue igual.
        let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let r = work(input_id, set);
        if init.is_ok() {
            unsafe { CoUninitialize() };
        }
        r
    }
}

#[cfg(not(windows))]
mod imp {
    use super::EndpointVolume;
    pub fn run(_input_id: &str, _set: Option<f32>) -> Result<EndpointVolume, String> {
        Err("El volumen del micrófono solo se puede consultar en Windows.".into())
    }
}
