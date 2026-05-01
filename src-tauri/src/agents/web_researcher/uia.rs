// ============================================================================
// Office Hub – agents/web_researcher/uia.rs
//
// UI Automation (UIA) Bindings for Web Researcher Agent
// ============================================================================

use windows::core::VARIANT;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, UIA_ControlTypePropertyId, UIA_IsOffscreenPropertyId,
    TreeScope_Descendants, UIA_DocumentControlTypeId,
    IUIAutomationGridPattern, UIA_GridPatternId,
    UIA_TableControlTypeId, UIA_DataGridControlTypeId,
    UIA_NamePropertyId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowTextW, IsWindowVisible, GetWindowRect
};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    GetDIBits, ReleaseDC, SelectObject, BitBlt, SRCCOPY, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS,
};
use windows::core::Interface;

pub struct UiaCore {
    uia: IUIAutomation,
}

unsafe impl Send for UiaCore {}
unsafe impl Sync for UiaCore {}

impl UiaCore {
    /// Initialize COM and create the UIA root interface.
    pub fn new() -> anyhow::Result<Self> {
        unsafe {
            // Attempt to initialize COM. It might already be initialized on this thread.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| anyhow::anyhow!("Failed to create CUIAutomation: {}", e))?;
                
            Ok(Self { uia })
        }
    }

    /// Finds a visible browser window (Edge or Chrome).
    pub fn find_browser_window(&self) -> anyhow::Result<HWND> {
        let mut browser_hwnd: Option<HWND> = None;
        
        unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> windows::Win32::Foundation::BOOL {
            let browser_hwnd_ptr = lparam.0 as *mut Option<HWND>;
            
            if !IsWindowVisible(hwnd).as_bool() {
                return true.into();
            }
            
            let mut class_name = [0u16; 256];
            let len = GetClassNameW(hwnd, &mut class_name);
            if len > 0 {
                let name = String::from_utf16_lossy(&class_name[..len as usize]);
                // Chrome and Edge use "Chrome_WidgetWin_1"
                if name == "Chrome_WidgetWin_1" {
                    // Check window title to ensure it's the main browser window (not a tooltip or hidden overlay)
                    let mut title = [0u16; 512];
                    let title_len = GetWindowTextW(hwnd, &mut title);
                    if title_len > 0 {
                        // Found a valid browser window
                        unsafe { *browser_hwnd_ptr = Some(hwnd); }
                        return false.into(); // Stop enumeration
                    }
                }
            }
            true.into()
        }

        unsafe {
            let _ = EnumWindows(Some(enum_windows_proc), LPARAM(&mut browser_hwnd as *mut _ as isize));
        }

        browser_hwnd.ok_or_else(|| anyhow::anyhow!("No visible Edge/Chrome browser window found"))
    }

    /// Extracts all readable text from the browser's Document element.
    pub fn extract_browser_text(&self, hwnd: HWND) -> anyhow::Result<String> {
        unsafe {
            let root_element: IUIAutomationElement = self.uia.ElementFromHandle(hwnd)
                .map_err(|e| anyhow::anyhow!("ElementFromHandle failed: {}", e))?;

            // We need to find the "Document" control type which contains the web page content
            let var_val = VARIANT::from(UIA_DocumentControlTypeId.0);
            let document_condition = self.uia.CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &var_val,
            )?;

            // Tối ưu hoá: Tạo CacheRequest để lấy trước tất cả Properties và Descendants
            // Điều này giảm thiểu hàng nghìn IPC calls xuống chỉ còn 1 call duy nhất.
            let cache_req = self.uia.CreateCacheRequest()?;
            cache_req.AddProperty(UIA_NamePropertyId)?;
            cache_req.AddProperty(UIA_IsOffscreenPropertyId)?;
            cache_req.SetTreeScope(TreeScope_Descendants)?;

            // Find the main document and build cache
            let document_element = root_element.FindFirstBuildCache(TreeScope_Descendants, &document_condition, &cache_req)
                .map_err(|e| anyhow::anyhow!("Could not find browser document element: {}", e))?;

            // Now extract text recursively using ONLY the cached data
            let mut extracted_text = String::new();
            self.extract_text_recursive_cached(&document_element, &mut extracted_text)?;
            
            Ok(extracted_text)
        }
    }

    unsafe fn extract_text_recursive_cached(&self, element: &IUIAutomationElement, out_text: &mut String) -> anyhow::Result<()> {
        // Skip offscreen elements
        let is_offscreen = element.GetCachedPropertyValue(UIA_IsOffscreenPropertyId)?;
        if let Ok(offscreen_bool) = bool::try_from(&is_offscreen) {
            if offscreen_bool {
                return Ok(());
            }
        }

        // Use CachedName instead of CurrentName
        let name_bstr = element.CachedName();
        if let Ok(name) = name_bstr {
            if !name.is_empty() {
                out_text.push_str(&name.to_string());
                out_text.push('\n');
            }
        }

        // Walk children through the cached collection, no IPC!
        if let Ok(children) = element.GetCachedChildren() {
            let count = children.Length().unwrap_or(0);
            for i in 0..count {
                if let Ok(child) = children.GetElement(i) {
                    let _ = self.extract_text_recursive_cached(&child, out_text);
                }
            }
        }

        Ok(())
    }

    /// Extracts tables from the browser document
    pub fn extract_browser_tables(&self, hwnd: HWND) -> anyhow::Result<Vec<Vec<Vec<String>>>> {
        let mut tables = Vec::new();
        unsafe {
            let root_element = self.uia.ElementFromHandle(hwnd)
                .map_err(|e| anyhow::anyhow!("ElementFromHandle failed: {}", e))?;

            let var_val = VARIANT::from(UIA_DocumentControlTypeId.0);
            let document_condition = self.uia.CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &var_val,
            )?;

            let document_element = root_element.FindFirst(TreeScope_Descendants, &document_condition)
                .map_err(|e| anyhow::anyhow!("Could not find browser document element: {}", e))?;
                
            let table_condition1 = self.uia.CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_TableControlTypeId.0),
            )?;
            let table_condition2 = self.uia.CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_DataGridControlTypeId.0),
            )?;
            let table_condition = self.uia.CreateOrCondition(&table_condition1, &table_condition2)?;

            // Use TreeWalker instead of FindAll to reduce overhead, combined with CacheRequest
            let walker = self.uia.CreateTreeWalker(&table_condition)?;
            
            let cache_req = self.uia.CreateCacheRequest()?;
            cache_req.AddPattern(UIA_GridPatternId)?;
            
            let mut element_res = walker.GetFirstChildElementBuildCache(&document_element, &cache_req);
            
            while let Ok(table_el) = element_res {
                // Get Cached Grid Pattern instead of Current
                if let Ok(pattern_obj) = table_el.GetCachedPattern(UIA_GridPatternId) {
                    if let Ok(grid) = pattern_obj.cast::<IUIAutomationGridPattern>() {
                        if let (Ok(rows), Ok(cols)) = (grid.CurrentRowCount(), grid.CurrentColumnCount()) {
                            let mut table_data = Vec::new();
                            for r in 0..rows {
                                let mut row_data = Vec::new();
                                for c in 0..cols {
                                    if let Ok(cell) = grid.GetItem(r, c) {
                                        let mut text = String::new();
                                        let _ = self.extract_text_recursive_cached(&cell, &mut text);
                                        row_data.push(text.trim().to_string());
                                    } else {
                                        row_data.push("".to_string());
                                    }
                                }
                                table_data.push(row_data);
                            }
                            tables.push(table_data);
                        }
                    }
                }
                
                element_res = walker.GetNextSiblingElementBuildCache(&table_el, &cache_req);
            }
        }
        Ok(tables)
    }

    /// Captures a screenshot of the specified window and saves it to a file
    pub fn capture_screenshot(&self, hwnd: HWND, output_path: &str) -> anyhow::Result<()> {
        unsafe {
            let mut rect = windows::Win32::Foundation::RECT::default();
            GetWindowRect(hwnd, &mut rect)?;
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            
            if width <= 0 || height <= 0 {
                return Err(anyhow::anyhow!("Invalid window size"));
            }

            let hdc_screen = GetDC(HWND::default());
            let hdc_mem = CreateCompatibleDC(hdc_screen);
            let hbitmap = CreateCompatibleBitmap(hdc_screen, width, height);
            let hbitmap_old = SelectObject(hdc_mem, hbitmap);

            // Copy from screen to memory DC
            BitBlt(
                hdc_mem, 0, 0, width, height, 
                hdc_screen, rect.left, rect.top, 
                SRCCOPY
            )?;

            // Prepare BITMAPINFO
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width;
            bmi.bmiHeader.biHeight = -height; // Top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB.0;

            let mut pixels: Vec<u8> = vec![0; (width * height * 4) as usize];
            GetDIBits(
                hdc_mem, hbitmap, 0, height as u32, 
                Some(pixels.as_mut_ptr() as *mut _), 
                &mut bmi, DIB_RGB_COLORS
            );

            // Clean up GDI objects
            SelectObject(hdc_mem, hbitmap_old);
            let _ = DeleteObject(hbitmap);
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(HWND::default(), hdc_screen);

            // Convert BGRA to RGBA
            for chunk in pixels.chunks_exact_mut(4) {
                chunk.swap(0, 2);
                chunk[3] = 255; // Alpha
            }

            image::save_buffer(
                output_path,
                &pixels,
                width as u32,
                height as u32,
                image::ColorType::Rgba8
            )?;
        }
        Ok(())
    }
}
