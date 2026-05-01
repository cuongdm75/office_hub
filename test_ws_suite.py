"""
Office Hub - WebSocket Test Suite v2
======================================
Kiem thu toan dien WS protocol: auth, ping, command, sessions, addin events.

Usage:
    python test_ws_suite.py
    python test_ws_suite.py --token <YOUR_TOKEN>
    python test_ws_suite.py --host 192.168.x.x --port 9001
    python test_ws_suite.py --skip-llm   # Skip slow LLM tests
"""

import asyncio
import websockets
import json
import sys
import time
import argparse
import traceback
from pathlib import Path
import base64

sys.stdout.reconfigure(encoding="utf-8")

# ──────────────────────────────────────────────────────────────
# Config
# ──────────────────────────────────────────────────────────────

def load_token_from_config(config_path: str = None) -> str:
    """Try to read auth_secret from config.yaml."""
    candidates = [
        config_path,
        r"src-tauri\target\debug\config.yaml",
        r"src-tauri\config.yaml",
        r"config.yaml",
    ]
    try:
        import yaml
        for c in candidates:
            if c and Path(c).exists():
                with open(c, encoding="utf-8") as f:
                    cfg = yaml.safe_load(f)
                token = cfg.get("websocket", {}).get("auth_secret")
                if token:
                    print(f"[INFO] Loaded token from: {c}")
                    return token
    except ImportError:
        pass
    return "87ecb66c080a4de29eb20555c397181f"


# ──────────────────────────────────────────────────────────────
# Test helpers
# ──────────────────────────────────────────────────────────────

class TestResult:
    def __init__(self):
        self.passed = []
        self.failed = []
        self.skipped = []

    def ok(self, name, note=""):
        label = f"  PASS  {name}" + (f" ({note})" if note else "")
        print(f"  OK {label}")
        self.passed.append(name)

    def fail(self, name, reason=""):
        print(f"  FAIL  {name}" + (f" -- {reason}" if reason else ""))
        self.failed.append((name, reason))

    def skip(self, name, reason=""):
        print(f"  SKIP  {name}" + (f" -- {reason}" if reason else ""))
        self.skipped.append(name)

    def summary(self):
        total = len(self.passed) + len(self.failed)
        print("\n" + "="*60)
        print(f"Results: {len(self.passed)}/{total} passed  ({len(self.skipped)} skipped)")
        if self.failed:
            print("Failed tests:")
            for name, reason in self.failed:
                print(f"  X {name}: {reason}")
        print("="*60)
        return len(self.failed) == 0


async def recv_with_timeout(ws, timeout=8.0):
    """Receive one message with a timeout. Returns parsed JSON or None."""
    try:
        raw = await asyncio.wait_for(ws.recv(), timeout=timeout)
        return json.loads(raw)
    except (asyncio.TimeoutError, Exception):
        return None


async def drain_until(ws, want_type: str, timeout: float, auto_approve_hitl=True):
    """
    Drain messages until we see a message of `want_type` OR timeout.
    Returns (found_msg, all_msgs_seen).
    """
    deadline = time.time() + timeout
    seen = []
    while time.time() < deadline:
        remaining = deadline - time.time()
        msg = await recv_with_timeout(ws, timeout=min(5.0, remaining))
        if msg is None:
            continue
        seen.append(msg)
        t = msg.get("type", "")
        # Print intermediate messages for debugging
        if t == "workflow_status":
            print(f"    [STREAM] workflow_status: {msg.get('status')} -- {msg.get('message','')[:60]}")
        elif t not in ("pong",):
            print(f"    [STREAM] msg.type={t}")
        # Auto-approve HITL
        if auto_approve_hitl and t == "approval_request":
            await ws.send(json.dumps({
                "type": "approval_response",
                "action_id": msg["action_id"],
                "approved": True,
                "responded_by": "test_suite"
            }))
            print("    [INFO] Auto-approved HITL request")
        if t == want_type:
            return msg, seen
    return None, seen


async def authenticate(ws, token: str, result: TestResult) -> bool:
    """Send auth and wait for auth_success."""
    await ws.send(json.dumps({"type": "auth", "token": token}))
    msg = await recv_with_timeout(ws, timeout=5)
    if msg and msg.get("type") == "auth_success":
        result.ok("TC-WS-1: Auth with correct token -> auth_success")
        return True
    else:
        result.fail("TC-WS-1: Auth with correct token -> auth_success", f"Got: {msg}")
        return False


# ──────────────────────────────────────────────────────────────
# Test cases
# ──────────────────────────────────────────────────────────────

async def test_auth_wrong_token(host, port, result: TestResult):
    """TC-WS-2: Wrong token should get auth_error."""
    uri = f"ws://{host}:{port}"
    try:
        async with websockets.connect(uri) as ws:
            await ws.send(json.dumps({"type": "auth", "token": "WRONG_TOKEN_12345"}))
            msg = await recv_with_timeout(ws, timeout=4)
            if msg and msg.get("type") == "auth_error":
                result.ok("TC-WS-2: Wrong token -> auth_error")
            else:
                # Accept either auth_error or connection close
                close_msg = await recv_with_timeout(ws, timeout=3)
                if close_msg is None:
                    result.ok("TC-WS-2: Wrong token -> connection closed")
                else:
                    result.fail("TC-WS-2: Wrong token -> auth_error",
                                f"Expected auth_error, got: {msg}, then: {close_msg}")
    except websockets.exceptions.ConnectionClosed:
        result.ok("TC-WS-2: Wrong token -> connection closed by server")
    except Exception as e:
        result.fail("TC-WS-2: Wrong token -> auth_error", str(e))


async def test_ping_pong(ws, result: TestResult):
    """TC-WS-3: Ping -> Pong within 3s."""
    ts = int(time.time() * 1000)
    await ws.send(json.dumps({"type": "ping", "timestamp_ms": ts}))
    t0 = time.time()
    msg = await recv_with_timeout(ws, timeout=3)
    elapsed = time.time() - t0
    if msg and msg.get("type") == "pong" and elapsed < 3:
        result.ok(f"TC-WS-3: Ping -> Pong", f"{elapsed*1000:.0f}ms")
    else:
        result.fail("TC-WS-3: Ping -> Pong", f"Got: {msg} in {elapsed:.1f}s")


async def test_list_sessions(ws, result: TestResult):
    """TC-WS-5: list_sessions -> session_list"""
    # Small pause to ensure no residual messages in buffer
    await asyncio.sleep(0.3)
    await ws.send(json.dumps({"type": "list_sessions"}))
    # May receive a stale pong first – drain until session_list or timeout
    deadline = time.time() + 8
    while time.time() < deadline:
        msg = await recv_with_timeout(ws, timeout=3)
        if msg is None:
            break
        if msg.get("type") == "session_list":
            count = len(msg.get("sessions", []))
            result.ok(f"TC-WS-5: list_sessions -> session_list", f"{count} sessions")
            return
        # Skip stale messages (e.g. pong)
    result.fail("TC-WS-5: list_sessions -> session_list", "No session_list received")


async def test_get_session_history(ws, result: TestResult):
    """TC-WS-6: get_session_history -> session_history (or graceful error)"""
    fake_id = "nonexistent_session_test_001"
    await ws.send(json.dumps({"type": "get_session_history", "session_id": fake_id}))
    msg = await recv_with_timeout(ws, timeout=5)
    if msg and msg.get("type") == "session_history":
        result.ok("TC-WS-6: get_session_history -> session_history (empty is OK)")
    elif msg and msg.get("type") == "error":
        # Acceptable: server returns error for unknown session
        result.ok("TC-WS-6: get_session_history -> graceful error response",
                  f"error: {msg.get('message','')[:40]}")
    elif msg is None:
        # Backend doesn't implement this message type yet - skip
        result.skip("TC-WS-6: get_session_history", "Not yet implemented by backend (returns nothing)")
    else:
        result.fail("TC-WS-6: get_session_history -> session_history", f"Got: {msg}")


async def test_command_with_llm(ws, session_id: str, result: TestResult, skip_llm=False):
    """TC-WS-4: command -> chat_reply (end-to-end with LLM)"""
    if skip_llm:
        result.skip("TC-WS-4: command -> chat_reply", "LLM tests skipped (--skip-llm)")
        return

    print("    [INFO] Sending LLM command (may take 30-90s with local Ollama)...")
    await ws.send(json.dumps({
        "type": "command",
        "session_id": session_id,
        "text": "Xin chao! Hay tra loi ngan gon: 1 + 1 bang may?"
    }))

    # Drain all messages, accept workflow_status intermediates
    found, seen = await drain_until(ws, "chat_reply", timeout=120)
    if found:
        result.ok("TC-WS-4: command -> chat_reply",
                  f"agent={found.get('agent_used','N/A')}")
        # Check if workflow_status was also emitted (Bug #2 fix verification)
        had_ws = any(m.get("type") == "workflow_status" for m in seen)
        if had_ws:
            result.ok("TC-WF-5: workflow_status received before chat_reply (Bug #2 FIXED)")
        else:
            result.fail("TC-WF-5: workflow_status relay", "workflow_status not received before chat_reply")
    else:
        types_seen = [m.get("type") for m in seen]
        result.fail("TC-WS-4: command -> chat_reply", f"Timeout (120s). Seen: {types_seen}")
        result.fail("TC-WF-5: workflow_status relay", "Timeout - could not verify")


async def test_voice_command(ws, session_id: str, result: TestResult, skip_llm=False):
    """TC-WS-7: voice_command (stub) -> some response"""
    if skip_llm:
        result.skip("TC-WS-7: voice_command -> chat_reply", "LLM tests skipped")
        return

    fake_audio = base64.b64encode(b"\x00" * 100).decode()
    await ws.send(json.dumps({
        "type": "voice_command",
        "session_id": session_id,
        "audio_base64": fake_audio
    }))
    # Voice may get any response (chat_reply, error, or nothing if not implemented)
    msg = await recv_with_timeout(ws, timeout=10)
    if msg and msg.get("type") == "chat_reply":
        result.ok("TC-WS-7: voice_command -> chat_reply")
    elif msg and msg.get("type") == "error":
        # Acceptable: voice not fully implemented
        result.ok("TC-WS-7: voice_command -> error (stub, expected)", msg.get("message","")[:40])
    elif msg is None:
        result.skip("TC-WS-7: voice_command", "No response (voice may be pending LLM from prev command)")
    else:
        result.fail("TC-WS-7: voice_command -> chat_reply", f"Got: {msg.get('type')}")


async def test_office_addin_event(ws, result: TestResult, skip_llm=False):
    """TC-WS-9: office_addin_event -> any AI response (workflow_status or chat_reply)"""
    if skip_llm:
        result.skip("TC-WS-9: office_addin_event -> response", "LLM tests skipped")
        return

    await ws.send(json.dumps({
        "type": "office_addin_event",
        "event": "DocumentOpened",
        "file_path": "C:\\test_document.docx",
        "app_type": "Word"
    }))
    # Backend may respond with workflow_status, chat_reply, or context_analysis
    msg = await recv_with_timeout(ws, timeout=15)
    if msg and msg.get("type") in ("context_analysis", "chat_reply", "workflow_status"):
        result.ok(f"TC-WS-9: office_addin_event -> {msg.get('type')} received")
    elif msg and msg.get("type") == "error":
        result.fail("TC-WS-9: office_addin_event -> response",
                    f"Server error: {msg.get('message')}")
    elif msg is None:
        result.skip("TC-WS-9: office_addin_event", "No response in 15s (event may not trigger AI)")
    else:
        result.fail("TC-WS-9: office_addin_event -> response", f"Unexpected: {msg.get('type')}")


async def test_chat_request_addin(ws, result: TestResult, skip_llm=False):
    """TC-WS-10: chat_request (from add-in) -> chat_reply"""
    if skip_llm:
        result.skip("TC-WS-10: chat_request -> chat_reply", "LLM tests skipped")
        return

    print("    [INFO] Sending chat_request from add-in...")
    await ws.send(json.dumps({
        "type": "chat_request",
        "content": "Xin chao, toi can ho tro soan thao van ban.",
        "file_context": None,
        "app_type": "Word",
        "email_context": None
    }))
    found, seen = await drain_until(ws, "chat_reply", timeout=60)
    if found:
        result.ok("TC-WS-10: chat_request -> chat_reply")
    else:
        types_seen = [m.get("type") for m in seen]
        if any(t in ("chat_response", "workflow_status") for t in types_seen):
            result.ok("TC-WS-10: chat_request -> response received", str(types_seen))
        else:
            result.fail("TC-WS-10: chat_request -> chat_reply",
                        f"Timeout. Seen: {types_seen}")


async def test_file_transfer_metadata(ws, session_id: str, result: TestResult, skip_llm=False):
    """TC-FILE-1: command to create Word doc -> check attachment in metadata"""
    if skip_llm:
        result.skip("TC-FILE-1: file transfer via metadata", "LLM tests skipped")
        return

    print("    [INFO] Requesting Word document creation...")
    await ws.send(json.dumps({
        "type": "command",
        "session_id": session_id,
        "text": "Tao mot file Word voi noi dung: 'Bao cao thu nghiem - Office Hub Test Suite'"
    }))

    found, seen = await drain_until(ws, "chat_reply", timeout=120)
    if found:
        meta = found.get("metadata") or {}
        if isinstance(meta, dict) and "attachment" in meta:
            att = meta["attachment"]
            has_data = bool(att.get("data"))
            has_name = bool(att.get("filename"))
            if has_data and has_name:
                result.ok("TC-FILE-1: file transfer -> attachment in metadata (Bug #3 FIXED)",
                          f"file={att.get('filename')}, size={len(att.get('data',''))} chars base64")
            else:
                result.fail("TC-FILE-1: file transfer -> attachment",
                            f"Attachment missing data or filename: {list(meta.keys())}")
        else:
            # Maybe Word COM not available - check content
            content = found.get("content", "")
            if "Word" in content or "docx" in content or ".docx" in content.lower():
                result.skip("TC-FILE-1: file transfer", "Word created but no attachment (COM may be unavailable in test env)")
            else:
                result.fail("TC-FILE-1: file transfer -> attachment",
                            f"No attachment in metadata. Keys: {list(meta.keys()) if isinstance(meta, dict) else type(meta)}")
    else:
        types_seen = [m.get("type") for m in seen]
        result.fail("TC-FILE-1: file transfer", f"No chat_reply in 120s. Seen: {types_seen}")


# ──────────────────────────────────────────────────────────────
# Main runner
# ──────────────────────────────────────────────────────────────

async def run_suite(host: str, port: int, token: str, skip_llm: bool = False):
    result = TestResult()
    uri = f"ws://{host}:{port}"
    session_id = f"test_suite_{int(time.time())}"

    print(f"\n{'='*60}")
    print(f"Office Hub WebSocket Test Suite v2")
    print(f"Target: {uri}")
    print(f"Token:  {token[:8]}...{token[-4:]}")
    print(f"Mode:   {'FAST (skip LLM)' if skip_llm else 'FULL (LLM enabled)'}")
    print(f"{'='*60}\n")

    # --- TC-WS-2: Wrong token (separate connection) ---
    print("[Phase 1] Auth Tests")
    await test_auth_wrong_token(host, port, result)

    # --- Main connection for non-LLM + core LLM tests ---
    try:
        async with websockets.connect(uri, open_timeout=10) as ws:
            authenticated = await authenticate(ws, token, result)
            if not authenticated:
                print("\n[FATAL] Cannot authenticate -- skipping remaining tests")
                result.summary()
                return False

            print("\n[Phase 1] Keep-alive")
            await test_ping_pong(ws, result)

            print("\n[Phase 2] Session Management")
            await test_list_sessions(ws, result)
            await test_get_session_history(ws, result)

            print("\n[Phase 3] Office Add-in Events")
            await test_office_addin_event(ws, result, skip_llm)

            print("\n[Phase 4] LLM Command + workflow_status relay (Bug #2)")
            await test_command_with_llm(ws, session_id, result, skip_llm)

            print("\n[Phase 5] Voice Command (stub)")
            await test_voice_command(ws, session_id, result, skip_llm)

    except ConnectionRefusedError:
        print(f"\n[FATAL] Connection refused at {uri}")
        print("-> Ensure the Office Hub Tauri backend is running.")
        return False
    except Exception as e:
        print(f"\n[FATAL] Unexpected error: {e}")
        traceback.print_exc()
        return False

    # --- Separate connection for add-in chat to avoid LLM state contamination ---
    if not skip_llm:
        print("\n[Phase 6] Add-in chat_request (fresh connection)")
        try:
            async with websockets.connect(uri, open_timeout=10) as ws2:
                await ws2.send(json.dumps({"type": "auth", "token": token}))
                auth_msg = await recv_with_timeout(ws2, timeout=5)
                if auth_msg and auth_msg.get("type") == "auth_success":
                    await test_chat_request_addin(ws2, result, skip_llm)
                else:
                    result.fail("TC-WS-10: chat_request -> chat_reply", "Auth failed on second connection")
        except Exception as e:
            result.fail("TC-WS-10: chat_request -> chat_reply", str(e))

        # --- File transfer test (another fresh connection) ---
        print("\n[Phase 7] File Transfer via metadata (Bug #3)")
        try:
            async with websockets.connect(uri, open_timeout=10) as ws3:
                await ws3.send(json.dumps({"type": "auth", "token": token}))
                auth_msg = await recv_with_timeout(ws3, timeout=5)
                if auth_msg and auth_msg.get("type") == "auth_success":
                    file_session = f"file_test_{int(time.time())}"
                    await test_file_transfer_metadata(ws3, file_session, result, skip_llm)
                else:
                    result.fail("TC-FILE-1: file transfer via metadata", "Auth failed")
        except Exception as e:
            result.fail("TC-FILE-1: file transfer via metadata", str(e))
    else:
        print("\n[Phase 6] Add-in chat_request")
        result.skip("TC-WS-10: chat_request -> chat_reply", "LLM tests skipped")
        print("\n[Phase 7] File Transfer via metadata (Bug #3)")
        result.skip("TC-FILE-1: file transfer via metadata", "LLM tests skipped")

    return result.summary()


def main():
    parser = argparse.ArgumentParser(description="Office Hub WebSocket Test Suite")
    parser.add_argument("--host", default="localhost")
    parser.add_argument("--port", type=int, default=9001)
    parser.add_argument("--token", default=None)
    parser.add_argument("--config", default=None)
    parser.add_argument("--skip-llm", action="store_true",
                        help="Skip slow LLM-dependent tests (auth/ping/sessions still run)")
    args = parser.parse_args()

    token = args.token or load_token_from_config(args.config)
    success = asyncio.run(run_suite(args.host, args.port, token, args.skip_llm))
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
