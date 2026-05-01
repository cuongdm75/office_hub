// ============================================================================
// Office Hub – agents/office_master/com_excel.rs
//
// Excel COM Automation
// ============================================================================

#[cfg(windows)]
use windows::{
    core::BSTR,
    Win32::System::Com::{
        CLSIDFromProgID, CoCreateInstance, CoInitializeEx, IDispatch, CLSCTX_LOCAL_SERVER,
        COINIT_APARTMENTTHREADED,
    },
};

#[cfg(windows)]
use crate::agents::com_utils::dispatch::{var_bool, var_bstr, var_i4, var_r4, ComObject};

pub struct ExcelApplication {
    #[cfg(windows)]
    pub app: ComObject,
}

impl ExcelApplication {
    #[cfg(windows)]
    pub fn connect_or_launch() -> anyhow::Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let clsid = CLSIDFromProgID(windows::core::w!("Excel.Application"))
                .map_err(|e| anyhow::anyhow!("Excel.Application not registered: {}", e))?;

            // Try to connect to existing or create new
            let dispatch: IDispatch = CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER)
                .map_err(|e| anyhow::anyhow!("CoCreateInstance Excel failed: {}", e))?;

            let app = ComObject::new(dispatch);
            let _ = app.set_property("Visible", var_bool(true));
            Ok(Self { app })
        }
    }

    #[cfg(not(windows))]
    pub fn connect_or_launch() -> anyhow::Result<Self> {
        anyhow::bail!("Excel COM automation not available on non-Windows")
    }

    /// Insert text into the active cell
    #[cfg(windows)]
    pub fn insert_text_at_cursor(&self, text: &str) -> anyhow::Result<String> {
        let active_cell = self
            .app
            .get_property_obj("ActiveCell")
            .map_err(|_| anyhow::anyhow!("No active Excel cell"))?;

        active_cell.set_property("Value", var_bstr(text))?;

        Ok(format!(
            "Inserted {} characters into ActiveCell",
            text.len()
        ))
    }

    #[cfg(not(windows))]
    pub fn insert_text_at_cursor(&self, _text: &str) -> anyhow::Result<String> {
        Ok("Excel COM Automation is only supported on Windows".to_string())
    }

    /// Replace the entire content of the active sheet
    #[cfg(windows)]
    pub fn replace_active_document(&self, text: &str) -> anyhow::Result<String> {
        let active_sheet = self
            .app
            .get_property_obj("ActiveSheet")
            .map_err(|_| anyhow::anyhow!("No active Excel sheet open"))?;

        // Clear existing cells
        let cells = active_sheet.get_property_obj("Cells")?;
        cells.invoke_method("Clear", vec![])?;

        // Just dump the text into A1 for now
        let a1_var = cells
            .invoke_method("Item", vec![var_i4(1), var_i4(1)])
            .map_err(|_| anyhow::anyhow!("Could not invoke Item on Cells"))?;
        let a1 = ComObject::new(
            IDispatch::try_from(&a1_var).map_err(|_| anyhow::anyhow!("Item is not an object"))?,
        );

        a1.set_property("Value", var_bstr(text))?;

        let sheet_name = active_sheet
            .get_property("Name")
            .ok()
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string())
            .unwrap_or_else(|| "sheet".to_string());

        Ok(format!("Replaced entire content of sheet '{}'", sheet_name))
    }

    #[cfg(not(windows))]
    pub fn replace_active_document(&self, _text: &str) -> anyhow::Result<String> {
        Ok("Excel COM Automation is only supported on Windows".to_string())
    }

    /// Save the active workbook
    #[cfg(windows)]
    pub fn save_active_document(&self) -> anyhow::Result<String> {
        let active_wb = self
            .app
            .get_property_obj("ActiveWorkbook")
            .map_err(|_| anyhow::anyhow!("No active Excel workbook open"))?;

        active_wb.invoke_method("Save", vec![])?;

        let doc_name = active_wb
            .get_property("Name")
            .ok()
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string())
            .unwrap_or_else(|| "workbook".to_string());

        Ok(format!("Saved active workbook '{}'", doc_name))
    }

    #[cfg(not(windows))]
    pub fn save_active_document(&self) -> anyhow::Result<String> {
        Ok("Excel COM Automation is only supported on Windows".to_string())
    }

    /// Extract text from the active workbook (using clipboard copy of UsedRange)
    #[cfg(windows)]
    pub fn extract_active_document(&self) -> anyhow::Result<String> {
        let active_sheet = self
            .app
            .get_property_obj("ActiveSheet")
            .map_err(|_| anyhow::anyhow!("No active Excel sheet open"))?;

        let used_range = active_sheet
            .get_property_obj("UsedRange")
            .map_err(|_| anyhow::anyhow!("Failed to get UsedRange"))?;

        // A quick hack for SafeArray: copy to clipboard and read it, or save to CSV
        // Here we copy to clipboard. Note: This destroys user's clipboard content!
        used_range.invoke_method("Copy", vec![])?;

        let workbooks = self.app.get_property_obj("Workbooks")?;
        let new_wb = workbooks.invoke_method("Add", vec![])?;
        let new_wb_obj = ComObject::new(IDispatch::try_from(&new_wb)?);

        let new_sheet = new_wb_obj.get_property_obj("ActiveSheet")?;
        // Paste special or just Paste
        new_sheet.invoke_method("Paste", vec![])?;

        let temp_csv = format!(
            "{}\\temp_excel_{}.csv",
            std::env::var("TEMP").unwrap_or_else(|_| ".".to_string()),
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        );

        // xlCSV = 6
        new_wb_obj.invoke_method("SaveAs", vec![var_bstr(&temp_csv), var_i4(6)])?;
        new_wb_obj.invoke_method("Close", vec![var_bool(false)])?;

        let text = std::fs::read_to_string(&temp_csv).unwrap_or_default();
        let _ = std::fs::remove_file(&temp_csv);

        Ok(text)
    }

    #[cfg(not(windows))]
    pub fn extract_active_document(&self) -> anyhow::Result<String> {
        Ok("Excel COM Automation is only supported on Windows".to_string())
    }

    /// Insert an image into the active sheet
    #[cfg(windows)]
    pub fn add_picture(
        &self,
        image_path: &str,
        left: f32,
        top: f32,
        width: f32,
        height: f32,
    ) -> anyhow::Result<String> {
        let active_sheet = self
            .app
            .get_property_obj("ActiveSheet")
            .map_err(|_| anyhow::anyhow!("No active Excel sheet open"))?;

        let shapes = active_sheet.get_property_obj("Shapes")?;

        let _shape_var = shapes
            .invoke_method(
                "AddPicture",
                vec![
                    var_bstr(image_path),
                    var_bool(false), // LinkToFile
                    var_bool(true),  // SaveWithDocument
                    var_r4(left),
                    var_r4(top),
                    if width > 0.0 {
                        var_r4(width)
                    } else {
                        var_i4(-1)
                    }, // -1 means keep original width if needed, but COM usually prefers explicit
                    if height > 0.0 {
                        var_r4(height)
                    } else {
                        var_i4(-1)
                    },
                ],
            )
            .map_err(|e| anyhow::anyhow!("Failed to AddPicture to Excel: {}", e))?;

        Ok(format!("Chèn ảnh thành công vào Excel từ {}", image_path))
    }

    #[cfg(not(windows))]
    pub fn add_picture(
        &self,
        _image_path: &str,
        _left: f32,
        _top: f32,
        _width: f32,
        _height: f32,
    ) -> anyhow::Result<String> {
        Ok("Excel COM Automation is only supported on Windows".to_string())
    }
}
