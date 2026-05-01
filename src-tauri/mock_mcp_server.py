import sys
import json
import logging
import os

logging.basicConfig(filename='mock_mcp_server.log', level=logging.DEBUG)

def main():
    logging.info("Starting mock MCP server")
    while True:
        line = sys.stdin.readline()
        if not line:
            logging.info("EOF reached")
            break
        line = line.strip()
        if not line:
            continue
            
        logging.info(f"Received: {line}")
        try:
            req = json.loads(line)
        except Exception as e:
            logging.error(f"JSON error: {e}")
            continue

        method = req.get("method")
        req_id = req.get("id")

        if not req_id:
            logging.info("Ignoring notification")
            continue

        resp = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": None,
            "error": None
        }

        if method == "initialize":
            resp["result"] = {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": { "listChanged": False }
                },
                "serverInfo": {
                    "name": "mock-mcp-server",
                    "version": "1.0.0"
                }
            }
        elif method == "tools/list":
            resp["result"] = {
                "tools": [
                    {
                        "name": "echo",
                        "description": "Echoes back the input",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "message": { "type": "string" }
                            },
                            "required": ["message"]
                        }
                    }
                ]
            }
        elif method == "tools/call":
            params = req.get("params", {})
            args = params.get("arguments", {})
            msg = args.get("message", "")
            resp["result"] = {
                "content": [
                    {
                        "type": "text",
                        "text": f"Mock Server Echo: {msg}"
                    }
                ],
                "isError": False
            }
        else:
            resp["error"] = {
                "code": -32601,
                "message": f"Method not found: {method}"
            }

        out = json.dumps(resp)
        logging.info(f"Sending: {out}")
        sys.stdout.write(out + "\n")
        sys.stdout.flush()

if __name__ == "__main__":
    main()
