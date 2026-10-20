#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
pub(super) struct WindowsModule(windows_sys::Win32::Foundation::HMODULE);

#[cfg(windows)]
impl WindowsModule {
    pub(super) fn load(path: &Path) -> std::io::Result<Self> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::System::LibraryLoader::{
            LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LoadLibraryExW,
        };
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let handle = unsafe {
            LoadLibraryExW(
                path.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            )
        };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for WindowsModule {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::FreeLibrary(self.0);
        }
    }
}
