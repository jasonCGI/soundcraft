"""Local ACE-Step music bridge, using the documented asynchronous API."""
import argparse
import array
import struct
import io
import json
import math
import sys
import time
import urllib.parse
import urllib.request
import wave
from pathlib import Path

MAX_AUDIO = 25 * 1024 * 1024
MAX_JSON = 1024 * 1024

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None

def validate(settings, endpoint):
    address = urllib.parse.urlsplit(endpoint)
    if address.scheme != 'http' or address.hostname != '127.0.0.1' or address.username or address.password or address.path not in ('', '/') or address.query or address.fragment:
        raise ValueError('Use a local ACE-Step base URL: http://127.0.0.1:PORT')
    prompt = settings.get('input', '')
    duration = settings.get('duration', 30)
    bpm = settings.get('bpm', 100)
    seed = settings.get('seed', 42)
    model = settings.get('model', 'acestep-v15-turbo')
    if not isinstance(prompt, str) or not prompt.strip() or len(prompt) > 2000:
        raise ValueError('Enter between 1 and 2000 prompt characters')
    if isinstance(duration, bool) or not isinstance(duration, (int, float)) or not math.isfinite(duration) or not 10 <= duration <= 60:
        raise ValueError('Duration must be between 10 and 60 seconds')
    if isinstance(bpm, bool) or not isinstance(bpm, int) or not 30 <= bpm <= 300:
        raise ValueError('Tempo must be between 30 and 300 BPM')
    if isinstance(seed, bool) or not isinstance(seed, int) or not 0 <= seed <= 2147483647:
        raise ValueError('Seed must be a nonnegative 32-bit integer')
    if model != 'acestep-v15-turbo':
        raise ValueError('This preview supports the ACE-Step turbo model')
    return dict(prompt=prompt, lyrics='[Instrumental]', audio_duration=duration, bpm=bpm,
                seed=seed, use_random_seed=False, model=model, thinking=False,
                use_cot_caption=False, use_cot_language=False, use_cot_metas=False,
                batch_size=1, inference_steps=8, audio_format='wav')

def normalize_audio(audio):
    """ACE-Step writes float WAVs; convert finite float32 samples to PCM16."""
    if len(audio) < 12 or audio[:4] != b'RIFF' or audio[8:12] != b'WAVE':
        raise ValueError('ACE-Step returned an invalid WAV')
    declared = struct.unpack_from('<I', audio, 4)[0] + 8
    if declared > len(audio):
        raise ValueError('ACE-Step returned truncated audio')
    fmt = None
    samples = None
    offset = 12
    while offset + 8 <= declared:
        kind = audio[offset:offset+4]
        size = struct.unpack_from('<I', audio, offset+4)[0]
        start = offset + 8
        end = start + size
        if end > declared:
            raise ValueError('ACE-Step returned a truncated WAV chunk')
        if kind == b'fmt ':
            fmt = audio[start:end]
        elif kind == b'data':
            samples = audio[start:end]
        offset = end + (size % 2)
    if fmt is None or len(fmt) < 16 or samples is None:
        raise ValueError('ACE-Step returned an incomplete WAV')
    encoding, channels, rate, _, alignment, bits = struct.unpack_from('<HHIIHH', fmt)
    if encoding != 3:
        return audio
    if channels not in (1,2) or not 8000 <= rate <= 192000 or bits != 32 or alignment != channels * 4 or len(samples) % alignment or not samples or len(samples) / alignment / rate > 65:
        raise ValueError('ACE-Step returned unsupported float audio')
    pcm = array.array('h')
    for (sample,) in struct.iter_unpack('<f', samples):
        if not math.isfinite(sample):
            raise ValueError('ACE-Step returned nonfinite audio samples')
        pcm.append(round(max(-1.0,min(1.0,sample)) * 32767))
    if sys.byteorder != 'little':
        pcm.byteswap()
    output = io.BytesIO()
    with wave.open(output,'wb') as wav:
        wav.setparams((channels,2,rate,0,'NONE','not compressed'))
        wav.writeframes(pcm.tobytes())
    return output.getvalue()

def generate(settings, endpoint, output, *, timeout=600, poll_interval=1):
    payload = validate(settings, endpoint)
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    base = endpoint.rstrip('/')
    def request(route, data=None, limit=MAX_JSON):
        encoded = None if data is None else json.dumps(data).encode()
        req = urllib.request.Request(base + route, data=encoded, headers={'Content-Type':'application/json'})
        with opener.open(req, timeout=30) as response:
            content = response.read(limit + 1)
        if len(content) > limit:
            raise ValueError('ACE-Step response exceeds the preview size limit')
        return content
    def api(route, data):
        result = json.loads(request(route, data))
        if not isinstance(result, dict) or result.get('code') != 200 or result.get('error'):
            raise ValueError('ACE-Step rejected the request')
        return result.get('data')
    task = api('/release_task', payload)
    if not isinstance(task, dict) or not isinstance(task.get('task_id'), str):
        raise ValueError('ACE-Step did not return a task ID')
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        results = api('/query_result', {'task_id_list':[task['task_id']]})
        if not isinstance(results, list) or not results or not isinstance(results[0], dict):
            raise ValueError('ACE-Step returned invalid task status')
        result = results[0]
        if result.get('status') == 2:
            raise ValueError('Music generation failed. Check the ACE-Step service log.')
        if result.get('status') == 1:
            takes = result.get('result')
            if isinstance(takes, str):
                takes = json.loads(takes)
            if not isinstance(takes, list) or not takes or not isinstance(takes[0], dict):
                raise ValueError('ACE-Step returned no audio take')
            audio_url = takes[0].get('file', '')
            if not isinstance(audio_url, str):
                raise ValueError('ACE-Step returned an invalid audio URL')
            address = urllib.parse.urlsplit(urllib.parse.urljoin(base + '/', audio_url))
            expected = urllib.parse.urlsplit(base)
            if (address.scheme, address.hostname, address.port) != (expected.scheme, expected.hostname, expected.port) or address.username or address.password or address.path != '/v1/audio' or address.fragment:
                raise ValueError('ACE-Step audio must come from the same local service')
            audio = normalize_audio(request('/v1/audio?' + address.query, limit=MAX_AUDIO))
            with wave.open(io.BytesIO(audio), 'rb') as wav:
                frames = wav.getnframes()
                if frames == 0 or wav.getnchannels() not in (1,2) or wav.getframerate() <= 0 or frames / wav.getframerate() > 65:
                    raise ValueError('ACE-Step returned empty or unsupported audio')
                if len(wav.readframes(frames)) != frames * wav.getnchannels() * wav.getsampwidth():
                    raise ValueError('ACE-Step returned truncated audio')
            target = Path(output)
            with target.open('xb') as destination:
                destination.write(audio)
            return
        if result.get('status') != 0:
            raise ValueError('ACE-Step returned an unknown task status')
        time.sleep(poll_interval)
    raise TimeoutError('Music generation timed out; the service may still finish its task')

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--endpoint', required=True)
    parser.add_argument('--out', required=True)
    args = parser.parse_args()
    try:
        generate(json.loads(sys.stdin.read(32000)), args.endpoint, args.out)
    except (ValueError, OSError, wave.Error, EOFError, TypeError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0

if __name__ == '__main__':
    sys.exit(main())
