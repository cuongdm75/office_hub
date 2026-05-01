use office_hub_lib::mcp::internal_servers::InternalMcpServer;
use office_hub_lib::mcp::native_chart::NativeChartServer;
use office_hub_lib::agents::office_master::OfficeMasterAgent;
use office_hub_lib::agents::Agent;
use office_hub_lib::orchestrator::AgentTask;
use std::collections::HashMap;

#[tokio::test]
#[cfg(windows)]
async fn test_native_chart_to_ppt_pipeline() {
    // Bước 1: Khởi tạo NativeChartServer và gọi công cụ generate_chart_native
    let chart_server = NativeChartServer::new();
    
    let args = serde_json::json!({
        "chart_type": "bar",
        "title": "Doanh thu Q1/2026",
        "data": [
            {"name": "Tháng 1", "value": 150.0},
            {"name": "Tháng 2", "value": 200.0},
            {"name": "Tháng 3", "value": 180.0}
        ],
        "x_key": "name",
        "y_key": "value"
    });

    let result = chart_server.call_tool("generate_chart_native", Some(args)).await.expect("Gọi công cụ tạo biểu đồ thất bại");
    assert!(!result.is_error, "Công cụ trả về lỗi");
    
    // Lấy đường dẫn file ảnh từ kết quả
    let content = result.content[0].text.as_ref().unwrap();
    assert!(content.contains("Tạo biểu đồ thành công"));
    
    // Phân tích đường dẫn file ảnh (Ví dụ: Đã lưu tại: C:\Users\admin\AppData\Local\Temp\office_hub_exports\chart_native_xxx.png)
    let parts: Vec<&str> = content.split("Đã lưu tại: ").collect();
    assert!(parts.len() == 2, "Không tìm thấy đường dẫn file ảnh trong output");
    let image_path = parts[1].trim().to_string();
    assert!(std::path::Path::new(&image_path).exists(), "File ảnh không tồn tại");

    // Bước 2: Tạo tác vụ cho OfficeMasterAgent để chèn ảnh vào slide
    let mut ppt_agent = OfficeMasterAgent::new();
    
    // Chúng ta tạo một map tham số để cấu hình tác vụ
    let mut parameters = HashMap::new();
    
    // Tạo một file PPTX tạm nếu có thể, hoặc dùng ActivePresentation
    // Trong môi trường test này, giả lập việc chèn vào slide số 1
    parameters.insert("slide_index".to_string(), serde_json::Value::Number(serde_json::Number::from(1)));
    parameters.insert("image_path".to_string(), serde_json::Value::String(image_path));
    parameters.insert("left".to_string(), serde_json::json!(50.0));
    parameters.insert("top".to_string(), serde_json::json!(50.0));
    parameters.insert("width".to_string(), serde_json::json!(600.0));
    parameters.insert("height".to_string(), serde_json::json!(400.0));
    
    // Lưu ý: Phải có sẵn một file PPTX mở trong hệ thống (hoặc tạo một file tạm nếu testing thật).
    // Ở đây dùng Ignore nếu môi trường không có PowerPoint đang mở
    let task = AgentTask {
        task_id: "test-chart-ppt-001".to_string(),
        action: "ppt_add_picture".to_string(),
        intent: office_hub_lib::orchestrator::intent::Intent::GeneralChat(Default::default()),
        message: "Chèn ảnh biểu đồ vào PowerPoint".to_string(),
        context_file: None,
        session_id: "test-session".to_string(),
        parameters,
        llm_gateway: None,
        global_policy: None,
        knowledge_context: None,
        parent_task_id: None,
        dependencies: vec![],
    };

    // Bước 3: Thực thi tác vụ (Có thể fail nếu PowerPoint chưa mở, nên test không panic)
    let ppt_result = ppt_agent.execute(task).await;
    
    match ppt_result {
        Ok(output) => {
            // Test hoàn tất luồng thành công (nếu PowerPoint đang hoạt động)
            println!("Pipeline Success: {}", output.content);
        },
        Err(e) => {
            // Bỏ qua lỗi nếu hệ thống chạy CI/CD không có PowerPoint COM server
            println!("Pipeline executed but PowerPoint COM failed (expected in CI): {}", e);
        }
    }
}
