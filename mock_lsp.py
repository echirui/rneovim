import sys
import json

def read_message():
    content_length = 0
    while True:
        line = sys.stdin.buffer.readline().decode('utf-8')
        if not line:
            return None
        if line.startswith("Content-Length:"):
            content_length = int(line.split(":")[1].strip())
        elif line == "\r\n":
            break
    
    if content_length > 0:
        body = sys.stdin.buffer.read(content_length).decode('utf-8')
        return json.loads(body)
    return None

def write_message(msg):
    body = json.dumps(msg)
    sys.stdout.write(f"Content-Length: {len(body)}\r\n\r\n{body}")
    sys.stdout.flush()

def main():
    while True:
        msg = read_message()
        if not msg:
            break
        
        method = msg.get("method")
        
        if method == "initialize":
            write_message({
                "jsonrpc": "2.0",
                "id": msg.get("id"),
                "result": {
                    "capabilities": {
                        "textDocumentSync": 1
                    }
                }
            })
        elif method == "textDocument/didOpen" or method == "textDocument/didChange":
            # Send a dummy diagnostic back
            uri = ""
            if method == "textDocument/didOpen":
                uri = msg["params"]["textDocument"]["uri"]
            else:
                uri = msg["params"]["textDocument"]["uri"]
                
            write_message({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": {
                    "uri": uri,
                    "diagnostics": [
                        {
                            "range": {
                                "start": {"line": 0, "character": 0},
                                "end": {"line": 0, "character": 5}
                            },
                            "severity": 1,
                            "message": "Mock Error Found!"
                        }
                    ]
                }
            })
        elif method == "shutdown":
            write_message({
                "jsonrpc": "2.0",
                "id": msg.get("id"),
                "result": None
            })
        elif method == "exit":
            break

if __name__ == "__main__":
    main()
