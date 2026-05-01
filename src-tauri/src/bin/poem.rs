use office_hub_lib::agents::com_utils::dispatch::ComObject;
use office_hub_lib::agents::com_utils::dispatch::{var_bool, var_bstr, var_i4, var_optional};
use office_hub_lib::agents::office_master::com_word::WordApplication;
use std::convert::TryFrom;
use windows::Win32::System::Com::IDispatch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Khoi dong Office Hub COM Automation...");

    let word = WordApplication::connect_or_launch()?;

    println!("Ket noi thanh cong! Dang mo Word va tao van ban moi...");
    // Make Word visible
    word.app.set_property("Visible", var_bool(true))?;

    let docs = word.app.get_property_obj("Documents")?;
    let doc_var = docs.invoke_method(
        "Add",
        vec![
            var_optional(),
            var_optional(),
            var_optional(),
            var_optional(),
        ],
    )?;
    let doc = ComObject::new(IDispatch::try_from(&doc_var).unwrap());

    println!("Dang trinh bay can trang, chinh le...");
    let page_setup = doc.get_property_obj("PageSetup")?;
    // 1 cm = 28.35 points
    use windows::core::VARIANT;
    page_setup.set_property("TopMargin", VARIANT::from(2.5 * 28.35f32))?;
    page_setup.set_property("BottomMargin", VARIANT::from(2.5 * 28.35f32))?;
    page_setup.set_property("LeftMargin", VARIANT::from(3.0 * 28.35f32))?;
    page_setup.set_property("RightMargin", VARIANT::from(2.0 * 28.35f32))?;

    let selection = word.app.get_property_obj("Selection")?;
    let font = selection.get_property_obj("Font")?;
    let paragraph_format = selection.get_property_obj("ParagraphFormat")?;

    // Tiêu đề
    font.set_property("Name", var_bstr("Times New Roman"))?;
    font.set_property("Size", var_i4(24))?;
    font.set_property("Bold", var_bool(true))?;
    font.set_property("Color", var_i4(12611584))?; // Xanh
    paragraph_format.set_property("Alignment", var_i4(1))?; // 1 = wdAlignParagraphCenter

    selection.invoke_method("TypeText", vec![var_bstr("CẢNH KHUYA")])?;
    selection.invoke_method("TypeParagraph", vec![])?;
    selection.invoke_method("TypeParagraph", vec![])?;

    // Nội dung bài thơ
    println!("Dang viet tho...");
    font.set_property("Size", var_i4(16))?;
    font.set_property("Bold", var_bool(false))?;
    font.set_property("Italic", var_bool(true))?;
    font.set_property("Color", var_i4(0))?; // Đen

    let poem = vec![
        "Tiếng suối trong như tiếng hát xa,",
        "Trăng lồng cổ thụ bóng lồng hoa.",
        "Cảnh khuya như vẽ người chưa ngủ,",
        "Chưa ngủ vì lo nỗi nước nhà.",
    ];

    for line in poem {
        selection.invoke_method("TypeText", vec![var_bstr(line)])?;
        selection.invoke_method("TypeParagraph", vec![])?;
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    selection.invoke_method("TypeParagraph", vec![])?;

    // Tác giả
    font.set_property("Italic", var_bool(false))?;
    font.set_property("Bold", var_bool(true))?;
    font.set_property("Size", var_i4(14))?;
    paragraph_format.set_property("Alignment", var_i4(2))?; // 2 = wdAlignParagraphRight

    selection.invoke_method("TypeText", vec![var_bstr("- Hồ Chí Minh -")])?;

    println!("Hoan tat kiem thu! Bai tho da duoc tao va trinh bay tren Word bang Office Hub COM.");
    Ok(())
}
