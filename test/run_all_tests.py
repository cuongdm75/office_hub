import asyncio
import websockets
import json
import sys
import traceback

sys.stdout.reconfigure(encoding='utf-8')

# test cases
commands = [
    {
        "name": "Folder Scanner",
        "text": r"Tóm tắt toàn bộ file trong thư mục e:\Office hub\test_mock_data"
    },
    {
        "name": "Analyst Agent",
        "text": r"Phân tích file báo cáo doanh thu Q4 tại e:\Office hub\test_mock_data\BaoCao_Q4.xlsx"
    },
    {
        "name": "Office Master",
        "text": r"Tạo một slide PPT từ nội dung file Word dự án tại e:\Office hub\test_mock_data\DuAn_TongHop.docx"
    },
    {
        "name": "Web Researcher & HITL",
        "text": "Mở trang google.com"
    }
]

async def run_test_case(websocket, test_idx, command):
    print(f"\n--- Running Test {test_idx + 1}: {command['name']} ---")
    session_id = f"test_session_e2e_{test_idx}"
    
    cmd = {
        "type": "command",
        "session_id": session_id,
        "text": command["text"]
    }
    print(f"[->] Sending command: {cmd['text']}")
    await websocket.send(json.dumps(cmd))
    
    # Wait for completion or HITL
    while True:
        try:
            response = await asyncio.wait_for(websocket.recv(), timeout=120)
            data = json.loads(response)
            msg_type = data.get("type")
            print(f"[<-] Received message type: {msg_type}")
            if msg_type not in ["approval_request", "chat_reply", "error"]:
                print(f"     Payload: {json.dumps(data)[:200]}")
            if msg_type == "approval_request":
                print(f"[<-] Received HITL Approval Request: {data.get('description')}")
                approval = {
                    "type": "approval_response",
                    "action_id": data["action_id"],
                    "approved": True,
                    "responded_by": "python_test_runner"
                }
                print("[->] Sending HITL Approve")
                await websocket.send(json.dumps(approval))
                
            elif msg_type == "chat_reply":
                print(f"[<-] Received Chat Reply:")
                print(f"     Content: {data.get('content')[:100]}...")
                print(f"     Agent used: {data.get('agent_used')}")
                print(f"✅ Test {command['name']} SUCCESS")
                return True
                
            elif msg_type == "error":
                print(f"[<-] Error from server: {data.get('message')}")
                print(f"❌ Test {command['name']} FAILED")
                return False
                
        except asyncio.TimeoutError:
            print(f"❌ Test {command['name']} TIMEOUT")
            return False

async def main():
    uri = "ws://localhost:9001"
    print(f"Connecting to {uri}...")
    try:
        async with websockets.connect(uri) as websocket:
            print("Connected to WebSocket Server!")
            results = []
            for i, cmd in enumerate(commands):
                success = await run_test_case(websocket, i, cmd)
                results.append((cmd['name'], success))
                await asyncio.sleep(2) # brief pause between tests
            
            print("\n--- TEST SUMMARY ---")
            for name, success in results:
                print(f"{name}: {'✅ PASS' if success else '❌ FAIL'}")
                
    except Exception as e:
        print(f"Connection failed: {e}")
        print(traceback.format_exc())
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())
