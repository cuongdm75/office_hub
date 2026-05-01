use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use crate::AppError;

#[derive(Debug, Serialize, Deserialize)]
pub struct KnowledgeFile {
    pub filename: String,
    pub path: String,
    pub modified_at: u64,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub filename: String,
    pub name: String,
    pub description: String,
    pub modified_at: u64,
    pub is_folder: bool,
}

/// Validates category to prevent path traversal
fn validate_category(category: &str) -> Result<(), AppError> {
    let allowed = ["knowledge", "policy", "skills", "files", "inbox", "outbox"];
    if !allowed.contains(&category) {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid category '{}'. Must be one of: knowledge, policy, skills, files, inbox, outbox", category),
        )));
    }
    Ok(())
}

/// Get directory path for the given category
fn get_category_dir(app: &AppHandle, category: &str, workspace_id: Option<&str>) -> Result<PathBuf, AppError> {
    validate_category(category)?;

    let base_dir = app.path().app_data_dir()
        .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string())))?;

    // Map category names to actual directory names (for backward compat)
    let rel_path = match category {
        "policy" => PathBuf::from("policies"),
        "skills" => {
            // Skills live in .agent/skills/ of workspace root for consistency
            let mut workspace = std::env::current_dir().unwrap_or_default();
            if workspace.ends_with("src-tauri") {
                workspace = workspace.parent().unwrap_or(&workspace).to_path_buf();
            }
            let skills_dir = workspace.join(".agent").join("skills");
            if !skills_dir.exists() {
                fs::create_dir_all(&skills_dir)?;
            }
            return Ok(skills_dir);
        }
        "files" => PathBuf::from("input"),
        "inbox" => PathBuf::from("input"),
        "outbox" => PathBuf::from("output"),
        _ => PathBuf::from("knowledge"),
    };

    let dir = if let Some(wid) = workspace_id {
        if wid == "default" {
            base_dir.join(&rel_path)
        } else {
            base_dir.join("workspaces").join(wid).join(&rel_path)
        }
    } else {
        base_dir.join(&rel_path)
    };

    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Validate a filename to prevent path traversal
fn validate_filename(filename: &str) -> Result<(), AppError> {
    if filename.contains("..") || filename.contains('\\') {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid filename",
        )));
    }
    Ok(())
}

// ── List ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_knowledge(app: AppHandle, category: Option<String>, workspace_id: Option<String>) -> Result<Vec<KnowledgeFile>, AppError> {
    let cat = category.as_deref().unwrap_or("knowledge");
    validate_category(cat)?;
    let dir = get_category_dir(&app, cat, workspace_id.as_deref())?;
    let mut files = Vec::new();

    if dir.exists() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
                let metadata = entry.metadata()?;
                let modified_at = metadata.modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                files.push(KnowledgeFile {
                    filename: entry.file_name().to_string_lossy().to_string(),
                    path: path.to_string_lossy().to_string(),
                    modified_at,
                    category: cat.to_string(),
                });
            } else if path.is_dir() && cat == "skills" {
                // For skills: include folder-based skills (contain SKILL.md)
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    let metadata = fs::metadata(&skill_md).unwrap_or_else(|_| entry.metadata().unwrap());
                    let modified_at = metadata.modified()
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    files.push(KnowledgeFile {
                        filename: format!("{}/SKILL.md", entry.file_name().to_string_lossy()),
                        path: skill_md.to_string_lossy().to_string(),
                        modified_at,
                        category: cat.to_string(),
                    });
                }
            }
        }
    }

    // Sort: index.md first, then newest first
    files.sort_by(|a, b| {
        if a.filename == "index.md" {
            std::cmp::Ordering::Less
        } else if b.filename == "index.md" {
            std::cmp::Ordering::Greater
        } else {
            b.modified_at.cmp(&a.modified_at)
        }
    });

    Ok(files)
}

// ── List Skills with parsed metadata ────────────────────────────────────────

#[tauri::command]
pub async fn list_skills_metadata(app: AppHandle) -> Result<Vec<SkillMetadata>, AppError> {
    let dir = get_category_dir(&app, "skills", None)?;
    let mut skills = Vec::new();

    if !dir.exists() { return Ok(skills); }

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        let modified_at = entry.metadata()
            .unwrap_or_else(|_| fs::metadata(&path).unwrap())
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if path.is_dir() {
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                if let Ok(content) = fs::read_to_string(&skill_md) {
                    let (name, description) = parse_skill_frontmatter(&content)
                        .unwrap_or_else(|| (entry.file_name().to_string_lossy().to_string(), String::new()));
                    skills.push(SkillMetadata {
                        filename: format!("{}/SKILL.md", entry.file_name().to_string_lossy()),
                        name,
                        description,
                        modified_at,
                        is_folder: true,
                    });
                }
            }
        } else if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Ok(content) = fs::read_to_string(&path) {
                let fname = entry.file_name().to_string_lossy().to_string();
                let (name, description) = parse_skill_frontmatter(&content)
                    .unwrap_or_else(|| (fname.clone(), String::new()));
                skills.push(SkillMetadata {
                    filename: fname,
                    name,
                    description,
                    modified_at,
                    is_folder: false,
                });
            }
        }
    }

    skills.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(skills)
}

fn parse_skill_frontmatter(content: &str) -> Option<(String, String)> {
    if !content.starts_with("---") { return None; }
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 { return None; }

    let yaml = parts[1];
    let mut name = String::new();
    let mut description = String::new();

    for line in yaml.lines() {
        if let Some(v) = line.strip_prefix("name:") {
            name = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("description:") {
            description = v.trim().to_string();
        }
    }

    if name.is_empty() { return None; }
    Some((name, description))
}

// ── Read ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn read_knowledge_file(app: AppHandle, filename: String, category: Option<String>, workspace_id: Option<String>) -> Result<String, AppError> {
    validate_filename(&filename)?;
    let cat = category.as_deref().unwrap_or("knowledge");
    let dir = get_category_dir(&app, cat, workspace_id.as_deref())?;

    // Support subfolder paths like "skill-name/SKILL.md" for skills
    let file_path = dir.join(&filename);

    // Security: ensure resolved path is still inside the category dir
    let canonical_dir = dir.canonicalize().unwrap_or(dir.clone());
    let canonical_file = file_path.canonicalize().unwrap_or(file_path.clone());
    if !canonical_file.starts_with(&canonical_dir) {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied, "Access denied",
        )));
    }

    if !file_path.exists() {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("File {} not found", filename),
        )));
    }

    Ok(fs::read_to_string(file_path)?)
}

// ── Save ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn save_knowledge_file(app: AppHandle, filename: String, content: String, category: Option<String>, workspace_id: Option<String>) -> Result<(), AppError> {
    // Allow subfolder separator only as "/" for skills (e.g. "skill-name/SKILL.md")
    let cat = category.as_deref().unwrap_or("knowledge");
    let dir = get_category_dir(&app, cat, workspace_id.as_deref())?;

    // Validate: no ".." traversal
    if filename.contains("..") || filename.contains('\\') {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput, "Invalid filename",
        )));
    }

    let final_filename = if !filename.ends_with(".md") {
        format!("{}.md", filename)
    } else {
        filename
    };

    let file_path = dir.join(&final_filename);

    // Create parent dir if needed (for skills subfolder)
    if let Some(parent) = file_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    fs::write(file_path, content)?;
    Ok(())
}

// ── Delete ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn delete_knowledge_file(app: AppHandle, filename: String, category: Option<String>, workspace_id: Option<String>) -> Result<(), AppError> {
    validate_filename(&filename)?;
    let cat = category.as_deref().unwrap_or("knowledge");
    let dir = get_category_dir(&app, cat, workspace_id.as_deref())?;
    let file_path = dir.join(&filename);

    if file_path.exists() {
        fs::remove_file(file_path)?;
    }
    Ok(())
}

// ── Workspace Management ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub created_at: u64,
}

#[tauri::command]
pub async fn list_workspaces(app: AppHandle) -> Result<Vec<Workspace>, AppError> {
    let base_dir = app.path().app_data_dir()
        .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string())))?
        .join("workspaces");

    let mut workspaces = Vec::new();
    
    // Always return default workspace
    workspaces.push(Workspace {
        id: "default".to_string(),
        name: "Default Workspace".to_string(),
        created_at: 0,
    });

    if base_dir.exists() {
        for entry in fs::read_dir(&base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let id = entry.file_name().to_string_lossy().to_string();
                let meta_path = path.join("meta.json");
                let mut name = id.clone();
                let mut created_at = 0;

                if meta_path.exists() {
                    if let Ok(content) = fs::read_to_string(&meta_path) {
                        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(n) = meta.get("name").and_then(|v| v.as_str()) {
                                name = n.to_string();
                            }
                            if let Some(c) = meta.get("created_at").and_then(|v| v.as_u64()) {
                                created_at = c;
                            }
                        }
                    }
                } else {
                    created_at = entry.metadata()
                        .ok()
                        .and_then(|m| m.created().ok())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                }

                workspaces.push(Workspace { id, name, created_at });
            }
        }
    }

    Ok(workspaces)
}

#[tauri::command]
pub async fn create_workspace(app: AppHandle, name: String) -> Result<Workspace, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let base_dir = app.path().app_data_dir()
        .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string())))?
        .join("workspaces").join(&id);

    fs::create_dir_all(&base_dir)?;
    fs::create_dir_all(base_dir.join("knowledge"))?;
    fs::create_dir_all(base_dir.join("policies"))?;
    fs::create_dir_all(base_dir.join("input"))?;
    fs::create_dir_all(base_dir.join("output"))?;
    fs::create_dir_all(base_dir.join("links"))?;
    fs::create_dir_all(base_dir.join("memory"))?;
    
    // Initialize links.json
    fs::write(base_dir.join("links.json"), "[]")?;

    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let meta = serde_json::json!({
        "id": id,
        "name": name,
        "created_at": created_at
    });

    fs::write(base_dir.join("meta.json"), serde_json::to_string_pretty(&meta)?)?;

    Ok(Workspace {
        id,
        name,
        created_at,
    })
}

#[tauri::command]
pub async fn delete_workspace(app: AppHandle, id: String) -> Result<(), AppError> {
    if id == "default" {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Cannot delete default workspace",
        )));
    }
    
    validate_filename(&id)?;

    let base_dir = app.path().app_data_dir()
        .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string())))?
        .join("workspaces").join(&id);

    if base_dir.exists() {
        fs::remove_dir_all(base_dir)?;
    }

    Ok(())
}

#[tauri::command]
pub async fn get_workspace_path(app: AppHandle, workspace_id: Option<String>) -> Result<String, AppError> {
    let base_dir = app.path().app_data_dir()
        .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string())))?;
        
    let dir = if let Some(wid) = workspace_id {
        if wid == "default" || wid == "Global" {
            base_dir.join("workspaces").join("default")
        } else {
            base_dir.join("workspaces").join(wid)
        }
    } else {
        base_dir.join("workspaces").join("default")
    };
    
    // Ensure default workspace exists
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::create_dir_all(dir.join("knowledge"));
        let _ = fs::create_dir_all(dir.join("policies"));
        let _ = fs::create_dir_all(dir.join("input"));
        let _ = fs::create_dir_all(dir.join("output"));
        let _ = fs::create_dir_all(dir.join("links"));
        let _ = fs::create_dir_all(dir.join("memory"));
        let _ = fs::write(dir.join("links.json"), "[]");
    }
    
    Ok(dir.to_string_lossy().to_string())
}

// ── Workspace Links ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceLink {
    pub id: String,
    pub title: String,
    pub url: String,
    pub added_at: u64,
}

fn get_links_file_path(app: &AppHandle, workspace_id: Option<&str>) -> Result<PathBuf, AppError> {
    let base_dir = app.path().app_data_dir()
        .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string())))?;
        
    let path = if let Some(wid) = workspace_id {
        if wid == "default" {
            base_dir.join("links.json")
        } else {
            base_dir.join("workspaces").join(wid).join("links.json")
        }
    } else {
        base_dir.join("links.json")
    };
    
    Ok(path)
}

#[tauri::command]
pub async fn list_workspace_links(app: AppHandle, workspace_id: Option<String>) -> Result<Vec<WorkspaceLink>, AppError> {
    let path = get_links_file_path(&app, workspace_id.as_deref())?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    
    let content = fs::read_to_string(&path)?;
    let links: Vec<WorkspaceLink> = serde_json::from_str(&content).unwrap_or_default();
    Ok(links)
}

#[tauri::command]
pub async fn add_workspace_link(app: AppHandle, workspace_id: Option<String>, url: String, title: String) -> Result<WorkspaceLink, AppError> {
    let path = get_links_file_path(&app, workspace_id.as_deref())?;
    
    let mut links: Vec<WorkspaceLink> = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Vec::new()
    };
    
    let link = WorkspaceLink {
        id: uuid::Uuid::new_v4().to_string(),
        title,
        url,
        added_at: std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs(),
    };
    
    links.push(link.clone());
    fs::write(&path, serde_json::to_string_pretty(&links)?)?;
    
    Ok(link)
}

#[tauri::command]
pub async fn remove_workspace_link(app: AppHandle, workspace_id: Option<String>, id: String) -> Result<(), AppError> {
    let path = get_links_file_path(&app, workspace_id.as_deref())?;
    if !path.exists() {
        return Ok(());
    }
    
    let content = fs::read_to_string(&path)?;
    let mut links: Vec<WorkspaceLink> = serde_json::from_str(&content).unwrap_or_default();
    
    links.retain(|l| l.id != id);
    fs::write(&path, serde_json::to_string_pretty(&links)?)?;
    
    Ok(())
}
