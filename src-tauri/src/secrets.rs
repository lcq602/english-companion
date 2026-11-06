//! 密钥保护：Windows DPAPI（当前用户范围）。
//! 前端把所有翻译服务密钥打包成一个 JSON 字符串交给这里加密，
//! 只保存十六进制密文；解密也只能在同一台机器、同一个 Windows 用户下完成。

#[cfg(windows)]
const ENTROPY: &[u8] = b"EnglishCompanion.secrets.v1";

pub fn protect(plaintext: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        let encrypted = dpapi(plaintext.as_bytes(), true)?;
        Ok(hex_encode(&encrypted))
    }
    #[cfg(not(windows))]
    {
        let _ = plaintext;
        Err("密钥加密仅支持 Windows".to_string())
    }
}

pub fn unprotect(hex: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        let bytes = hex_decode(hex).ok_or_else(|| "本地密钥数据损坏".to_string())?;
        let decrypted = dpapi(&bytes, false)?;
        String::from_utf8(decrypted).map_err(|_| "本地密钥数据损坏".to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = hex;
        Err("密钥解密仅支持 Windows".to_string())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[cfg(windows)]
fn dpapi(input: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr() as *mut u8,
    };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr() as *mut u8,
    };
    let mut output_blob = CRYPT_INTEGER_BLOB::default();
    unsafe {
        let result = if encrypt {
            CryptProtectData(
                &input_blob,
                windows_core::PCWSTR::null(),
                Some(&entropy_blob),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output_blob,
            )
        } else {
            CryptUnprotectData(
                &input_blob,
                None,
                Some(&entropy_blob),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output_blob,
            )
        };
        result.map_err(|_| {
            if encrypt {
                "无法用 Windows DPAPI 加密密钥".to_string()
            } else {
                "无法用 Windows DPAPI 解密密钥（可能不是同一台电脑或同一个 Windows 用户）".to_string()
            }
        })?;
        let data = std::slice::from_raw_parts(output_blob.pbData, output_blob.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output_blob.pbData as *mut core::ffi::c_void)));
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let data = vec![0u8, 15, 16, 255];
        assert_eq!(hex_decode(&hex_encode(&data)), Some(data));
        assert_eq!(hex_decode("zz"), None);
        assert_eq!(hex_decode("abc"), None);
    }
}
