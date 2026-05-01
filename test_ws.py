import asyncio
import websockets
import json
import sys
import traceback

sys.stdout.reconfigure(encoding='utf-8')

async def test_websocket():
    uri = "ws://localhost:9001"
    
    with open("test_ws_log.txt", "w", encoding="utf-8") as f:
        def log(msg):
            print(msg)
            f.write(msg + "\n")
            f.flush()

        log(f"Connecting to {uri}...")
        try:
            async with websockets.connect(uri) as websocket:
                log("Connected to WebSocket Server!")
                
                auth_msg = {
                    "type": "auth",
                    "token": "87ecb66c080a4de29eb20555c397181f"
                }
                log("Sending auth...")
                await websocket.send(json.dumps(auth_msg))
                
                cmd = {
                    "type": "command",
                    "session_id": "test_session_ws_1",
                    "text": "Go to https://google.com"
                }
                log(f"Sending command: {cmd['text']}")
                await websocket.send(json.dumps(cmd))

                while True:
                    response = await websocket.recv()
                    data = json.loads(response)
                    log(f"\n[SERVER -> CLIENT] Type: {data.get('type')}")
                    
                    if data.get("type") == "approval_request":
                        log(f"Received HITL Approval Request for action: {data['action_id']}")
                        log(f"Description: {data.get('description')}")
                        
                        approval = {
                            "type": "approval_response",
                            "action_id": data["action_id"],
                            "approved": True,
                            "responded_by": "python_test_script"
                        }
                        log("Sending approval response: Approve")
                        await websocket.send(json.dumps(approval))
                        
                    elif data.get("type") == "chat_reply":
                        log(f"Received Chat Reply:")
                        log(f"Content: {data.get('content')}")
                        log(f"Agent used: {data.get('agent_used')}")
                        log("Test successful! Exiting...")
                        break
                        
                    elif data.get("type") == "error":
                        log(f"Error from server: {data.get('message')}")
                        break
                        
        except Exception as e:
            log(f"Connection failed: {e}")
            log(traceback.format_exc())
            sys.exit(1)

if __name__ == "__main__":
    asyncio.run(test_websocket())
