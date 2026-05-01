// ============================================================================
// Office Hub – agents/office_master/com_ppt.rs
//
// PowerPoint COM Automation – Phase 3 Real Implementation
// ============================================================================

#[cfg(windows)]
use windows::{
    core::BSTR,
    Win32::System::Com::{
        CLSIDFromProgID, CoCreateInstance, CoInitializeEx, IDispatch,
        CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED,
    },
};

#[cfg(windows)]
use crate::agents::com_utils::dispatch::{var_bool, var_bstr, var_i4, var_r4, var_optional, ComObject};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Metadata of an existing presentation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PresentationInfo {
    pub file_path: String,
    pub slide_count: usize,
    pub slide_titles: Vec<String>,
}

/// A slide to add when building a presentation from outline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SlideSpec {
    /// Slide title text (goes into placeholder index 1 or shape named "Title")
    pub title: String,
    /// Body text lines (joined with \r for PowerPoint newline)
    pub body_lines: Vec<String>,
    /// PPT layout constant (1 = ppLayoutTitle, 2 = ppLayoutTitleBody, 7 = ppLayoutBlank)
    pub layout: i32,
}

// ─────────────────────────────────────────────────────────────────────────────
// PowerPointApplication
// ─────────────────────────────────────────────────────────────────────────────

pub struct PowerPointApplication {
    #[cfg(windows)]
    pub app: ComObject,
}

impl PowerPointApplication {
    // ── Connect ───────────────────────────────────────────────────────────────

    #[cfg(windows)]
    pub fn connect_or_launch() -> anyhow::Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let clsid = CLSIDFromProgID(windows::core::w!("PowerPoint.Application"))
                .map_err(|e| anyhow::anyhow!("PowerPoint.Application not registered: {}", e))?;
            let dispatch: IDispatch = CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER)
                .map_err(|e| anyhow::anyhow!("CoCreateInstance PowerPoint failed: {}", e))?;
            let app = ComObject::new(dispatch);
            // PPT needs Visible = true or it stays hidden
            let _ = app.set_property("Visible", var_bool(true));
            Ok(Self { app })
        }
    }

    #[cfg(not(windows))]
    pub fn connect_or_launch() -> anyhow::Result<Self> {
        anyhow::bail!("PowerPoint COM automation not available on non-Windows")
    }

    // ── Inspect ───────────────────────────────────────────────────────────────

    /// Return slide count and slide titles from an existing presentation.
    #[cfg(windows)]
    pub fn inspect_presentation(&self, file_path: &str) -> anyhow::Result<PresentationInfo> {
        let pres = self.open_presentation(file_path)?;

        let slides = pres.get_property_obj("Slides")?;
        let count = i32::try_from(&slides.get_property("Count")?).unwrap_or(0) as usize;

        let mut titles = Vec::with_capacity(count);
        for i in 1..=(count as i32) {
            let title = self.get_slide_title(&slides, i).unwrap_or_else(|_| format!("Slide {}", i));
            titles.push(title);
        }

        self.close_presentation(&pres, false)?;

        Ok(PresentationInfo {
            file_path: file_path.to_string(),
            slide_count: count,
            slide_titles: titles,
        })
    }

    #[cfg(not(windows))]
    pub fn inspect_presentation(&self, file_path: &str) -> anyhow::Result<PresentationInfo> {
        Ok(PresentationInfo {
            file_path: file_path.to_string(),
            slide_count: 0,
            slide_titles: vec![],
        })
    }

    // ── Create ────────────────────────────────────────────────────────────────

    /// Create a presentation from an ordered list of `SlideSpec`, save to `output_path`.
    #[cfg(windows)]
    pub fn create_from_outline(
        &self,
        template_path: Option<&str>,
        slides: &[SlideSpec],
        output_path: &str,
        backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        let pres_obj = self.new_presentation()?;

        if let Some(tpl) = template_path {
            if std::path::Path::new(tpl).exists() {
                let _ = pres_obj.invoke_method("ApplyTemplate", vec![var_bstr(tpl)]);
            }
        }

        self.backup_presentation(&pres_obj, backup_dir)?;

        let slides_col = pres_obj.get_property_obj("Slides")?;

        // Remove the default blank slide that PPT adds on Presentations.Add
        let existing = i32::try_from(&slides_col.get_property("Count")?).unwrap_or(0);
        if existing > 0 && !slides.is_empty() {
            if let Ok(sv) = slides_col.invoke_method("Item", vec![var_i4(1)]) {
                if let Ok(sd) = IDispatch::try_from(&sv) {
                    let slide0 = ComObject::new(sd);
                    let _ = slide0.invoke_method("Delete", vec![]);
                }
            }
        }

        for (idx, spec) in slides.iter().enumerate() {
            let slide_var = slides_col.invoke_method("Add", vec![
                var_i4((idx + 1) as i32),
                var_i4(spec.layout),
            ])?;
            let slide = ComObject::new(
                IDispatch::try_from(&slide_var)
                    .map_err(|e| anyhow::anyhow!("Slide not object: {}", e))?,
            );

            let shapes = slide.get_property_obj("Shapes")?;

            // Try to set title (placeholder 1)
            self.set_placeholder_text(&shapes, 1, &spec.title).ok();

            // Try to set body (placeholder 2)
            if !spec.body_lines.is_empty() {
                let body = spec.body_lines.join("\r");
                self.set_placeholder_text(&shapes, 2, &body).ok();
            }
        }

        // ppSaveAsDefault = 11 (.pptx)
        pres_obj.invoke_method("SaveAs", vec![var_bstr(output_path), var_i4(11)])?;
        self.close_presentation(&pres_obj, false)?;

        Ok(format!(
            "Presentation saved to '{}' ({} slides)",
            output_path,
            slides.len()
        ))
    }

    #[cfg(not(windows))]
    pub fn create_from_outline(
        &self,
        _template_path: Option<&str>,
        _slides: &[SlideSpec],
        _output_path: &str,
        _backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        Ok("PowerPoint COM Automation is only supported on Windows".to_string())
    }

    /// Legacy single-content creation (used by ppt_create_presentation action).
    #[cfg(windows)]
    pub fn create_presentation_from_template(
        &self,
        template_path: Option<&str>,
        content: &str,
        backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        let spec = SlideSpec {
            title: content.lines().next().unwrap_or("Title").to_string(),
            body_lines: content.lines().skip(1).map(String::from).collect(),
            layout: 1, // ppLayoutTitle
        };
        let out = format!(
            "{}\\new_presentation_{}.pptx",
            std::env::var("TEMP").unwrap_or_else(|_| ".".to_string()),
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        );
        self.create_from_outline(template_path, &[spec], &out, backup_dir)
    }

    #[cfg(not(windows))]
    pub fn create_presentation_from_template(
        &self,
        _template_path: Option<&str>,
        _content: &str,
        _backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        Ok("PowerPoint COM Automation is only supported on Windows".to_string())
    }

    // ── Edit ─────────────────────────────────────────────────────────────────

    /// Update a shape's text by slide index and shape name or index.
    #[cfg(windows)]
    pub fn update_shape_text(
        &self,
        file_path: &str,
        slide_index: i32,
        shape_name_or_index: &str,
        new_text: &str,
        backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        let pres = self.open_presentation(file_path)?;
        self.backup_presentation(&pres, backup_dir)?;

        let slides = pres.get_property_obj("Slides")?;
        let slide_var = slides.invoke_method("Item", vec![var_i4(slide_index)])
            .map_err(|e| anyhow::anyhow!("Slide {} not found: {}", slide_index, e))?;
        let slide = ComObject::new(
            IDispatch::try_from(&slide_var)
                .map_err(|e| anyhow::anyhow!("Slide not object: {}", e))?,
        );
        let shapes = slide.get_property_obj("Shapes")?;

        // Try by name first, then by numeric index
        let shape_var = if let Ok(n) = shape_name_or_index.parse::<i32>() {
            shapes.invoke_method("Item", vec![var_i4(n)])
        } else {
            shapes.invoke_method("Item", vec![var_bstr(shape_name_or_index)])
        }.map_err(|e| anyhow::anyhow!("Shape '{}' not found: {}", shape_name_or_index, e))?;

        let shape = ComObject::new(
            IDispatch::try_from(&shape_var)
                .map_err(|e| anyhow::anyhow!("Shape not object: {}", e))?,
        );

        let tf = shape.get_property_obj("TextFrame")?;
        let tr = tf.get_property_obj("TextRange")?;
        tr.set_property("Text", var_bstr(new_text))?;

        // ppSaveAsDefault = 11
        pres.invoke_method("SaveAs", vec![var_bstr(file_path), var_i4(11)])?;
        self.close_presentation(&pres, false)?;

        Ok(format!(
            "Updated shape '{}' on slide {} in '{}'",
            shape_name_or_index, slide_index, file_path
        ))
    }

    #[cfg(not(windows))]
    pub fn update_shape_text(
        &self,
        _file_path: &str,
        _slide_index: i32,
        _shape_name_or_index: &str,
        _new_text: &str,
        _backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        Ok("PowerPoint COM Automation is only supported on Windows".to_string())
    }

    /// Add a new slide at a given index.
    #[cfg(windows)]
    pub fn add_slide(
        &self,
        file_path: &str,
        index: i32,
        spec: &SlideSpec,
        backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        let pres = self.open_presentation(file_path)?;
        self.backup_presentation(&pres, backup_dir)?;

        let slides = pres.get_property_obj("Slides")?;
        let slide_var = slides.invoke_method("Add", vec![var_i4(index), var_i4(spec.layout)])?;
        let slide = ComObject::new(
            IDispatch::try_from(&slide_var)
                .map_err(|e| anyhow::anyhow!("New slide not object: {}", e))?,
        );
        let shapes = slide.get_property_obj("Shapes")?;
        self.set_placeholder_text(&shapes, 1, &spec.title).ok();
        if !spec.body_lines.is_empty() {
            self.set_placeholder_text(&shapes, 2, &spec.body_lines.join("\r")).ok();
        }

        pres.invoke_method("SaveAs", vec![var_bstr(file_path), var_i4(11)])?;
        self.close_presentation(&pres, false)?;

        Ok(format!("Added slide at index {} in '{}'", index, file_path))
    }

    #[cfg(not(windows))]
    pub fn add_slide(
        &self,
        _file_path: &str,
        _index: i32,
        _spec: &SlideSpec,
        _backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        Ok("PowerPoint COM Automation is only supported on Windows".to_string())
    }

    /// Delete slide at given index.
    #[cfg(windows)]
    pub fn delete_slide(
        &self,
        file_path: &str,
        slide_index: i32,
        backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        let pres = self.open_presentation(file_path)?;
        self.backup_presentation(&pres, backup_dir)?;

        let slides = pres.get_property_obj("Slides")?;
        let sv = slides.invoke_method("Item", vec![var_i4(slide_index)])
            .map_err(|e| anyhow::anyhow!("Slide {} not found: {}", slide_index, e))?;
        let slide = ComObject::new(IDispatch::try_from(&sv)?);
        slide.invoke_method("Delete", vec![])?;

        pres.invoke_method("SaveAs", vec![var_bstr(file_path), var_i4(11)])?;
        self.close_presentation(&pres, false)?;
        Ok(format!("Deleted slide {} from '{}'", slide_index, file_path))
    }

    #[cfg(not(windows))]
    pub fn delete_slide(
        &self,
        _file_path: &str,
        _slide_index: i32,
        _backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        Ok("PowerPoint COM Automation is only supported on Windows".to_string())
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    #[cfg(windows)]
    fn new_presentation(&self) -> anyhow::Result<ComObject> {
        let pres_col = self.app.get_property_obj("Presentations")?;
        let pv = pres_col.invoke_method("Add", vec![var_bool(true)])?;
        let disp = IDispatch::try_from(&pv)
            .map_err(|e| anyhow::anyhow!("Presentations.Add non-object: {}", e))?;
        Ok(ComObject::new(disp))
    }

    #[cfg(windows)]
    fn open_presentation(&self, file_path: &str) -> anyhow::Result<ComObject> {
        let pres_col = self.app.get_property_obj("Presentations")?;

        // Check if already open
        let count = i32::try_from(&pres_col.get_property("Count")?).unwrap_or(0);
        for i in 1..=count {
            if let Ok(pv) = pres_col.invoke_method("Item", vec![var_i4(i)]) {
                if let Ok(disp) = IDispatch::try_from(&pv) {
                    let p = ComObject::new(disp);
                    let name = p.get_property("FullName").ok()
                        .and_then(|v| BSTR::try_from(&v).ok())
                        .map(|b| b.to_string())
                        .unwrap_or_default();
                    if name.to_lowercase() == file_path.to_lowercase() {
                        return Ok(p);
                    }
                }
            }
        }

        let pv = pres_col.invoke_method("Open", vec![
            var_bstr(file_path),
            var_optional(), // ReadOnly
            var_optional(), // Untitled
            var_bool(true), // WithWindow
        ]).map_err(|e| anyhow::anyhow!("Presentations.Open('{}') failed: {}", file_path, e))?;

        let disp = IDispatch::try_from(&pv)
            .map_err(|e| anyhow::anyhow!("Open returned non-object: {}", e))?;
        Ok(ComObject::new(disp))
    }

    #[cfg(windows)]
    fn close_presentation(&self, pres: &ComObject, save: bool) -> anyhow::Result<()> {
        if save {
            let _ = pres.invoke_method("Save", vec![]);
        }
        let _ = pres.invoke_method("Close", vec![]);
        Ok(())
    }

    #[cfg(windows)]
    fn backup_presentation(&self, pres: &ComObject, backup_dir: Option<&str>) -> anyhow::Result<()> {
        let full_name = pres.get_property("FullName").ok()
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string())
            .unwrap_or_default();

        if full_name.is_empty() || (!full_name.contains('\\') && !full_name.contains('/')) {
            return Ok(()); // New unsaved presentation
        }

        let path = std::path::Path::new(&full_name);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("presentation");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("pptx");
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");

        let backup_path = if let Some(dir) = backup_dir {
            let _ = std::fs::create_dir_all(dir);
            format!("{}/{}_backup_{}.{}", dir, stem, ts, ext)
        } else {
            let parent = path.parent().and_then(|p| p.to_str()).unwrap_or(".");
            format!("{}/{}_backup_{}.{}", parent, stem, ts, ext)
        };

        let _ = pres.invoke_method("SaveCopyAs", vec![var_bstr(&backup_path)]);
        tracing::info!("Backed up presentation → {}", backup_path);
        Ok(())
    }

    /// Get the title of a slide from its Shapes/Placeholders.
    #[cfg(windows)]
    fn get_slide_title(&self, slides_col: &ComObject, index: i32) -> anyhow::Result<String> {
        let sv = slides_col.invoke_method("Item", vec![var_i4(index)])?;
        let slide = ComObject::new(IDispatch::try_from(&sv)?);
        let shapes = slide.get_property_obj("Shapes")?;

        // Try placeholder 1 (title)
        if let Ok(ph) = shapes.invoke_method("Item", vec![var_i4(1)]) {
            if let Ok(sd) = IDispatch::try_from(&ph) {
                let shape = ComObject::new(sd);
                if let Ok(tf) = shape.get_property_obj("TextFrame") {
                    if let Ok(tr) = tf.get_property_obj("TextRange") {
                        if let Ok(tv) = tr.get_property("Text") {
                            if let Ok(bstr) = BSTR::try_from(&tv) {
                                return Ok(bstr.to_string());
                            }
                        }
                    }
                }
            }
        }
        Ok(format!("Slide {}", index))
    }

    /// Set text in a placeholder by index (1-based).
    #[cfg(windows)]
    fn set_placeholder_text(
        &self,
        shapes: &ComObject,
        placeholder_idx: i32,
        text: &str,
    ) -> anyhow::Result<()> {
        let ph_var = shapes.invoke_method("Item", vec![var_i4(placeholder_idx)])?;
        let shape = ComObject::new(IDispatch::try_from(&ph_var)?);
        let tf = shape.get_property_obj("TextFrame")?;
        let tr = tf.get_property_obj("TextRange")?;
        tr.set_property("Text", var_bstr(text))?;
        Ok(())
    }

    // ── ActivePresentation methods ─────────────────────────────────────────────

    /// Insert text into the active presentation by adding a new slide at the end
    #[cfg(windows)]
    pub fn insert_text_at_cursor(&self, text: &str) -> anyhow::Result<String> {
        let active_pres = self.app.get_property_obj("ActivePresentation")
            .map_err(|_| anyhow::anyhow!("No active PowerPoint presentation open"))?;
        
        let slides = active_pres.get_property_obj("Slides")?;
        let count = i32::try_from(&slides.get_property("Count")?).unwrap_or(0);
        
        // ppLayoutText = 2
        let slide_var = slides.invoke_method("Add", vec![var_i4(count + 1), var_i4(2)])?;
        let slide = ComObject::new(IDispatch::try_from(&slide_var)?);
        let shapes = slide.get_property_obj("Shapes")?;
        
        // Title
        self.set_placeholder_text(&shapes, 1, "New Slide").ok();
        // Body
        self.set_placeholder_text(&shapes, 2, text).ok();

        Ok(format!("Added new slide with text at position {}", count + 1))
    }

    #[cfg(not(windows))]
    pub fn insert_text_at_cursor(&self, _text: &str) -> anyhow::Result<String> {
        Ok("PowerPoint COM Automation is only supported on Windows".to_string())
    }

    /// Insert a picture into the active slide
    #[cfg(windows)]
    pub fn add_picture(&self, file_path: &str, slide_index: i32, left: f32, top: f32, width: f32, height: f32) -> anyhow::Result<String> {
        let active_pres = self.app.get_property_obj("ActivePresentation")
            .map_err(|_| anyhow::anyhow!("No active PowerPoint presentation open"))?;

        let slides = active_pres.get_property_obj("Slides")?;
        let slide_var = slides.invoke_method("Item", vec![var_i4(slide_index)])
            .map_err(|_| anyhow::anyhow!("Slide {} not found", slide_index))?;
            
        let slide = ComObject::new(IDispatch::try_from(&slide_var)?);
        let shapes = slide.get_property_obj("Shapes")?;
        
        let path_bstr = var_bstr(file_path);
        let link_to_file = var_i4(0); // msoFalse
        let save_with_document = var_i4(-1); // msoTrue
        
        let _shape_var = shapes.invoke_method("AddPicture", vec![
            path_bstr,
            link_to_file,
            save_with_document,
            var_r4(left),
            var_r4(top),
            var_r4(width),
            var_r4(height)
        ])?;
        
        Ok(format!("Inserted picture '{}' into slide {}", file_path, slide_index))
    }

    #[cfg(not(windows))]
    pub fn add_picture(&self, _file_path: &str, _slide_index: i32, _left: f32, _top: f32, _width: f32, _height: f32) -> anyhow::Result<String> {
        Ok("PowerPoint COM Automation is only supported on Windows".to_string())
    }

    /// Replace the entire content of the active presentation (deletes all slides, adds a new one)
    #[cfg(windows)]
    pub fn replace_active_document(&self, text: &str) -> anyhow::Result<String> {
        let active_pres = self.app.get_property_obj("ActivePresentation")
            .map_err(|_| anyhow::anyhow!("No active PowerPoint presentation open"))?;

        let slides = active_pres.get_property_obj("Slides")?;
        let count = i32::try_from(&slides.get_property("Count")?).unwrap_or(0);

        // Delete all existing slides from back to front
        for i in (1..=count).rev() {
            if let Ok(sv) = slides.invoke_method("Item", vec![var_i4(i)]) {
                if let Ok(sd) = IDispatch::try_from(&sv) {
                    let slide = ComObject::new(sd);
                    let _ = slide.invoke_method("Delete", vec![]);
                }
            }
        }

        // Add one slide
        let slide_var = slides.invoke_method("Add", vec![var_i4(1), var_i4(2)])?;
        let slide = ComObject::new(IDispatch::try_from(&slide_var)?);
        let shapes = slide.get_property_obj("Shapes")?;
        
        self.set_placeholder_text(&shapes, 1, "Replaced Content").ok();
        self.set_placeholder_text(&shapes, 2, text).ok();

        Ok("Replaced entire presentation with new content".to_string())
    }

    #[cfg(not(windows))]
    pub fn replace_active_document(&self, _text: &str) -> anyhow::Result<String> {
        Ok("PowerPoint COM Automation is only supported on Windows".to_string())
    }

    /// Save the active presentation
    #[cfg(windows)]
    pub fn save_active_document(&self) -> anyhow::Result<String> {
        let active_pres = self.app.get_property_obj("ActivePresentation")
            .map_err(|_| anyhow::anyhow!("No active PowerPoint presentation open"))?;

        active_pres.invoke_method("Save", vec![])?;

        let doc_name = active_pres.get_property("Name").ok()
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string())
            .unwrap_or_else(|| "presentation".to_string());

        Ok(format!("Saved active presentation '{}'", doc_name))
    }

    #[cfg(not(windows))]
    pub fn save_active_document(&self) -> anyhow::Result<String> {
        Ok("PowerPoint COM Automation is only supported on Windows".to_string())
    }

    /// Extract text from all slides in the active presentation
    #[cfg(windows)]
    pub fn extract_active_document(&self) -> anyhow::Result<String> {
        let active_pres = self.app.get_property_obj("ActivePresentation")
            .map_err(|_| anyhow::anyhow!("No active PowerPoint presentation open"))?;

        let slides = active_pres.get_property_obj("Slides")?;
        let count = i32::try_from(&slides.get_property("Count")?).unwrap_or(0);

        let mut all_text = String::new();

        for i in 1..=count {
            all_text.push_str(&format!("--- Slide {} ---\n", i));
            if let Ok(sv) = slides.invoke_method("Item", vec![var_i4(i)]) {
                if let Ok(sd) = IDispatch::try_from(&sv) {
                    let slide = ComObject::new(sd);
                    if let Ok(shapes) = slide.get_property_obj("Shapes") {
                        let shapes_count = i32::try_from(&shapes.get_property("Count")?).unwrap_or(0);
                        for j in 1..=shapes_count {
                            if let Ok(sh_var) = shapes.invoke_method("Item", vec![var_i4(j)]) {
                                if let Ok(sh_disp) = IDispatch::try_from(&sh_var) {
                                    let shape = ComObject::new(sh_disp);
                                    // Check if HasTextFrame == -1 (msoTrue)
                                    if let Ok(has_tf) = shape.get_property("HasTextFrame") {
                                        if i32::try_from(&has_tf).unwrap_or(0) == -1 {
                                            if let Ok(tf) = shape.get_property_obj("TextFrame") {
                                                if let Ok(tr) = tf.get_property_obj("TextRange") {
                                                    if let Ok(tv) = tr.get_property("Text") {
                                                        if let Ok(bstr) = BSTR::try_from(&tv) {
                                                            let text = bstr.to_string();
                                                            if !text.trim().is_empty() {
                                                                all_text.push_str(&text);
                                                                all_text.push('\n');
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            all_text.push('\n');
        }

        Ok(all_text)
    }

    #[cfg(not(windows))]
    pub fn extract_active_document(&self) -> anyhow::Result<String> {
        Ok("PowerPoint COM Automation is only supported on Windows".to_string())
    }
}
