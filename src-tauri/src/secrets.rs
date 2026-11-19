// PAIGE Desktop - owner-side credential protection (Increment 1E).
//
// Owner-connected gateway keys are stored OUTSIDE the repository, in the
// per-user app config directory, encrypted at rest with Windows DPAPI
// (CryptProtectData / CryptUnprotectData) bound to the owner's account.
// Keys never enter the WebView, the repository, or any committed artifact.

use std::path::PathBuf;

// --- Windows DPAPI (crypt32) ---
#[link(name = "crypt32")]
unsafe extern "system" {
    fn CryptProtectData(
        pdata_in: *const DATA_BLOB,
        sz_data_desc: *const u16,
        p_optional_entropy: *const std::ffi::c_void,
        pv_reserved: *const std::ffi::c_void,
        pprompt_struct: *const std::ffi::c_void,
        dw_flags: u32,
        pdata_out: *mut DATA_BLOB,
    ) -> i32;
    fn CryptUnprotectData(
        pdata_in: *const DATA_BLOB,
        ppsz_data_desc: *mut *const u16,
        p_optional_entropy: *const std::ffi::c_void,
        pv_reserved: *const std::ffi::c_void,
        pprompt_struct: *const std::ffi::c_void,
        dw_flags: u32,
        pdata_out: *mut DATA_BLOB,
    ) -> i32;
    fn LocalFree(h: *const std::ffi::c_void) -> *const std::ffi::c_void;
}

#[repr(C)]
struct DATA_BLOB {
    cb_size: u32,
    pb_data: *mut u8,
}

const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

fn dpapi_protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    unsafe {
        let mut in_blob = DATA_BLOB {
            cb_size: plain.len() as u32,
            pb_data: plain.as_ptr() as *mut u8,
        };
        let mut out_blob = DATA_BLOB { cb_size: 0, pb_data: std::ptr::null_mut() };
        let ok = CryptProtectData(
            &in_blob,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        );
        if ok == 0 {
            return Err("CryptProtectData failed".to_string());
        }
        let out = std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_size as usize).to_vec();
        LocalFree(out_blob.pb_data as *const std::ffi::c_void);
        Ok(out)
    }
}

fn dpapi_unprotect(cipher: &[u8]) -> Result<Vec<u8>, String> {
    unsafe {
        let in_blob = DATA_BLOB {
            cb_size: cipher.len() as u32,
            pb_data: cipher.as_ptr() as *mut u8,
        };
        let mut out_blob = DATA_BLOB { cb_size: 0, pb_data: std::ptr::null_mut() };
        let ok = CryptUnprotectData(
            &in_blob,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        );
        if ok == 0 {
            return Err("CryptUnprotectData failed (wrong machine/account?)".to_string());
        }
        let out = std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_size as usize).to_vec();
        LocalFree(out_blob.pb_data as *const std::ffi::c_void);
        Ok(out)
    }
}

/// Per-user app config directory (outside the repository):
/// %APPDATA%\paige-desktop\ — created on demand.
/// `PAIGE_TEST_CONFIG_DIR` overrides the location so tests never touch the
/// owner's real gateway configuration.
pub fn config_dir() -> Result<PathBuf, String> {
    let dir = match std::env::var("PAIGE_TEST_CONFIG_DIR") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            let base = std::env::var("APPDATA")
                .map(PathBuf::from)
                .map_err(|_| "APPDATA is not set".to_string())?;
            base.join("paige-desktop")
        }
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create config dir: {e}"))?;
    Ok(dir)
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct GatewayConfig {
    /// DPAPI-protected OpenRouter API key (base64 of ciphertext). None if not connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_key_protected: Option<String>,
    /// "auto" by default; a manual model pin (gateway-prefixed id) if the owner set one.
    #[serde(default = "default_selection")]
    pub selection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_pin: Option<String>,
}

fn default_selection() -> String {
    "auto".to_string()
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            openrouter_key_protected: None,
            selection: default_selection(),
            manual_pin: None,
        }
    }
}

impl GatewayConfig {
    fn path() -> Result<PathBuf, String> {
        Ok(config_dir()?.join("gateways.json"))
    }

    pub fn load() -> GatewayConfig {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).map_err(|e| e.to_string()))
            .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path()?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| format!("cannot write gateway config: {e}"))?;
        Ok(())
    }

    pub fn openrouter_key(&self) -> Result<Option<String>, String> {
        match &self.openrouter_key_protected {
            None => Ok(None),
            Some(b64) => {
                let cipher = base64_decode(b64)?;
                let plain = dpapi_unprotect(&cipher)?;
                let s = String::from_utf8(plain).map_err(|_| "stored key is not valid UTF-8".to_string())?;
                Ok(Some(s))
            }
        }
    }

    pub fn set_openrouter_key(&mut self, key: &str) -> Result<(), String> {
        let key = key.trim().to_string();
        if key.is_empty() {
            return Err("empty key".to_string());
        }
        let cipher = dpapi_protect(key.as_bytes())?;
        self.openrouter_key_protected = Some(base64_encode(&cipher));
        self.save()
    }

    pub fn clear_openrouter_key(&mut self) -> Result<(), String> {
        self.openrouter_key_protected = None;
        self.save()
    }
}

// Minimal base64 (no external crate).
const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let bytes: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if bytes.len() % 4 != 0 {
        return Err("invalid base64 length".to_string());
    }
    for chunk in bytes.chunks(4) {
        let mut n: u32 = 0;
        for (i, c) in chunk.iter().enumerate() {
            let v = if *c == b'=' {
                if i < 2 { return Err("invalid base64 padding".to_string()); }
                0
            } else {
                B64.iter().position(|x| x == c).ok_or("invalid base64 character")? as u32
            };
            n = (n << 6) | v;
        }
        out.push((n >> 16) as u8);
        if chunk[2] != b'=' {
            out.push((n >> 8) as u8);
        }
        if chunk[3] != b'=' {
            out.push(n as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trip() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let enc = base64_encode(data);
            let dec = base64_decode(&enc).unwrap();
            assert_eq!(dec, data, "round trip failed for {data:?}");
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn dpapi_round_trip() {
        let plain = b"paige-desktop-test-key-123";
        let cipher = dpapi_protect(plain).unwrap();
        assert_ne!(&cipher, plain);
        let back = dpapi_unprotect(&cipher).unwrap();
        assert_eq!(back, plain);
    }

    #[test]
    fn config_key_store_round_trip() {
        // Isolate the config location so this test never touches the owner's
        // real gateway configuration.
        unsafe {
            std::env::set_var("PAIGE_TEST_CONFIG_DIR", std::env::temp_dir().join("paige-cfg-test").to_string_lossy().to_string());
        }
        let mut cfg = GatewayConfig::default();
        cfg.set_openrouter_key("sk-or-test-abc123").unwrap();
        assert!(cfg.openrouter_key_protected.is_some());
        let stored = cfg.openrouter_key_protected.clone().unwrap();
        assert!(!stored.contains("sk-or-test"), "ciphertext must not contain the plaintext");
        let key = cfg.openrouter_key().unwrap();
        assert_eq!(key.as_deref(), Some("sk-or-test-abc123"));
        cfg.clear_openrouter_key().unwrap();
        assert!(cfg.openrouter_key().unwrap().is_none());
    }
}
