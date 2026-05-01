import WebSocket from 'ws';

console.log('Khởi chạy Script giả lập Office Add-in...');

const ws = new WebSocket('ws://localhost:9001');

ws.on('open', () => {
    console.log('✅ Đã kết nối với Office Hub Backend (Port 9001)');

    // 1. Giả lập mở file Word
    console.log('-> Gửi sự kiện DocumentOpened (Word)...');
    ws.send(JSON.stringify({
        type: 'office_addin_event',
        event: 'DocumentOpened',
        file_path: 'C:\\Users\\admin\\Desktop\\TestWord.docx',
        app_type: 'Word'
    }));

    // 2. Chờ 3 giây, sau đó giả lập chat request
    setTimeout(() => {
        console.log('-> Gửi yêu cầu Chat (Sửa văn bản)...');
        ws.send(JSON.stringify({
            type: 'chat_request',
            content: 'Thay đổi chữ Nguyễn Văn A thành Trần Văn B trong văn bản hiện tại',
            file_context: 'C:\\Users\\admin\\Desktop\\TestWord.docx'
        }));
    }, 3000);
});

ws.on('message', (data) => {
    try {
        const msg = JSON.parse(data.toString());
        if (msg.type === 'context_analysis') {
            console.log('\n[Phản hồi từ Backend] Context Summary:');
            console.log(msg.summary);
        } else if (msg.type === 'chat_response') {
            console.log('\n[Phản hồi từ Backend] Chat Reply:');
            console.log(msg.content);
            console.log('\nThử nghiệm kết thúc thành công! Đóng kết nối.');
            ws.close();
        } else if (msg.type === 'error') {
            console.error('\n[Lỗi từ Backend]:', msg.message);
        }
    } catch (e) {
        console.log('Raw message:', data.toString());
    }
});

ws.on('close', () => {
    console.log('Kết nối đã đóng.');
});

ws.on('error', (err) => {
    console.error('Lỗi WebSocket:', err.message);
    console.log('Hãy đảm bảo Backend Tauri đang chạy (cargo run)');
});
