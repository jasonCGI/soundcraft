import io
import json
import tempfile
import threading
import unittest
import wave
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from bridge import generate

class BridgeTests(unittest.TestCase):
    def test_local_speech_contract(self):
        data = io.BytesIO()
        with wave.open(data, "wb") as wav:
            wav.setnchannels(1)
            wav.setsampwidth(2)
            wav.setframerate(24000)
            wav.writeframes(b"\x00\x00" * 240)
        observed = []
        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                observed.append((self.path, json.loads(self.rfile.read(int(self.headers["Content-Length"])))))
                self.send_response(200)
                self.end_headers()
                self.wfile.write(data.getvalue())
            def log_message(self, *args):
                pass
        server = HTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as folder:
                out = Path(folder) / "take.wav"
                generate({"input": "Hello", "voice": "af_heart"}, f"http://127.0.0.1:{server.server_port}", out)
                self.assertEqual(out.read_bytes(), data.getvalue())
                self.assertEqual(observed[0][0], "/v1/audio/speech")
                self.assertEqual(observed[0][1]["response_format"], "wav")
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
    def test_reject_remote_and_invalid_requests(self):
        for endpoint, settings in [("https://example.com", {"input": "hello"}), ("http://127.0.0.1:8800", {"input": ""}), ("http://127.0.0.1:8800", {"input": "x" * 5001}), ("http://127.0.0.1:8800", {"input": "hello", "speed": float("nan")})]:
            with self.subTest(endpoint=endpoint):
                with self.assertRaises(ValueError):
                    generate(settings, endpoint, "unused.wav")
    def test_reject_invalid_audio_and_provider_errors(self):
        class Handler(BaseHTTPRequestHandler):
            code = 200
            def do_POST(self):
                self.rfile.read(int(self.headers["Content-Length"]))
                self.send_response(self.code)
                self.end_headers()
                self.wfile.write(b"not a WAV")
            def log_message(self, *args):
                pass
        server = HTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as folder:
                out = Path(folder) / "take.wav"
                with self.assertRaises((wave.Error, EOFError)):
                    generate({"input":"hello"}, f"http://127.0.0.1:{server.server_port}", out)
                self.assertFalse(out.exists())
                Handler.code = 503
                with self.assertRaisesRegex(ValueError, "HTTP 503"):
                    generate({"input":"hello"}, f"http://127.0.0.1:{server.server_port}", out)
                self.assertFalse(out.exists())
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
if __name__ == "__main__":
    unittest.main()
