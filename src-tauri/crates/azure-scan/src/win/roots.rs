//! Where the launchers say they put things.
//!
//! Four of the six readable launchers keep their install list in the
//! registry rather than in a file. All of these are plain reads of keys
//! the launchers publish for exactly this purpose; nothing here writes.

use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ,
};

use crate::sources::{name_from_directory, Installed};

/// Steam's own directory, from the key Steam writes at install.
pub fn steam_path() -> Option<String> {
    read_string(HKEY_CURRENT_USER, w!("Software\\Valve\\Steam"), w!("SteamPath"))
}

/// GOG records the title and the executable, so nothing is guessed.
pub fn gog_installs() -> Vec<Installed> {
    subkeys(HKEY_LOCAL_MACHINE, w!("SOFTWARE\\WOW6432Node\\GOG.com\\Games"))
        .into_iter()
        .filter_map(|id| {
            let key = HSTRING::from(format!("SOFTWARE\\WOW6432Node\\GOG.com\\Games\\{id}"));
            let at = PCWSTR(key.as_ptr());
            let directory = read_string(HKEY_LOCAL_MACHINE, at, w!("path"))?;
            let name = read_string(HKEY_LOCAL_MACHINE, at, w!("gameName"))
                .unwrap_or_else(|| name_from_directory(&directory));
            let exe = read_string(HKEY_LOCAL_MACHINE, at, w!("exe"));
            Some(Installed { name, directory, exe })
        })
        .collect()
}

/// Ubisoft Connect gives a directory and nothing else, so the folder's
/// own name stands in for the title.
pub fn ubisoft_installs() -> Vec<Installed> {
    subkeys(
        HKEY_LOCAL_MACHINE,
        w!("SOFTWARE\\WOW6432Node\\Ubisoft\\Launcher\\Installs"),
    )
    .into_iter()
    .filter_map(|id| {
        let key =
            HSTRING::from(format!("SOFTWARE\\WOW6432Node\\Ubisoft\\Launcher\\Installs\\{id}"));
        let directory = read_string(HKEY_LOCAL_MACHINE, PCWSTR(key.as_ptr()), w!("InstallDir"))?;
        Some(Installed {
            name: name_from_directory(&directory),
            directory,
            exe: None,
        })
    })
    .collect()
}

/// The EA app keys each title by its own name, which is the one useful
/// thing about them.
pub fn ea_installs() -> Vec<Installed> {
    subkeys(HKEY_LOCAL_MACHINE, w!("SOFTWARE\\WOW6432Node\\Electronic Arts"))
        .into_iter()
        .filter_map(|title| {
            let key = HSTRING::from(format!("SOFTWARE\\WOW6432Node\\Electronic Arts\\{title}"));
            let at = PCWSTR(key.as_ptr());
            let directory = read_string(HKEY_LOCAL_MACHINE, at, w!("Install Dir"))
                .or_else(|| read_string(HKEY_LOCAL_MACHINE, at, w!("InstallDir")))?;
            Some(Installed { name: title, directory, exe: None })
        })
        .collect()
}

/// Reads one `REG_SZ`, or `None` when it is absent or not a string.
fn read_string(root: HKEY, key: PCWSTR, value: PCWSTR) -> Option<String> {
    let mut size = 0u32;
    // SAFETY: read-only. A null buffer asks for the size first, which is
    // how the allocation below is sized exactly.
    let status =
        unsafe { RegGetValueW(root, key, value, RRF_RT_REG_SZ, None, None, Some(&mut size)) };
    if status != ERROR_SUCCESS && status != ERROR_MORE_DATA {
        return None;
    }

    let mut buf = vec![0u16; (size as usize / 2) + 1];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: `buf` holds `size` bytes and outlives the call.
    let status = unsafe {
        RegGetValueW(
            root,
            key,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut std::ffi::c_void),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }

    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let text = String::from_utf16_lossy(&buf[..end]).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The names of a key's immediate children. Empty when the key is absent,
/// which is the ordinary case for a launcher nobody installed.
fn subkeys(root: HKEY, key: PCWSTR) -> Vec<String> {
    let mut handle = HKEY::default();
    // SAFETY: opens for reading and closes below.
    let status = unsafe { RegOpenKeyExW(root, key, Some(0), KEY_READ, &mut handle) };
    if status != ERROR_SUCCESS {
        return Vec::new();
    }

    let mut names = Vec::new();
    let mut index = 0u32;
    loop {
        let mut buf = [0u16; 256];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` is `len` wide characters and outlives the call.
        let status = unsafe {
            RegEnumKeyExW(
                handle,
                index,
                Some(PWSTR(buf.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            )
        };
        if status != ERROR_SUCCESS {
            break;
        }
        names.push(String::from_utf16_lossy(&buf[..len as usize]));
        index += 1;
    }

    // SAFETY: the handle came from RegOpenKeyExW above and is closed once.
    unsafe {
        let _ = RegCloseKey(handle);
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registry_probe_answers_without_panicking() {
        // What is installed differs on every machine, so none of these
        // can assert a count. That a missing launcher reads as "nothing"
        // rather than exploding is the contract.
        let _ = steam_path();
        let _ = gog_installs();
        let _ = ubisoft_installs();
        let _ = ea_installs();
    }

    #[test]
    fn a_key_that_does_not_exist_is_empty_rather_than_an_error() {
        assert!(subkeys(HKEY_LOCAL_MACHINE, w!("SOFTWARE\\AzureNoSuchKeyHere")).is_empty());
        assert_eq!(
            read_string(
                HKEY_LOCAL_MACHINE,
                w!("SOFTWARE\\AzureNoSuchKeyHere"),
                w!("nothing")
            ),
            None
        );
    }
}
