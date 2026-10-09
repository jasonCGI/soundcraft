import io
import json
import tempfile
import struct
import threading
import unittest
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from music_bridge import generate, validate, normalize_audio

class MusicTests(unittest.TestCase):
    def test_async_contract_download_and_no_overwrite(self):
        audio = io.BytesIO()
        with wave.open(audio, 'wb') as wav:
            wav.setparams((2,2,48000,0,'NONE','not compressed'))
            wav.writeframes(b'\0' * 480)
        submitted = []
        scenario = {"status":1,"file":"/v1/audio?path=test.wav","audio":audio.getvalue()}
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                if self.path == '/release_task':
                    submitted.append(body)
                    data = {'task_id':'test'}
                else:
                    data = [{'status':scenario['status'], 'result':json.dumps([{'file':scenario['file']}])}]
                self.send_response(200)
                self.end_headers()
                self.wfile.write(json.dumps({'code':200,'data':data}).encode())
            def do_GET(self):
                self.send_response(200)
                self.end_headers()
                self.wfile.write(scenario['audio'])
        server = ThreadingHTTPServer(('127.0.0.1',0),Handler)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            with tempfile.TemporaryDirectory() as folder:
                path = Path(folder)/'take.wav'
                settings = {'input':'Original ambient instrumental','seed':123}
                generate(settings, f'http://127.0.0.1:{server.server_port}', path)
                self.assertEqual(path.read_bytes(), audio.getvalue())
                self.assertEqual(submitted[0]['seed'],123)
                self.assertEqual(submitted[0]['lyrics'],'[Instrumental]')
                self.assertFalse(submitted[0]['thinking'])
                with self.assertRaises(FileExistsError):
                    generate(settings, f'http://127.0.0.1:{server.server_port}', path)
                for field, value in [('file','https://example.com/audio.wav'),('file','http://127.0.0.1:9/v1/audio?path=x'),('status',2),('audio',b'not audio')]:
                    original = scenario[field]
                    scenario[field] = value
                    rejected = Path(folder)/'rejected.wav'
                    with self.assertRaises((ValueError,wave.Error,EOFError)):
                        generate(settings, f'http://127.0.0.1:{server.server_port}', rejected)
                    self.assertFalse(rejected.exists())
                    scenario[field] = original
                scenario['status'] = 0
                with self.assertRaises(TimeoutError):
                    generate(settings, f'http://127.0.0.1:{server.server_port}', Path(folder)/'timeout.wav',timeout=0.02,poll_interval=0.01)
        finally:
            server.shutdown()
            server.server_close()
            worker.join()
    def test_float_wav_conversion_and_rejection(self):
        def floating(values):
            samples=struct.pack('<'+'f'*len(values),*values)
            fmt=struct.pack('<HHIIHH',3,1,48000,192000,4,32)
            chunks=b'fmt '+struct.pack('<I',len(fmt))+fmt+b'data'+struct.pack('<I',len(samples))+samples
            return b'RIFF'+struct.pack('<I',4+len(chunks))+b'WAVE'+chunks
        converted=normalize_audio(floating([0.0,0.5,-0.5,2.0]))
        with wave.open(io.BytesIO(converted),'rb') as wav:
            self.assertEqual(wav.getsampwidth(),2)
            self.assertEqual(struct.unpack('<hhhh',wav.readframes(4)),(0,16384,-16384,32767))
        with self.assertRaises(ValueError):
            normalize_audio(floating([float('nan')]))
        with self.assertRaises(ValueError):
            normalize_audio(floating([0.1])[:-1])
    def test_invalid_settings_before_network(self):
        for changed in [{'duration':float('nan')},{'duration':61},{'bpm':20},{'seed':-1},{'model':'other'},{'input':''}]:
            with self.assertRaises(ValueError):
                validate(dict(input='Ambient', **{k:v for k,v in changed.items() if k != 'input'}) if 'input' not in changed else changed, 'http://127.0.0.1:8001')
        self.assertEqual(validate({'input':'Warm piano','genre':'Ambient'}, 'http://127.0.0.1:8001')['prompt'],'Ambient, Warm piano, instrumental, no vocals')
        self.assertEqual(validate({'input':'Folk song','lyrics':'[Verse]\nAn original line'}, 'http://127.0.0.1:8001')['lyrics'],'[Verse]\nAn original line')
        with self.assertRaises(ValueError):
            validate({'input':'Ambient'}, 'https://example.com')

if __name__ == '__main__':
    unittest.main()
