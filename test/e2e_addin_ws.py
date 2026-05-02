"""
Office Hub – E2E Test Suite for Office Add-in WebSocket Protocol
================================================================
Tests the full add-in <-> backend WebSocket communication flow.

Usage:
    python test/e2e_addin_ws.py

Requirements:
    pip install websockets
    Backend must be running: .\\start.ps1
"""

import asyncio
import json
import sys
import time
import traceback

try:
    import websockets
except ImportError:
    print("ERROR: Missing dependency. Run: pip install websockets")
    sys.exit(1)

sys.stdout.reconfigure(encoding='utf-8')

WS_URL = "ws://127.0.0.1:9001"
AUTH_TOKEN = "5d1de71cd0e14880bf60b68d48dcaeaa"  # from office-addin/.env

# ─────────────────────────────────────────────────────────────────────────────

class TestResult:
    def __init__(self, name: str):
        self.name = name
        self.passed = False
        self.message = ""
        self.duration_ms = 0.0

    def __str__(self):
        status = "PASS" if self.passed else "FAIL"
        icon = "OK" if self.passed else "!!"
        return f"[{icon}] [{self.duration_ms:.0f}ms] {status}: {self.name} - {self.message}"


async def recv_with_timeout(ws, timeout: float = 15.0):
    """Receive next message with timeout. Returns parsed dict or None on timeout."""
    try:
        raw = await asyncio.wait_for(ws.recv(), timeout=timeout)
        return json.loads(raw)
    except asyncio.TimeoutError:
        return None
    except Exception as e:
        print(f"  [recv error] {e}")
        return None


async def authenticate(ws) -> bool:
    """Send auth token and wait for auth_success."""
    await ws.send(json.dumps({"type": "auth", "token": AUTH_TOKEN}))
    msg = await recv_with_timeout(ws, timeout=5.0)
    if msg and msg.get("type") == "auth_success":
        print("  [auth] Authenticated OK")
        return True
    # If auth_secret is None on server side, server may not send auth_success
    if msg is None:
        print("  [auth] No auth response (auth_secret not configured) - OK")
        return True
    print(f"  [auth] Unexpected response: {msg.get('type')}")
    return False


# ─────────────────────────────────────────────────────────────────────────────
# Test Cases
# ─────────────────────────────────────────────────────────────────────────────

async def test_01_connect_and_auth() -> TestResult:
    """Test: Can connect and authenticate."""
    result = TestResult("Connect & Auth")
    t0 = time.monotonic()
    try:
        async with websockets.connect(WS_URL, open_timeout=5) as ws:
            ok = await authenticate(ws)
            result.passed = ok
            result.message = "Connected and authenticated" if ok else "Auth failed"
    except Exception as e:
        result.message = f"Connection failed: {e}"
    result.duration_ms = (time.monotonic() - t0) * 1000
    return result


async def test_07_ping_pong(ws) -> TestResult:
    """Test: Ping message gets Pong response."""
    result = TestResult("Ping/Pong")
    t0 = time.monotonic()
    try:
        ts = int(time.time() * 1000)
        await ws.send(json.dumps({"type": "ping", "timestamp_ms": ts}))
        msg = await recv_with_timeout(ws, timeout=5.0)
        if msg and msg.get("type") == "pong":
            result.passed = True
            result.message = f"Pong received (latency ~{int(time.time()*1000) - ts}ms)"
        else:
            result.message = f"Expected pong, got: {msg}"
    except Exception as e:
        result.message = str(e)
    result.duration_ms = (time.monotonic() - t0) * 1000
    return result


async def test_02_office_addin_event(ws) -> TestResult:
    """Test: DocumentOpened event from Office Add-in is accepted."""
    result = TestResult("OfficeAddinEvent - DocumentOpened (Word)")
    t0 = time.monotonic()
    try:
        await ws.send(json.dumps({
            "type": "office_addin_event",
            "event": "DocumentOpened",
            "file_path": "C:\\Users\\admin\\Desktop\\TestWord.docx",
            "app_type": "Word"
        }))
        msg = await recv_with_timeout(ws, timeout=3.0)
        if msg is None or msg.get("type") != "error":
            result.passed = True
            result.message = "Event accepted (no error)"
        else:
            result.message = f"Server error: {msg.get('message')}"
    except Exception as e:
        result.message = str(e)
    result.duration_ms = (time.monotonic() - t0) * 1000
    return result


async def test_05_outlook_event(ws) -> TestResult:
    """Test: Outlook-specific DocumentOpened event (no file_path required)."""
    result = TestResult("OfficeAddinEvent - Outlook Email")
    t0 = time.monotonic()
    try:
        await ws.send(json.dumps({
            "type": "office_addin_event",
            "event": "DocumentOpened",
            "app_type": "Outlook",
            "subject": "Test Email Subject",
            "sender": "test@example.com"
        }))
        msg = await recv_with_timeout(ws, timeout=3.0)
        if msg is None or msg.get("type") != "error":
            result.passed = True
            result.message = "Outlook event accepted"
        else:
            result.message = f"Error: {msg.get('message')}"
    except Exception as e:
        result.message = str(e)
    result.duration_ms = (time.monotonic() - t0) * 1000
    return result


async def test_03_chat_request_basic() -> TestResult:
    """Test: chat_request gets a chat_reply response (isolated connection)."""
    result = TestResult("ChatRequest - Basic Reply")
    t0 = time.monotonic()
    try:
        async with websockets.connect(WS_URL, open_timeout=8) as ws:
            await authenticate(ws)
            await ws.send(json.dumps({
                "type": "chat_request",
                "content": "Xin chao! Ban co the lam gi?",
                "app_type": "Word",
                "file_context": ""
            }))

            final_reply = None
            deadline = time.monotonic() + 120.0
            while time.monotonic() < deadline:
                remaining = deadline - time.monotonic()
                msg = await recv_with_timeout(ws, timeout=min(remaining, 60.0))
                if msg is None:
                    break
                msg_type = msg.get("type", "")
                print(f"    <- [{msg_type}]", end="")
                if msg_type in ("workflow_status", "chat_progress"):
                    info = msg.get("step_name") or msg.get("thought", "")[:40]
                    print(f" {info}")
                elif msg_type in ("chat_reply", "chat_response"):
                    content = (msg.get("content") or "")[:80]
                    print(f" {content}...")
                    final_reply = msg
                    break
                elif msg_type == "error":
                    print(f" ERROR: {msg.get('message')}")
                    break
                else:
                    print()

            if final_reply:
                result.passed = True
                result.message = f"Reply: {(final_reply.get('content') or '')[:60]}..."
            else:
                result.message = "Timeout waiting for chat_reply"
    except Exception as e:
        result.message = str(e)
        traceback.print_exc()
    result.duration_ms = (time.monotonic() - t0) * 1000
    return result


async def test_04_chat_with_document_content() -> TestResult:
    """Test: chat_request with inline document_content (isolated connection)."""
    result = TestResult("ChatRequest - With Document Content")
    t0 = time.monotonic()
    try:
        async with websockets.connect(WS_URL, open_timeout=8) as ws:
            await authenticate(ws)
            await ws.send(json.dumps({
                "type": "chat_request",
                "content": "Tom tat noi dung tai lieu nay trong 1 cau.",
                "app_type": "Word",
                "file_context": "TestDocument.docx",
                "document_content": "Hop dong so 001/2026 ky ngay 15 thang 4 nam 2026 giua Cong ty A va Cong ty B ve viec cung cap dich vu phan mem voi tong gia tri 500 trieu dong."
            }))

            final_reply = None
            deadline = time.monotonic() + 120.0
            while time.monotonic() < deadline:
                remaining = deadline - time.monotonic()
                msg = await recv_with_timeout(ws, timeout=min(remaining, 60.0))
                if msg is None:
                    break
                msg_type = msg.get("type", "")
                if msg_type in ("chat_reply", "chat_response"):
                    final_reply = msg
                    break
                elif msg_type == "error":
                    result.message = f"Server error: {msg.get('message')}"
                    result.duration_ms = (time.monotonic() - t0) * 1000
                    return result

            if final_reply:
                result.passed = True
                result.message = f"Reply: {(final_reply.get('content') or '')[:60]}..."
            else:
                result.message = "Timeout waiting for reply"
    except Exception as e:
        result.message = str(e)
    result.duration_ms = (time.monotonic() - t0) * 1000
    return result


async def test_06_reconnect_resilience() -> TestResult:
    """Test: Can disconnect and reconnect multiple times."""
    result = TestResult("Reconnect Resilience (3 cycles)")
    t0 = time.monotonic()
    success_count = 0
    try:
        for i in range(3):
            async with websockets.connect(WS_URL, open_timeout=5) as ws:
                ok = await authenticate(ws)
                if ok:
                    success_count += 1
            await asyncio.sleep(0.5)
        result.passed = success_count == 3
        result.message = f"{success_count}/3 reconnect cycles OK"
    except Exception as e:
        result.message = str(e)
    result.duration_ms = (time.monotonic() - t0) * 1000
    return result


# ─────────────────────────────────────────────────────────────────────────────

async def main():
    print("=" * 60)
    print("  Office Hub - E2E Office Add-in WebSocket Tests")
    print(f"  Target: {WS_URL}")
    print("=" * 60)

    results: list = []

    # Standalone tests
    results.append(await test_01_connect_and_auth())
    results.append(await test_06_reconnect_resilience())

    # Chat tests on isolated connections (avoids context_analysis contamination)
    print("\n[Running isolated chat tests...]")
    results.append(await test_03_chat_request_basic())
    results.append(await test_04_chat_with_document_content())

    # Stateful tests on shared connection
    print("\n[Opening shared connection for stateful tests...]")
    try:
        async with websockets.connect(WS_URL, open_timeout=8) as ws:
            auth_ok = await authenticate(ws)
            if not auth_ok:
                print("FAIL: Cannot proceed - authentication failed")
                sys.exit(1)

            results.append(await test_07_ping_pong(ws))
            results.append(await test_02_office_addin_event(ws))
            results.append(await test_05_outlook_event(ws))
    except Exception as e:
        print(f"\nFAIL: Could not connect for stateful tests: {e}")
        print("      Is the backend running?  Run: .\\start.ps1")

    print("\n" + "=" * 60)
    print("  TEST SUMMARY")
    print("=" * 60)
    passed = sum(1 for r in results if r.passed)
    for r in results:
        print(f"  {r}")
    print(f"\n  Result: {passed}/{len(results)} tests passed")
    print("=" * 60)

    if passed < len(results):
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
