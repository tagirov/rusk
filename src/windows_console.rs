/// Windows console initialization for ANSI color support
#[cfg(windows)]
pub fn enable_ansi_support() {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, SetConsoleMode, GetStdHandle, 
        STD_OUTPUT_HANDLE, ENABLE_VIRTUAL_TERMINAL_PROCESSING
    };

    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if !handle.is_null() && handle != (-1i32 as *mut _) {
            let mut mode = 0;
            if GetConsoleMode(handle, &mut mode) != 0 {
                let new_mode = mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
                SetConsoleMode(handle, new_mode);
            }
        }
    }
}

#[cfg(not(windows))]
pub fn enable_ansi_support() {
    // No-op on non-Windows platforms
}
