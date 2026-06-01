//! OS endpoint volume via Windows Core Audio (`IAudioEndpointVolume`).
//!
//! cpal captures audio but exposes no volume control, so we reach into Core
//! Audio directly to get/set the selected source's Windows volume — the very
//! same slider as Settings → System → Sound → Input → Volume.
//!
//! The cpal/WASAPI device name equals the Core Audio endpoint FriendlyName, so
//! we match endpoints by name. `source` picks the data-flow direction: "system"
//! sources are output (loopback) endpoints → `eRender`; mic inputs → `eCapture`.
//! When no endpoint matches (or name is empty) we fall back to the default
//! endpoint for that flow, so the slider still controls *something* sensible.
//!
//! Windows-only; other platforms get an `Unsupported` error via the stubs.

#[cfg(windows)]
pub fn get_volume(name: &str, source: &str) -> Result<f32, String> {
    imp::get_volume(name, source)
}

#[cfg(windows)]
pub fn set_volume(name: &str, source: &str, level: f32) -> Result<(), String> {
    imp::set_volume(name, source, level)
}

#[cfg(not(windows))]
pub fn get_volume(_name: &str, _source: &str) -> Result<f32, String> {
    Err("입력 볼륨 조절은 Windows에서만 지원됩니다".into())
}

#[cfg(not(windows))]
pub fn set_volume(_name: &str, _source: &str, _level: f32) -> Result<(), String> {
    Err("입력 볼륨 조절은 Windows에서만 지원됩니다".into())
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, eRender, EDataFlow, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
        DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED, STGM_READ,
    };

    /// RAII COM init for the calling thread. Tauri command threads are not COM-
    /// initialized, so each call sets up + tears down its own apartment. A
    /// successful `CoInitializeEx` (incl. `S_FALSE` = already init same mode)
    /// must be paired with `CoUninitialize`; `RPC_E_CHANGED_MODE` means someone
    /// else owns the apartment — don't uninit then.
    struct ComGuard {
        owned: bool,
    }
    impl ComGuard {
        fn new() -> Self {
            let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            ComGuard { owned: hr.is_ok() }
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.owned {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn flow_for(source: &str) -> EDataFlow {
        // "system" = output endpoint captured via loopback → its OS volume is
        // the render (output) endpoint volume. Everything else is a mic input.
        if source == "system" {
            eRender
        } else {
            eCapture
        }
    }

    /// Read PKEY_Device_FriendlyName off an endpoint. Returns None on any COM
    /// hiccup so enumeration can skip and keep going.
    unsafe fn friendly_name(device: &IMMDevice) -> Option<String> {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let prop = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        let pwstr = PropVariantToStringAlloc(&prop).ok()?;
        let s = pwstr.to_string().ok();
        // PropVariantToStringAlloc allocates via CoTaskMemAlloc — free it.
        CoTaskMemFree(Some(pwstr.0 as *const _));
        s
    }

    /// cpal can truncate long endpoint names, so accept exact or either-prefix.
    fn names_match(a: &str, b: &str) -> bool {
        !b.is_empty() && (a == b || a.starts_with(b) || b.starts_with(a))
    }

    unsafe fn find_endpoint(name: &str, flow: EDataFlow) -> Result<IMMDevice, String> {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("CoCreateInstance(MMDeviceEnumerator): {e}"))?;
        let collection = enumerator
            .EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
            .map_err(|e| format!("EnumAudioEndpoints: {e}"))?;
        let count = collection.GetCount().map_err(|e| format!("GetCount: {e}"))?;
        for i in 0..count {
            let device = match collection.Item(i) {
                Ok(d) => d,
                Err(_) => continue,
            };
            if let Some(fname) = friendly_name(&device) {
                if names_match(&fname, name) {
                    return Ok(device);
                }
            }
        }
        // No name match → default endpoint for this flow.
        enumerator
            .GetDefaultAudioEndpoint(flow, eConsole)
            .map_err(|e| format!("GetDefaultAudioEndpoint: {e}"))
    }

    unsafe fn endpoint_volume(name: &str, flow: EDataFlow) -> Result<IAudioEndpointVolume, String> {
        let device = find_endpoint(name, flow)?;
        device
            .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            .map_err(|e| format!("Activate(IAudioEndpointVolume): {e}"))
    }

    pub fn get_volume(name: &str, source: &str) -> Result<f32, String> {
        let _com = ComGuard::new();
        let flow = flow_for(source);
        unsafe {
            let vol = endpoint_volume(name, flow)?;
            vol.GetMasterVolumeLevelScalar()
                .map_err(|e| format!("GetMasterVolumeLevelScalar: {e}"))
        }
    }

    pub fn set_volume(name: &str, source: &str, level: f32) -> Result<(), String> {
        let _com = ComGuard::new();
        let flow = flow_for(source);
        let level = level.clamp(0.0, 1.0);
        unsafe {
            let vol = endpoint_volume(name, flow)?;
            // null event-context GUID: we have no change-notification client.
            vol.SetMasterVolumeLevelScalar(level, std::ptr::null())
                .map_err(|e| format!("SetMasterVolumeLevelScalar: {e}"))
        }
    }
}
