// ============================================================================
// Office Hub – agents/com_utils.rs
//
// Shared helpers for IDispatch COM Automation in Rust.
// ============================================================================

#[cfg(windows)]
pub mod dispatch {
    use windows::core::{BSTR, PCWSTR, VARIANT};
    use windows::Win32::System::Com::{
        IDispatch, DISPATCH_FLAGS, DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT,
        DISPPARAMS, EXCEPINFO,
    };

    use windows::Win32::System::Ole::DISPID_PROPERTYPUT;

    pub struct ComObject {
        pub dispatch: IDispatch,
    }

    impl ComObject {
        pub fn new(dispatch: IDispatch) -> Self {
            Self { dispatch }
        }

        pub fn get_id_of_name(&self, name: &str) -> anyhow::Result<i32> {
            let mut name_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let mut dispid: i32 = 0;
            let mut name_pwstr = PCWSTR(name_wide.as_mut_ptr());

            unsafe {
                self.dispatch
                    .GetIDsOfNames(
                        &windows::core::GUID::zeroed(),
                        &name_pwstr,
                        1,
                        0x0400, // LOCALE_USER_DEFAULT
                        &mut dispid,
                    )
                    .map_err(|e| anyhow::anyhow!("GetIDsOfNames failed for {}: {}", name, e))?;
            }
            Ok(dispid)
        }

        pub fn invoke_method(&self, name: &str, mut args: Vec<VARIANT>) -> anyhow::Result<VARIANT> {
            let dispid = self.get_id_of_name(name)?;
            // IDispatch expects arguments in reverse order
            args.reverse();
            let mut dispparams = DISPPARAMS {
                rgvarg: args.as_mut_ptr(),
                rgdispidNamedArgs: std::ptr::null_mut(),
                cArgs: args.len() as u32,
                cNamedArgs: 0,
            };
            self.invoke_raw(dispid, DISPATCH_METHOD, &mut dispparams)
        }

        pub fn get_property(&self, name: &str) -> anyhow::Result<VARIANT> {
            let dispid = self.get_id_of_name(name)?;
            let mut dispparams = DISPPARAMS::default();
            self.invoke_raw(dispid, DISPATCH_PROPERTYGET, &mut dispparams)
        }

        pub fn get_property_obj(&self, name: &str) -> anyhow::Result<ComObject> {
            let var = self.get_property(name)?;
            let disp: IDispatch = core::convert::TryFrom::try_from(&var)
                .map_err(|e| anyhow::anyhow!("Property {} is not an object: {}", name, e))?;
            Ok(ComObject::new(disp))
        }

        pub fn set_property(&self, name: &str, mut value: VARIANT) -> anyhow::Result<()> {
            let dispid = self.get_id_of_name(name)?;
            let mut dispid_put = DISPID_PROPERTYPUT;
            let mut dispparams = DISPPARAMS {
                rgvarg: &mut value,
                rgdispidNamedArgs: &mut dispid_put,
                cArgs: 1,
                cNamedArgs: 1,
            };
            self.invoke_raw(dispid, DISPATCH_PROPERTYPUT, &mut dispparams)?;
            Ok(())
        }

        fn invoke_raw(
            &self,
            dispid: i32,
            flags: DISPATCH_FLAGS,
            dispparams: &mut DISPPARAMS,
        ) -> anyhow::Result<VARIANT> {
            let mut result = VARIANT::default();
            let mut excepinfo = EXCEPINFO::default();
            let mut argerr = 0u32;

            unsafe {
                self.dispatch
                    .Invoke(
                        dispid,
                        &windows::core::GUID::zeroed(),
                        0x0400, // LOCALE_USER_DEFAULT
                        flags,
                        dispparams,
                        Some(&mut result),
                        Some(&mut excepinfo),
                        Some(&mut argerr),
                    )
                    .map_err(|e| anyhow::anyhow!("Invoke failed: {}", e))?;
            }
            Ok(result)
        }
    }

    // Helpers to create VARIANTs
    pub fn var_i4(v: i32) -> VARIANT {
        VARIANT::from(v)
    }

    pub fn var_bstr(s: &str) -> VARIANT {
        VARIANT::from(BSTR::from(s))
    }

    pub fn var_bool(b: bool) -> VARIANT {
        VARIANT::from(b)
    }

    pub fn var_r4(f: f32) -> VARIANT {
        VARIANT::from(f)
    }

    pub fn var_optional() -> VARIANT {
        VARIANT::default()
    }
}

// Non-windows stub module so the crate still compiles cross-platform
#[cfg(not(windows))]
pub mod dispatch {
    #[derive(Debug)]
    pub struct ComObject;
    impl ComObject {
        pub fn new(_: ()) -> Self {
            Self
        }
    }
    pub fn var_i4(_: i32) {}
    pub fn var_bstr(_: &str) {}
    pub fn var_bool(_: bool) {}
    pub fn var_r4(_: f32) {}
    pub fn var_optional() {}
}

#[cfg(windows)]
pub mod watchdog {
    use std::time::Duration;
    use tokio::time;
    use tracing::warn;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, PostMessageW, GWL_STYLE, WM_CLOSE, WS_POPUP,
    };

    /// Spawns a background task that periodically checks for and dismisses blocking Office dialogs.
    pub fn spawn_com_watchdog() {
        tauri::async_runtime::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                unsafe {
                    let _ = EnumWindows(Some(enum_windows_proc), LPARAM(0));
                }
            }
        });
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, _: LPARAM) -> BOOL {
        let mut class_name = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut class_name);
        if len > 0 {
            let class_str = String::from_utf16_lossy(&class_name[..len as usize]);
            if class_str == "#32770" {
                let mut process_id = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut process_id));
                if process_id != 0 {
                    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)
                        .unwrap_or_default();
                    if !handle.is_invalid() {
                        let mut exe_name = [0u16; 1024];
                        let mut size = exe_name.len() as u32;
                        // Using QueryFullProcessImageNameW
                        let res = windows::Win32::System::Threading::QueryFullProcessImageNameW(
                            handle,
                            windows::Win32::System::Threading::PROCESS_NAME_FORMAT(0),
                            windows::core::PWSTR(exe_name.as_mut_ptr()),
                            &mut size,
                        );
                        CloseHandle(handle).ok();

                        if res.is_ok() && size > 0 {
                            let path_str =
                                String::from_utf16_lossy(&exe_name[..size as usize]).to_lowercase();
                            if path_str.ends_with("excel.exe")
                                || path_str.ends_with("winword.exe")
                                || path_str.ends_with("powerpnt.exe")
                            {
                                // Additional checks: is it a popup, and what is its title?
                                let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
                                if (style & WS_POPUP.0) == WS_POPUP.0 {
                                    let mut title_lower = String::new();
                                    let text_len = GetWindowTextLengthW(hwnd);
                                    if text_len > 0 {
                                        let mut buffer = vec![0u16; (text_len + 1) as usize];
                                        let copied = GetWindowTextW(hwnd, &mut buffer);
                                        if copied > 0 {
                                            title_lower = String::from_utf16_lossy(
                                                &buffer[..copied as usize],
                                            )
                                            .to_lowercase();
                                        }
                                    }

                                    // Close if it's a known blocking dialog or if we can't read the title
                                    let should_close = title_lower.is_empty()
                                        || title_lower.contains("save as")
                                        || title_lower.contains("error")
                                        || title_lower.contains("microsoft excel")
                                        || title_lower.contains("microsoft word");

                                    if should_close {
                                        warn!("Detected blocking COM dialog (#32770, WS_POPUP) from {}. Title: '{}'. Attempting WM_CLOSE...", path_str, title_lower);
                                        let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        BOOL::from(true)
    }
}

#[cfg(not(windows))]
pub mod watchdog {
    pub fn spawn_com_watchdog() {
        // No-op on non-Windows
    }
}
