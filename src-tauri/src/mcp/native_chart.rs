use crate::mcp::broker::InternalMcpServer;
use crate::mcp::{McpTool, ToolCallResult, ToolContent};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::Value;
use plotters::prelude::*;
use std::path::PathBuf;

pub struct NativeChartServer;

impl NativeChartServer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl InternalMcpServer for NativeChartServer {
    fn name(&self) -> &str {
        "native_chart_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![
            McpTool {
                name: "generate_chart_native".to_string(),
                description: "Vẽ biểu đồ từ dữ liệu JSON và xuất ra đường dẫn file ảnh PNG cục bộ bằng Native Engine. Không cần Frontend. Rất hữu ích khi cần chèn biểu đồ vào PowerPoint (PPTX) hoặc Word.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "chart_type": { "type": "string", "enum": ["bar", "line", "scatter"], "description": "Loại biểu đồ" },
                        "title": { "type": "string", "description": "Tiêu đề của biểu đồ" },
                        "data": { "type": "array", "description": "Mảng dữ liệu JSON để vẽ. VD: [{'name': 'Q1', 'value': 100}, {'name': 'Q2', 'value': 200}]", "items": { "type": "object" } },
                        "x_key": { "type": "string", "description": "Tên trường dữ liệu dùng cho trục X (VD: 'name')" },
                        "y_key": { "type": "string", "description": "Tên trường dữ liệu dùng cho trục Y (VD: 'value')" }
                    },
                    "required": ["chart_type", "title", "data", "x_key", "y_key"]
                }),
                tags: vec![],
            }
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        if name != "generate_chart_native" {
            return Err(anyhow!("Tool not found"));
        }

        let args = arguments.unwrap_or_default();
        let chart_type = args.get("chart_type").and_then(|v| v.as_str()).unwrap_or("bar");
        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("Chart");
        let data = args.get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let x_key = args.get("x_key").and_then(|v| v.as_str()).unwrap_or("name");
        let y_key = args.get("y_key").and_then(|v| v.as_str()).unwrap_or("value");

        if data.is_empty() {
            return Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some("Data is empty".to_string()),
                    data: None,
                    mime_type: None,
                }],
                is_error: true,
            });
        }

        let temp_dir = std::env::temp_dir().join("office_hub_exports");
        let _ = std::fs::create_dir_all(&temp_dir);
        let request_id = uuid::Uuid::new_v4().to_string();
        let file_name = format!("chart_native_{}.png", request_id);
        let file_path = temp_dir.join(&file_name);

        match draw_chart(chart_type, title, &data, x_key, y_key, &file_path) {
            Ok(_) => {
                Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!("Tạo biểu đồ thành công. Đã lưu tại: {}", file_path.to_string_lossy())),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: false,
                })
            }
            Err(e) => {
                Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!("Failed to draw chart: {}", e)),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                })
            }
        }
    }
}

fn draw_chart(chart_type: &str, title: &str, data: &[Value], x_key: &str, y_key: &str, file_path: &PathBuf) -> anyhow::Result<()> {
    let root = BitMapBackend::new(file_path, (800, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    let mut parsed_data = Vec::new();
    let mut max_y = 0.0_f64;

    for item in data {
        let x_val = item.get(x_key).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let y_val = item.get(y_key).and_then(|v| v.as_f64()).or_else(|| item.get(y_key).and_then(|v| v.as_i64()).map(|i| i as f64)).unwrap_or(0.0);
        if y_val > max_y {
            max_y = y_val;
        }
        parsed_data.push((x_val, y_val));
    }

    max_y = max_y * 1.1;
    if max_y <= 0.0 { max_y = 10.0; }

    let mut chart = ChartBuilder::on(&root)
        .caption(title, ("sans-serif", 30).into_font())
        .margin(30)
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(
            -0.5f64..(parsed_data.len() as f64 - 0.5),
            0.0f64..max_y,
        )?;

    let x_labels: Vec<String> = parsed_data.iter().map(|(x, _)| x.clone()).collect();
    chart.configure_mesh()
        .x_label_formatter(&|v| {
            let idx = v.round() as i32;
            if idx >= 0 && idx < x_labels.len() as i32 {
                x_labels[idx as usize].clone()
            } else {
                String::new()
            }
        })
        .x_labels(parsed_data.len())
        .draw()?;

    match chart_type {
        "bar" => {
            chart.draw_series(
                parsed_data.iter().enumerate().map(|(i, (_, y))| {
                    let x0 = i as f64 - 0.3;
                    let x1 = i as f64 + 0.3;
                    Rectangle::new([(x0, 0.0), (x1, *y)], RGBColor(54, 162, 235).filled())
                })
            )?;
        }
        "line" => {
            chart.draw_series(LineSeries::new(
                parsed_data.iter().enumerate().map(|(i, (_, y))| (i as f64, *y)),
                &RGBColor(255, 99, 132),
            ))?;
            chart.draw_series(PointSeries::of_element(
                parsed_data.iter().enumerate().map(|(i, (_, y))| (i as f64, *y)),
                5,
                &RGBColor(255, 99, 132),
                &|c, s, st| {
                    return EmptyElement::at(c)
                    + Circle::new((0,0),s,st.filled())
                },
            ))?;
        }
        "scatter" => {
            chart.draw_series(PointSeries::of_element(
                parsed_data.iter().enumerate().map(|(i, (_, y))| (i as f64, *y)),
                5,
                &RGBColor(75, 192, 192),
                &|c, s, st| {
                    return EmptyElement::at(c)
                    + Circle::new((0,0),s,st.filled())
                },
            ))?;
        }
        _ => {
            // Default to bar
            chart.draw_series(
                parsed_data.iter().enumerate().map(|(i, (_, y))| {
                    let x0 = i as f64 - 0.3;
                    let x1 = i as f64 + 0.3;
                    Rectangle::new([(x0, 0.0), (x1, *y)], RGBColor(54, 162, 235).filled())
                })
            )?;
        }
    }

    root.present()?;
    Ok(())
}
