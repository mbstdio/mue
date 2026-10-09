use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::ERROR_FILE_NOT_FOUND,
        System::Registry::{
            HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
            RegSetKeyValueW,
        },
    },
    core::HSTRING,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

// Restore the previous registration if writing the preferences fails.
pub fn configure(enabled: bool, persist: impl FnOnce() -> Result<()>) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let key = HSTRING::from(RUN_KEY);
    let name = HSTRING::from("Mue");
    let previous = unsafe {
        let mut bytes = 0;
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            &name,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut bytes),
        );
        if status == ERROR_FILE_NOT_FOUND {
            None
        } else {
            status.ok().context("Cannot read startup registration")?;
            let mut value = vec![0u16; (bytes as usize).div_ceil(2)];
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                RRF_RT_REG_SZ,
                None,
                Some(value.as_mut_ptr().cast()),
                Some(&mut bytes),
            )
            .ok()
            .context("Cannot read startup registration")?;
            value.truncate((bytes as usize).div_ceil(2));
            Some(value)
        }
    };
    let command = if enabled {
        let executable = std::env::current_exe().context("Cannot locate Mue executable")?;
        Some(
            std::iter::once('"' as u16)
                .chain(executable.as_os_str().encode_wide())
                .chain("\" --background\0".encode_utf16())
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    write(command.as_deref())?;
    if let Err(error) = persist() {
        write(previous.as_deref())
            .context(format!("{error:#}; cannot restore startup registration"))?;
        return Err(error);
    }
    Ok(())
}

fn write(command: Option<&[u16]>) -> Result<()> {
    let key = HSTRING::from(RUN_KEY);
    let name = HSTRING::from("Mue");
    let status = unsafe {
        match command {
            Some(value) => RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                REG_SZ.0,
                Some(value.as_ptr().cast()),
                std::mem::size_of_val(value) as u32,
            ),
            None => RegDeleteKeyValueW(HKEY_CURRENT_USER, &key, &name),
        }
    };
    if command.is_none() && status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    status.ok().context("Cannot update startup registration")
}
